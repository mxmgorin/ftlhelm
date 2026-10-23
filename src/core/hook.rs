//! Symbol resolution from the executable's .symtab, the trampoline page near the binary,
//! inline hooks, and the redirected step cap of CFPS::OnLoop.

use std::collections::HashMap;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::Mutex;

use super::util::log;

const EXE_ENV: &str = "FTLPAD_EXE";
const EXE_FALLBACK: &str = "/proc/self/exe";
const MAPS: &str = "/proc/self/maps";

const SHT_SYMTAB: u32 = 2;
const EHDR_SHOFF: u64 = 0x28;
const EHDR_SHENTSIZE: u64 = 0x3a;
const EHDR_SHNUM: u64 = 0x3c;
const SHDR_SIZE: usize = 64;
const SYM_SIZE: usize = 24;

const TRAMPOLINE_HINT: usize = 0x7000_0000;
const TRAMPOLINE_PAGE: usize = 4096;
/// Distance between candidate addresses when the trampoline page must be placed by search.
const TRAMPOLINE_STEP: usize = 0x100_0000;
const JMP_LEN: usize = 5;
const FAR_JMP_LEN: usize = 12;
const NOP: u8 = 0x90;

/// CFPS::OnLoop caps the step with "movss cap(%rip),%xmm1; unpcklpd %xmm0,%xmm0" within
/// this many bytes of its start.
const STEP_CAP_SCAN: usize = 0x100;
const STEP_CAP_LOAD: [u8; 4] = [0xf3, 0x0f, 0x10, 0x0d];
const STEP_CAP_NEXT: [u8; 4] = [0x66, 0x0f, 0x14, 0xc0];

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
    let path = std::env::var(EXE_ENV).unwrap_or_else(|_| EXE_FALLBACK.to_string());
    let mut found = vec![0usize; wanted.len()];
    match File::open(&path) {
        Ok(f) => {
            scan_symtab(&f, wanted, &mut found);
        }
        Err(_) => log!("cannot open the executable"),
    }
    found
}

fn scan_symtab(f: &File, wanted: &[&str], found: &mut [usize]) -> Option<()> {
    let index: HashMap<&[u8], usize> = wanted.iter().enumerate().map(|(i, n)| (n.as_bytes(), i)).collect();
    let shoff = read_u64(f, EHDR_SHOFF)?;
    if read_u16(f, EHDR_SHENTSIZE)? as usize != SHDR_SIZE {
        return None;
    }
    let shnum = read_u16(f, EHDR_SHNUM)? as usize;
    let sh = read_vec(f, shoff, shnum * SHDR_SIZE)?;
    let section = |i: usize| &sh[i * SHDR_SIZE..(i + 1) * SHDR_SIZE];
    for i in 0..shnum {
        let s = section(i);
        let link = le_u32(s, 40) as usize;
        if le_u32(s, 4) != SHT_SYMTAB || link >= shnum {
            continue;
        }
        let st = section(link);
        let strtab = read_vec(f, le_u64(st, 24), le_u64(st, 32) as usize)?;
        let syms = read_vec(f, le_u64(s, 24), le_u64(s, 32) as usize)?;
        for sym in syms.chunks_exact(SYM_SIZE) {
            let value = le_u64(sym, 8) as usize;
            let name_at = le_u32(sym, 0) as usize;
            if value == 0 || name_at >= strtab.len() {
                continue;
            }
            let name = &strtab[name_at..];
            let name = &name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())];
            if let Some(&w) = index.get(name) {
                if found[w] == 0 {
                    found[w] = value;
                }
            }
        }
    }
    Some(())
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

/// Writes "jmp rel32" at `at`.
/// # Safety
/// `at` must be writable for JMP_LEN bytes, with `to` within rel32 reach.
unsafe fn put_jmp(at: usize, to: usize) {
    let rel = (to as i64 - (at + JMP_LEN) as i64) as i32;
    let p = at as *mut u8;
    p.write(0xe9);
    (p.add(1) as *mut i32).write_unaligned(rel);
}

/// Writes "mov $to,%rax; jmp *%rax" at `at`.
/// # Safety
/// `at` must be writable for FAR_JMP_LEN bytes.
unsafe fn put_far_jmp(at: usize, to: usize) {
    let p = at as *mut u8;
    p.write(0x48);
    p.add(1).write(0xb8);
    (p.add(2) as *mut u64).write_unaligned(to as u64);
    p.add(10).write(0xff);
    p.add(11).write(0xe0);
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
