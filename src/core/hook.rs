//! Symbol resolution from the executable's .symtab, PLT slot redirection, and code patches
//! through a trampoline page mapped within rel32 reach of the executable: inline hooks, the
//! step cap of CFPS::OnLoop, and the widened pad test of the engine's joystick scan.

use std::collections::HashMap;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::Mutex;

use super::util::log;

const EXE_ENV: &str = "FTLPAD_EXE";
const EXE_FALLBACK: &str = "/proc/self/exe";
const MAPS: &str = "/proc/self/maps";

const SHT_SYMTAB: u32 = 2;
const SHT_RELA: u32 = 4;
const SHT_NOTE: u32 = 7;
const SHT_DYNSYM: u32 = 11;
const EHDR_SHOFF: u64 = 0x28;
const EHDR_SHENTSIZE: u64 = 0x3a;
const EHDR_SHNUM: u64 = 0x3c;
const SHDR_SIZE: usize = 64;
const SH_TYPE: usize = 4;
const SH_OFFSET: usize = 24;
const SH_SIZE: usize = 32;
const SH_LINK: usize = 40;
const SYM_SIZE: usize = 24;
const ST_NAME: usize = 0;
const ST_VALUE: usize = 8;
const RELA_SIZE: usize = 24;
const R_OFFSET: usize = 0;
const R_INFO: usize = 8;
/// r_info holds the relocation type below this bit and the symbol index from it up.
const R_SYM_SHIFT: u32 = 32;
const R_X86_64_JUMP_SLOT: u32 = 7;
const NOTE_NAMESZ: usize = 0;
const NOTE_DESCSZ: usize = 4;
const NOTE_TYPE: usize = 8;
const NOTE_HDR_SIZE: usize = NOTE_TYPE + std::mem::size_of::<u32>();
const NOTE_ALIGN: usize = 4;
const NT_GNU_BUILD_ID: u32 = 3;
const NOTE_OWNER_GNU: &[u8] = b"GNU\0";

const TRAMPOLINE_HINT: usize = 0x7000_0000;
const TRAMPOLINE_PAGE: usize = 4096;
/// Distance between candidate addresses when the trampoline page must be placed by search.
const TRAMPOLINE_STEP: usize = 0x100_0000;
const JMP_REL32: u8 = 0xe9;
const JMP_LEN: usize = 1 + std::mem::size_of::<i32>();
/// "mov $imm64,%rax" and "jmp *%rax".
const MOVABS_RAX: [u8; 2] = [0x48, 0xb8];
const JMP_RAX: [u8; 2] = [0xff, 0xe0];
const FAR_JMP_LEN: usize = MOVABS_RAX.len() + std::mem::size_of::<u64>() + JMP_RAX.len();
const NOP: u8 = 0x90;

/// CFPS::OnLoop caps the step with "movss cap(%rip),%xmm1; unpcklpd %xmm0,%xmm0" within
/// this many bytes of its start.
const STEP_CAP_SCAN: usize = 0x100;
const STEP_CAP_LOAD: [u8; 4] = [0xf3, 0x0f, 0x10, 0x0d];
const STEP_CAP_NEXT: [u8; 4] = [0x66, 0x0f, 0x14, 0xc0];

/// scan_joystick takes a device without ABS_X/ABS_Y only when it reports BTN_DPAD_UP,
/// tested as "testb $mask,keybits+BTN_DPAD_UP/8(%rax); je reject" within this many bytes.
const PAD_TEST_SCAN: usize = 0x200;
const BTN_DPAD_UP: i64 = 0x220;
const BTN_SOUTH: i64 = 0x130;
const TESTB_ABS: [u8; 2] = [0xf6, 0x80];
const JE_REL32: [u8; 2] = [0x0f, 0x84];
const TESTB_LEN: usize = 7;
const JE_LEN: usize = 6;
const JNZ_REL8: u8 = 0x75;
const JNZ_LEN: usize = 2;

/// The trampoline page and how much of it is taken, as addresses.
static TRAMPOLINES: Mutex<(usize, usize)> = Mutex::new((0, 0));

fn read_u16(f: &File, at: u64) -> Option<u16> {
    let mut b = [0u8; 2];
    f.read_exact_at(&mut b, at).ok()?;
    Some(u16::from_le_bytes(b))
}

fn read_u64(f: &File, at: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    f.read_exact_at(&mut b, at).ok()?;
    Some(u64::from_le_bytes(b))
}

/// Reads `len` bytes at `at`; None when they run past the end of the file.
fn read_vec(f: &File, at: u64, len: usize) -> Option<Vec<u8>> {
    if at.checked_add(len as u64)? > f.metadata().ok()?.len() {
        return None;
    }
    let mut v = vec![0u8; len];
    f.read_exact_at(&mut v, at).ok()?;
    Some(v)
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("slice of four bytes"))
}

fn le_u64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("slice of eight bytes"))
}

/// Looks up the wanted symbols in the executable's .symtab; returns their addresses in
/// order, 0 for each one missing.
pub fn resolve_symbols(wanted: &[&str]) -> Vec<usize> {
    let mut found = vec![0usize; wanted.len()];
    match open_exe() {
        Some(f) => {
            scan_symtab(&f, wanted, &mut found);
        }
        None => log!("cannot open the executable"),
    }
    found
}

/// Reads the executable's GNU build ID; None when it has none or cannot be read.
pub fn build_id() -> Option<Vec<u8>> {
    read_build_id(&open_exe()?)
}

/// Reads an ELF file's GNU build ID; None when it has none.
fn read_build_id(f: &File) -> Option<Vec<u8>> {
    Sections::read(f)?.of_kind(SHT_NOTE).find_map(|s| find_build_id(&s.data(f)?))
}

fn open_exe() -> Option<File> {
    let path = std::env::var(EXE_ENV).unwrap_or_else(|_| EXE_FALLBACK.to_string());
    File::open(path).ok()
}

/// An ELF file's section header table.
struct Sections(Vec<u8>);

/// One entry of the section header table.
#[derive(Clone, Copy)]
struct Section<'a>(&'a [u8]);

impl Sections {
    fn read(f: &File) -> Option<Sections> {
        let shoff = read_u64(f, EHDR_SHOFF)?;
        if read_u16(f, EHDR_SHENTSIZE)? as usize != SHDR_SIZE {
            return None;
        }
        let shnum = read_u16(f, EHDR_SHNUM)? as usize;
        read_vec(f, shoff, shnum * SHDR_SIZE).map(Sections)
    }

    /// Section `i`; None past the table.
    fn get(&self, i: usize) -> Option<Section<'_>> {
        self.0.get(i * SHDR_SIZE..(i + 1) * SHDR_SIZE).map(Section)
    }

    fn of_kind(&self, kind: u32) -> impl Iterator<Item = Section<'_>> {
        self.0.as_chunks::<SHDR_SIZE>().0.iter().map(|s| Section(&s[..])).filter(move |s| s.kind() == kind)
    }
}

impl Section<'_> {
    fn kind(self) -> u32 {
        le_u32(self.0, SH_TYPE)
    }

    /// The index of the section this one links to.
    fn link(self) -> usize {
        le_u32(self.0, SH_LINK) as usize
    }

    /// Reads the section's contents; None when they run past the end of the file.
    fn data(self, f: &File) -> Option<Vec<u8>> {
        read_vec(f, le_u64(self.0, SH_OFFSET), le_u64(self.0, SH_SIZE) as usize)
    }
}

/// The string at `at` of a string table, up to its terminator; None past the table.
fn table_str(table: &[u8], at: usize) -> Option<&[u8]> {
    table.get(at..)?.split(|&c| c == 0).next()
}

/// Walks a note section; returns the descriptor of its GNU build ID note.
fn find_build_id(notes: &[u8]) -> Option<Vec<u8>> {
    let align = |n: usize| n.next_multiple_of(NOTE_ALIGN);
    let mut at = 0;
    while at + NOTE_HDR_SIZE <= notes.len() {
        let namesz = le_u32(notes, at + NOTE_NAMESZ) as usize;
        let descsz = le_u32(notes, at + NOTE_DESCSZ) as usize;
        let kind = le_u32(notes, at + NOTE_TYPE);
        let name_at = at + NOTE_HDR_SIZE;
        let desc_at = name_at + align(namesz);
        let end = desc_at + align(descsz);
        if desc_at + descsz > notes.len() {
            return None;
        }
        if kind == NT_GNU_BUILD_ID && &notes[name_at..name_at + namesz] == NOTE_OWNER_GNU {
            return Some(notes[desc_at..desc_at + descsz].to_vec());
        }
        at = end;
    }
    None
}

fn scan_symtab(f: &File, wanted: &[&str], found: &mut [usize]) -> Option<()> {
    let index: HashMap<&[u8], usize> = wanted.iter().enumerate().map(|(i, n)| (n.as_bytes(), i)).collect();
    let sections = Sections::read(f)?;
    for symtab in sections.of_kind(SHT_SYMTAB) {
        let Some(strtab) = sections.get(symtab.link()) else {
            continue;
        };
        let names = strtab.data(f)?;
        let syms = symtab.data(f)?;
        for sym in syms.as_chunks::<SYM_SIZE>().0 {
            let value = le_u64(sym, ST_VALUE) as usize;
            if value == 0 {
                continue;
            }
            let Some(name) = table_str(&names, le_u32(sym, ST_NAME) as usize) else {
                continue;
            };
            if let Some(&w) = index.get(name) {
                if found[w] == 0 {
                    found[w] = value;
                }
            }
        }
    }
    Some(())
}

/// Finds the PLT slot through which the executable calls the library function `name`;
/// returns the slot's address.
pub fn plt_slot(name: &str) -> Option<usize> {
    find_plt_slot(&open_exe()?, name)
}

/// Finds the PLT slot through which an ELF file calls the library function `name`;
/// returns the slot's address.
fn find_plt_slot(f: &File, name: &str) -> Option<usize> {
    let sections = Sections::read(f)?;
    for rela_table in sections.of_kind(SHT_RELA) {
        let Some(dynsym) = sections.get(rela_table.link()).filter(|s| s.kind() == SHT_DYNSYM) else {
            continue;
        };
        let Some(strtab) = sections.get(dynsym.link()) else {
            continue;
        };
        let names = strtab.data(f)?;
        let syms = dynsym.data(f)?;
        let relas = rela_table.data(f)?;
        for rela in relas.as_chunks::<RELA_SIZE>().0 {
            let info = le_u64(rela, R_INFO);
            if info as u32 != R_X86_64_JUMP_SLOT {
                continue;
            }
            let sym_at = (info >> R_SYM_SHIFT) as usize * SYM_SIZE;
            let Some(sym) = syms.get(sym_at..sym_at + SYM_SIZE) else {
                continue;
            };
            if table_str(&names, le_u32(sym, ST_NAME) as usize) == Some(name.as_bytes()) {
                return Some(le_u64(rela, R_OFFSET) as usize);
            }
        }
    }
    None
}

/// Points the PLT slot at `slot` to `to`; returns the address it held, or None when the
/// slot cannot be made writable.
pub fn redirect_slot(slot: usize, to: usize) -> Option<usize> {
    if !protect(slot, std::mem::size_of::<usize>(), libc::PROT_READ | libc::PROT_WRITE) {
        return None;
    }
    // SAFETY: slot is a pointer-aligned PLT entry of the executable, writable now.
    Some(unsafe { (slot as *mut usize).replace(to) })
}

/// Reads the address the PLT slot at `slot` holds.
pub fn slot_target(slot: usize) -> usize {
    // SAFETY: slot is a PLT entry of the executable, mapped for the process's life.
    unsafe { (slot as *const usize).read() }
}

/// Checks that addr lies in an executable mapping of this process; a preloaded library
/// also lands in the shell or wrapper that launches the game.
pub fn mapped_executable(addr: usize) -> bool {
    let Ok(maps) = std::fs::read_to_string(MAPS) else {
        return false;
    };
    maps.lines().any(|line| {
        let mut parts = line.split_whitespace();
        let (Some(range), Some(perms)) = (parts.next(), parts.next()) else {
            return false;
        };
        let Some((lo, hi)) = range.split_once('-') else {
            return false;
        };
        let (Ok(lo), Ok(hi)) = (usize::from_str_radix(lo, 16), usize::from_str_radix(hi, 16)) else {
            return false;
        };
        perms.as_bytes().get(2) == Some(&b'x') && lo <= addr && addr < hi
    })
}

fn protect(at: usize, len: usize, prot: libc::c_int) -> bool {
    // SAFETY: sysconf has no preconditions.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
    let start = at & !(page - 1);
    let end = (at + len + page - 1) & !(page - 1);
    // SAFETY: the range covers mapped code of this process.
    unsafe { libc::mprotect(start as *mut libc::c_void, end - start, prot) == 0 }
}

fn fits_rel32(from: usize, to: usize) -> bool {
    let d = to as i64 - from as i64;
    d >= i32::MIN as i64 && d <= i32::MAX as i64
}

/// Writes `parts` back to back at `at`.
/// # Safety
/// `at` must be writable for the parts' total length.
unsafe fn put_code(at: usize, parts: &[&[u8]]) {
    let mut p = at as *mut u8;
    for part in parts {
        std::ptr::copy_nonoverlapping(part.as_ptr(), p, part.len());
        p = p.add(part.len());
    }
}

/// Writes "jmp rel32" at `at`.
/// # Safety
/// `at` must be writable for JMP_LEN bytes, with `to` within rel32 reach.
unsafe fn put_jmp(at: usize, to: usize) {
    let rel = (to as i64 - (at + JMP_LEN) as i64) as i32;
    put_code(at, &[&[JMP_REL32], &rel.to_le_bytes()]);
}

/// Writes "mov $to,%rax; jmp *%rax" at `at`.
/// # Safety
/// `at` must be writable for FAR_JMP_LEN bytes.
unsafe fn put_far_jmp(at: usize, to: usize) {
    put_code(at, &[&MOVABS_RAX, &(to as u64).to_le_bytes(), &JMP_RAX]);
}

fn map_trampolines(at: usize, flags: libc::c_int) -> Option<usize> {
    // SAFETY: an anonymous private mapping touches nothing else.
    let page = unsafe {
        libc::mmap(
            at as *mut libc::c_void,
            TRAMPOLINE_PAGE,
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | flags,
            -1,
            0,
        )
    };
    (page != libc::MAP_FAILED).then_some(page as usize)
}

fn unmap(page: usize) {
    // SAFETY: page is a mapping made by map_trampolines.
    unsafe { libc::munmap(page as *mut libc::c_void, TRAMPOLINE_PAGE) };
}

/// Maps the trampoline page within a rel32 jump of `near`: at the hint, else at the first
/// free step around it.
fn place_trampolines(near: usize) -> Option<usize> {
    if let Some(page) = map_trampolines(TRAMPOLINE_HINT, 0) {
        if fits_rel32(near, page) {
            return Some(page);
        }
        unmap(page);
    }
    let base = near & !(TRAMPOLINE_STEP - 1);
    let mut d = TRAMPOLINE_STEP;
    while d < i32::MAX as usize / 2 {
        for candidate in [base.checked_add(d), base.checked_sub(d)].into_iter().flatten() {
            // Kernels without MAP_FIXED_NOREPLACE take the address as a hint.
            if let Some(page) = map_trampolines(candidate, libc::MAP_FIXED_NOREPLACE) {
                if fits_rel32(near, page) {
                    return Some(page);
                }
                unmap(page);
            }
        }
        d += TRAMPOLINE_STEP;
    }
    None
}

/// Takes `len` bytes of the trampoline page, aligned to `align`, within rel32 reach of
/// `near`; places the page on first use.
fn take_trampoline(near: usize, len: usize, align: usize) -> Option<usize> {
    let mut t = TRAMPOLINES.lock().unwrap_or_else(|e| e.into_inner());
    if t.0 == 0 {
        t.0 = place_trampolines(near)?;
    }
    let at = (t.1 + align - 1) & !(align - 1);
    if at + len > TRAMPOLINE_PAGE || !fits_rel32(near, t.0 + at) {
        return None;
    }
    t.1 = at + len;
    Some(t.0 + at)
}

/// Diverts the function at `func` to `repl` after checking its first bytes; returns an
/// entry that runs the original, or None with the reason logged under `name`. The
/// prologue must be whole instructions with no relative operands.
pub fn hook(name: &str, func: usize, repl: usize, prologue: &[u8]) -> Option<usize> {
    // SAFETY: func is a resolved function of the game; reading its prologue is in bounds.
    let current = unsafe { std::slice::from_raw_parts(func as *const u8, prologue.len()) };
    if prologue.len() < JMP_LEN || current != prologue {
        log!("{name}: prologue differs");
        return None;
    }
    let Some(entry) = take_trampoline(func, FAR_JMP_LEN + prologue.len() + JMP_LEN, 1) else {
        log!("{name}: no trampoline room within reach");
        return None;
    };
    let orig = entry + FAR_JMP_LEN;
    // SAFETY: the entry lies in the writable trampoline page and reaches func's body.
    unsafe {
        put_far_jmp(entry, repl);
        std::ptr::copy_nonoverlapping(prologue.as_ptr(), orig as *mut u8, prologue.len());
        put_jmp(orig + prologue.len(), func + prologue.len());
    }
    if !protect(func, prologue.len(), libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC) {
        log!("{name}: code not writable");
        return None;
    }
    // SAFETY: the prologue bytes were just made writable; the game is not running yet.
    unsafe {
        put_jmp(func, entry);
        std::ptr::write_bytes((func + JMP_LEN) as *mut u8, NOP, prologue.len() - JMP_LEN);
    }
    protect(func, prologue.len(), libc::PROT_READ | libc::PROT_EXEC);
    Some(orig)
}

fn bit_mask(code: i64) -> u8 {
    1 << (code % 8)
}

/// Makes scan_joystick also take a device reporting BTN_SOUTH, a pad with neither sticks
/// nor D-pad buttons; returns false with the reason logged when the test is not found.
pub fn accept_buttons_only_pads(scan: usize) -> bool {
    let site_len = TESTB_LEN + JE_LEN;
    let mask = bit_mask(BTN_DPAD_UP);
    // SAFETY: the scan stays within the start of scan_joystick's code.
    let code = unsafe { std::slice::from_raw_parts(scan as *const u8, PAD_TEST_SCAN) };
    let mut sites = (0..=PAD_TEST_SCAN - site_len).filter(|&i| {
        code[i..i + TESTB_ABS.len()] == TESTB_ABS
            && code[i + TESTB_LEN - 1] == mask
            && code[i + TESTB_LEN..i + TESTB_LEN + JE_REL32.len()] == JE_REL32
    });
    let Some(site) = sites.next() else {
        log!("pad scan: test not found");
        return false;
    };
    if sites.next().is_some() {
        log!("pad scan: test not unique");
        return false;
    }
    let at = scan + site;
    let dpad_disp = le_u32(code, site + TESTB_ABS.len()) as i32;
    let je_rel = le_u32(code, site + TESTB_LEN + JE_REL32.len()) as i32;
    let reject = (at as i64 + site_len as i64 + je_rel as i64) as usize;
    let Ok(south_disp) = i32::try_from(dpad_disp as i64 - BTN_DPAD_UP / 8 + BTN_SOUTH / 8) else {
        log!("pad scan: key bitmap out of reach");
        return false;
    };

    // testb dpad; jnz accept; testb south; jnz accept; jmp reject; accept: jmp back.
    let tests_len = 2 * (TESTB_LEN + JNZ_LEN);
    let Some(t) = take_trampoline(at, tests_len + 2 * JMP_LEN, 1) else {
        log!("pad scan: no trampoline room within reach");
        return false;
    };
    let accept = t + tests_len + JMP_LEN;
    // SAFETY: t lies in the writable trampoline page with the whole sequence taken.
    unsafe {
        let mut p = t;
        for (disp, bit) in [(dpad_disp, mask), (south_disp, bit_mask(BTN_SOUTH))] {
            std::ptr::copy_nonoverlapping(TESTB_ABS.as_ptr(), p as *mut u8, TESTB_ABS.len());
            ((p + TESTB_ABS.len()) as *mut i32).write_unaligned(disp);
            ((p + TESTB_LEN - 1) as *mut u8).write(bit);
            p += TESTB_LEN;
            (p as *mut u8).write(JNZ_REL8);
            ((p + 1) as *mut u8).write((accept - (p + JNZ_LEN)) as u8);
            p += JNZ_LEN;
        }
        put_jmp(p, reject);
        put_jmp(accept, at + site_len);
    }
    if !protect(at, site_len, libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC) {
        log!("pad scan: code not writable");
        return false;
    }
    // SAFETY: the test's bytes were just made writable; the input scan has not run yet.
    unsafe {
        put_jmp(at, t);
        std::ptr::write_bytes((at + JMP_LEN) as *mut u8, NOP, site_len - JMP_LEN);
    }
    protect(at, site_len, libc::PROT_READ | libc::PROT_EXEC);
    true
}

/// Points CFPS::OnLoop's step cap load at a float in the trampoline page, so the fast
/// speed can raise the cap; returns that float's address, or None with the reason logged.
pub fn redirect_step_cap(fps_loop: usize) -> Option<usize> {
    let insn = STEP_CAP_LOAD.len() + std::mem::size_of::<i32>();
    // SAFETY: the scan stays within the start of CFPS::OnLoop's code.
    let code = unsafe { std::slice::from_raw_parts(fps_loop as *const u8, STEP_CAP_SCAN) };
    let mut sites = (0..=STEP_CAP_SCAN - insn - STEP_CAP_NEXT.len()).filter(|&i| {
        code[i..i + STEP_CAP_LOAD.len()] == STEP_CAP_LOAD
            && code[i + insn..i + insn + STEP_CAP_NEXT.len()] == STEP_CAP_NEXT
    });
    let Some(site) = sites.next() else {
        log!("step cap: load not found");
        return None;
    };
    if sites.next().is_some() {
        log!("step cap: load not unique");
        return None;
    }
    let disp_at = fps_loop + site + STEP_CAP_LOAD.len();
    let next = fps_loop + site + insn;
    // SAFETY: disp_at holds the rel32 of the load, inside the code read above.
    let disp = unsafe { (disp_at as *const i32).read_unaligned() };
    let cap = (next as i64 + disp as i64) as usize;
    let Some(redirected) = take_trampoline(next, std::mem::size_of::<f32>(), std::mem::align_of::<f32>()) else {
        log!("step cap: no room near the code");
        return None;
    };
    // SAFETY: cap is the game's constant; redirected lies in the writable trampoline page.
    unsafe { (redirected as *mut f32).write((cap as *const f32).read()) };
    if !protect(disp_at, std::mem::size_of::<i32>(), libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC) {
        log!("step cap: code not writable");
        return None;
    }
    // SAFETY: the displacement bytes were just made writable.
    unsafe { (disp_at as *mut i32).write_unaligned((redirected as i64 - next as i64) as i32) };
    protect(disp_at, std::mem::size_of::<i32>(), libc::PROT_READ | libc::PROT_EXEC);
    Some(redirected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::fd::FromRawFd;

    // ELF64 values the reader itself never needs.
    const EHDR_SIZE: usize = 64;
    const SHT_STRTAB: u32 = 3;
    const R_X86_64_GLOB_DAT: u32 = 6;
    const NT_GNU_ABI_TAG: u32 = 1;
    const BUILD_ID_LEN: usize = 20;

    fn put(buf: &mut [u8], at: usize, bytes: &[u8]) {
        buf[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// An in-memory file holding `bytes`.
    fn memfile(bytes: &[u8]) -> File {
        // SAFETY: memfd_create with a NUL-terminated name.
        let fd = unsafe { libc::memfd_create(c"ftlpad-test".as_ptr(), 0) };
        assert!(fd >= 0, "memfd_create failed");
        // SAFETY: a fresh descriptor the File now owns.
        let mut f = unsafe { File::from_raw_fd(fd) };
        f.write_all(bytes).expect("a memfd takes writes");
        f
    }

    /// An ELF image built section by section: the data after the header, the section
    /// headers last, a null section first.
    struct Elf {
        data: Vec<u8>,
        headers: Vec<[u8; SHDR_SIZE]>,
    }

    impl Elf {
        fn new() -> Elf {
            Elf { data: vec![0; EHDR_SIZE], headers: vec![[0; SHDR_SIZE]] }
        }

        /// Adds a section; returns its index.
        fn section(&mut self, kind: u32, link: usize, bytes: &[u8]) -> usize {
            let mut h = [0u8; SHDR_SIZE];
            put(&mut h, SH_TYPE, &kind.to_le_bytes());
            put(&mut h, SH_OFFSET, &(self.data.len() as u64).to_le_bytes());
            put(&mut h, SH_SIZE, &(bytes.len() as u64).to_le_bytes());
            put(&mut h, SH_LINK, &(link as u32).to_le_bytes());
            self.data.extend_from_slice(bytes);
            self.headers.push(h);
            self.headers.len() - 1
        }

        fn file(mut self) -> File {
            let shoff = self.data.len() as u64;
            put(&mut self.data, EHDR_SHOFF as usize, &shoff.to_le_bytes());
            put(&mut self.data, EHDR_SHENTSIZE as usize, &(SHDR_SIZE as u16).to_le_bytes());
            put(&mut self.data, EHDR_SHNUM as usize, &(self.headers.len() as u16).to_le_bytes());
            for h in &self.headers {
                self.data.extend_from_slice(h);
            }
            memfile(&self.data)
        }
    }

    /// A string table of `names`; returns it with each name's offset.
    fn strtab(names: &[&str]) -> (Vec<u8>, Vec<u32>) {
        let mut table = vec![0u8];
        let offsets = names
            .iter()
            .map(|n| {
                let at = table.len() as u32;
                table.extend_from_slice(n.as_bytes());
                table.push(0);
                at
            })
            .collect();
        (table, offsets)
    }

    fn symbols(entries: &[(u32, u64)]) -> Vec<u8> {
        let mut table = vec![0u8; SYM_SIZE];
        for &(name, value) in entries {
            let mut sym = [0u8; SYM_SIZE];
            put(&mut sym, ST_NAME, &name.to_le_bytes());
            put(&mut sym, ST_VALUE, &value.to_le_bytes());
            table.extend_from_slice(&sym);
        }
        table
    }

    fn relas(entries: &[(u64, u32, u32)]) -> Vec<u8> {
        let mut table = Vec::new();
        for &(offset, sym, kind) in entries {
            let mut rela = [0u8; RELA_SIZE];
            put(&mut rela, R_OFFSET, &offset.to_le_bytes());
            put(&mut rela, R_INFO, &(((sym as u64) << R_SYM_SHIFT) | kind as u64).to_le_bytes());
            table.extend_from_slice(&rela);
        }
        table
    }

    fn note(owner: &[u8], kind: u32, desc: &[u8]) -> Vec<u8> {
        let mut n = Vec::new();
        n.extend_from_slice(&(owner.len() as u32).to_le_bytes());
        n.extend_from_slice(&(desc.len() as u32).to_le_bytes());
        n.extend_from_slice(&kind.to_le_bytes());
        for part in [owner, desc] {
            n.extend_from_slice(part);
            n.resize(n.len().next_multiple_of(NOTE_ALIGN), 0);
        }
        n
    }

    #[test]
    fn the_symbol_table_gives_each_wanted_symbol_its_first_address() {
        let (names, at) = strtab(&["wanted_a", "other", "wanted_b"]);
        let mut elf = Elf::new();
        let strings = elf.section(SHT_STRTAB, 0, &names);
        let past_strings = names.len() as u32;
        let syms = symbols(&[(at[0], 0x1000), (at[1], 0x2000), (at[2], 0), (at[0], 0x3000), (past_strings, 0x4000)]);
        elf.section(SHT_SYMTAB, strings + 99, &syms);
        elf.section(SHT_SYMTAB, strings, &syms);
        let mut found = [0; 3];
        scan_symtab(&elf.file(), &["wanted_a", "wanted_b", "missing"], &mut found);
        assert_eq!(found, [0x1000, 0, 0]);
    }

    #[test]
    fn a_plt_slot_is_the_jump_slot_of_the_named_symbol() {
        let (names, at) = strtab(&["snd_pcm_open", "snd_pcm_recover"]);
        let mut elf = Elf::new();
        let strings = elf.section(SHT_STRTAB, 0, &names);
        let symtab = elf.section(SHT_SYMTAB, strings, &symbols(&[(at[0], 0)]));
        elf.section(SHT_RELA, symtab, &relas(&[(0x6000, 1, R_X86_64_JUMP_SLOT)]));
        let dynsym = elf.section(SHT_DYNSYM, strings, &symbols(&[(at[0], 0), (at[1], 0)]));
        let relocs = relas(&[
            (0x5000, 2, R_X86_64_JUMP_SLOT),
            (0x5008, 1, R_X86_64_GLOB_DAT),
            (0x5010, 1, R_X86_64_JUMP_SLOT),
            (0x5018, 9, R_X86_64_JUMP_SLOT),
        ]);
        elf.section(SHT_RELA, dynsym, &relocs);
        let f = elf.file();
        assert_eq!(find_plt_slot(&f, "snd_pcm_recover"), Some(0x5000));
        assert_eq!(find_plt_slot(&f, "snd_pcm_open"), Some(0x5010));
        assert_eq!(find_plt_slot(&f, "snd_pcm_close"), None);
    }

    #[test]
    fn the_build_id_is_the_gnu_build_id_note() {
        let id = [0xab; BUILD_ID_LEN];
        let mut notes = note(b"XYZ\0", NT_GNU_BUILD_ID, &[1, 2, 3]);
        notes.extend(note(NOTE_OWNER_GNU, NT_GNU_ABI_TAG, &[0; 16]));
        notes.extend(note(NOTE_OWNER_GNU, NT_GNU_BUILD_ID, &id));
        let mut elf = Elf::new();
        elf.section(SHT_NOTE, 0, &notes);
        assert_eq!(read_build_id(&elf.file()), Some(id.to_vec()));
        let mut cut = note(NOTE_OWNER_GNU, NT_GNU_BUILD_ID, &id);
        cut.truncate(cut.len() - 1);
        let mut elf = Elf::new();
        elf.section(SHT_NOTE, 0, &cut);
        assert_eq!(read_build_id(&elf.file()), None);
        assert_eq!(read_build_id(&Elf::new().file()), None);
    }

    #[no_mangle]
    extern "C" fn ftlpad_test_marker_a() -> i32 {
        1
    }

    #[no_mangle]
    extern "C" fn ftlpad_test_marker_b() -> i32 {
        2
    }

    /// The markers' link-time addresses in this binary's .symtab, and their runtime ones.
    fn markers(f: &File) -> ([usize; 2], [usize; 2]) {
        let mut found = [0; 2];
        scan_symtab(f, &["ftlpad_test_marker_a", "ftlpad_test_marker_b"], &mut found);
        assert!(found.iter().all(|&a| a != 0), "markers missing from .symtab: {found:x?}");
        (found, [ftlpad_test_marker_a as *const () as usize, ftlpad_test_marker_b as *const () as usize])
    }

    #[test]
    fn this_binary_resolves_its_own_symbols_at_one_load_bias() {
        let (linked, runtime) = markers(&File::open(EXE_FALLBACK).expect("the test binary is readable"));
        assert_eq!(runtime[0].wrapping_sub(linked[0]), runtime[1].wrapping_sub(linked[1]));
    }

    /// Library functions a Rust test binary calls through its PLT.
    const PLT_CALLEES: [&std::ffi::CStr; 3] = [c"_Unwind_Resume", c"memcpy", c"memset"];

    #[test]
    fn this_binarys_plt_slots_hold_what_the_loader_bound() {
        let f = File::open(EXE_FALLBACK).expect("the test binary is readable");
        let (linked, runtime) = markers(&f);
        let bias = runtime[0].wrapping_sub(linked[0]);
        let mut checked = 0;
        for name in PLT_CALLEES {
            let Some(slot) = find_plt_slot(&f, name.to_str().expect("an ASCII name")) else {
                continue;
            };
            // SAFETY: dlsym with a NUL-terminated name.
            let bound = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) } as usize;
            assert_eq!(slot_target(bias.wrapping_add(slot)), bound, "{name:?}");
            checked += 1;
        }
        assert!(checked > 0, "none of {PLT_CALLEES:?} has a PLT slot");
    }

    const MOV_EAX_IMM32: u8 = 0xb8;
    const RET: u8 = 0xc3;
    const ORIGINAL_RESULT: i32 = 42;
    const REPLACEMENT_RESULT: i32 = 7;
    /// The mov: one whole instruction with no relative operand.
    const PROLOGUE_LEN: usize = 1 + std::mem::size_of::<i32>();

    extern "C" fn replacement() -> i32 {
        REPLACEMENT_RESULT
    }

    fn call(addr: usize) -> i32 {
        // SAFETY: addr holds code of an extern "C" fn() -> i32.
        unsafe { std::mem::transmute::<usize, extern "C" fn() -> i32>(addr)() }
    }

    #[test]
    fn a_hook_runs_the_replacement_and_its_entry_runs_the_original() {
        let mut code = vec![MOV_EAX_IMM32];
        code.extend_from_slice(&ORIGINAL_RESULT.to_le_bytes());
        code.push(RET);
        // SAFETY: sysconf has no preconditions.
        let len = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        // SAFETY: an anonymous private mapping touches nothing else.
        let page = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert_ne!(page, libc::MAP_FAILED);
        let func = page as usize;
        // SAFETY: the page is writable and longer than the code.
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), page as *mut u8, code.len()) };
        assert!(protect(func, code.len(), libc::PROT_READ | libc::PROT_EXEC));
        assert_eq!(call(func), ORIGINAL_RESULT);
        let orig =
            hook("test", func, replacement as *const () as usize, &code[..PROLOGUE_LEN]).expect("the prologue matches");
        assert_eq!(call(func), REPLACEMENT_RESULT);
        assert_eq!(call(orig), ORIGINAL_RESULT);
        assert_eq!(hook("test", func, replacement as *const () as usize, &code[..PROLOGUE_LEN]), None);
    }
}
