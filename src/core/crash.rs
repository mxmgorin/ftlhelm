//! The crash report: `crash:` lines in the log naming the fatal signal, the faulting code and
//! the ftlhelm hook running, before the handler the game had in place runs as before.
//!
//! The handler reads the registers and what install() gathered, nothing else in memory, and
//! writes through log_unlocked, so it neither faults nor waits on a lock itself; it runs on
//! its own stack, so an overflowed one reaches it too. It goes in on the first frame: the
//! game's `main` replaces the handlers set at load. A Rust panic logs its place through the
//! panic hook, and the abort that follows reaches the report.

use std::ffi::{c_int, c_void};
use std::ops::Range;
use std::sync::atomic::{compiler_fence, AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;

use super::hook::mapping_of;
use super::util::{log, log_unlocked};

/// The handler's stack: room for the report's formatting over the kernel's signal frame.
const ALT_STACK_LEN: usize = 64 << 10;

/// The signals reported, by name.
const SIGNALS: [(c_int, &str); 5] = [
    (libc::SIGSEGV, "SIGSEGV"),
    (libc::SIGBUS, "SIGBUS"),
    (libc::SIGILL, "SIGILL"),
    (libc::SIGFPE, "SIGFPE"),
    (libc::SIGABRT, "SIGABRT"),
];

/// The general registers reported, one line per row.
#[rustfmt::skip]
const REGISTERS: [[(c_int, &str); 4]; 4] = [
    [(libc::REG_RAX, "rax"), (libc::REG_RBX, "rbx"), (libc::REG_RCX, "rcx"), (libc::REG_RDX, "rdx")],
    [(libc::REG_RSI, "rsi"), (libc::REG_RDI, "rdi"), (libc::REG_RBP, "rbp"), (libc::REG_RSP, "rsp")],
    [(libc::REG_R8, "r8"), (libc::REG_R9, "r9"), (libc::REG_R10, "r10"), (libc::REG_R11, "r11")],
    [(libc::REG_R12, "r12"), (libc::REG_R13, "r13"), (libc::REG_R14, "r14"), (libc::REG_R15, "r15")],
];

/// The ftlhelm code a hook runs on the main thread, as the report names it.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Phase {
    Outside,
    OnLoop,
    Overlay,
    ButtonMove,
}

impl Phase {
    const ALL: [Phase; 4] = [Phase::Outside, Phase::OnLoop, Phase::Overlay, Phase::ButtonMove];

    /// The hooked function; None outside ftlhelm.
    fn hook(self) -> Option<&'static str> {
        match self {
            Phase::Outside => None,
            Phase::OnLoop => Some("CApp::OnLoop"),
            Phase::Overlay => Some("MouseControl::OnRender"),
            Phase::ButtonMove => Some("GenericButton::MouseMove"),
        }
    }
}

static PHASE: AtomicU8 = AtomicU8::new(Phase::Outside as u8);

/// Restores the phase entered before it when dropped.
pub struct PhaseGuard(u8);

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        compiler_fence(Ordering::SeqCst);
        PHASE.store(self.0, Ordering::Relaxed);
    }
}

/// Names `phase` as running until the guard drops; guards nest. Main thread only.
pub fn enter(phase: Phase) -> PhaseGuard {
    let before = PHASE.swap(phase as u8, Ordering::Relaxed);
    // The handler interrupts this thread, so ordering against its own code is enough.
    compiler_fence(Ordering::SeqCst);
    PhaseGuard(before)
}

/// What the handler needs, gathered at install.
struct Setup {
    game: Range<usize>,
    library: Range<usize>,
    /// The library's load address, which its offsets count from.
    base: usize,
    main_thread: libc::pthread_t,
    /// The actions in place before, in SIGNALS order.
    previous: [libc::sigaction; SIGNALS.len()],
}

static SETUP: OnceLock<Setup> = OnceLock::new();
/// Set while a report runs or chains, so a signal raised meanwhile reports nothing.
static REPORTED: AtomicBool = AtomicBool::new(false);

extern "C" {
    /// The library's ELF header, at its load address.
    static __ehdr_start: u8;
}

/// Puts the report in front of the handlers in place for the fatal signals; `game_code` is
/// an address in the game's code. Logs a failure. Main thread, once.
pub fn install(game_code: usize) {
    let code = |addr: usize| mapping_of(addr).map_or(0..0, |m| m.range);
    // SAFETY: sigaction is plain data; all zeroes is a valid value.
    let mut previous: [libc::sigaction; SIGNALS.len()] = unsafe { std::mem::zeroed() };
    for ((sig, _), prev) in SIGNALS.iter().zip(&mut previous) {
        // SAFETY: reads the current action into a sigaction.
        unsafe { libc::sigaction(*sig, std::ptr::null(), prev) };
    }
    let setup = Setup {
        game: code(game_code),
        library: code(install as *const () as usize),
        base: std::ptr::addr_of!(__ehdr_start) as usize,
        // SAFETY: pthread_self has no preconditions.
        main_thread: unsafe { libc::pthread_self() },
        previous,
    };
    if SETUP.set(setup).is_err() {
        return;
    }
    // SAFETY: a stack_t is plain data; the buffer leaks, so it outlives the thread.
    unsafe {
        let mut current: libc::stack_t = std::mem::zeroed();
        libc::sigaltstack(std::ptr::null(), &mut current);
        if current.ss_flags & libc::SS_DISABLE != 0 || current.ss_size < ALT_STACK_LEN {
            let stack = Box::leak(vec![0u8; ALT_STACK_LEN].into_boxed_slice());
            let alt = libc::stack_t { ss_sp: stack.as_mut_ptr().cast(), ss_flags: 0, ss_size: ALT_STACK_LEN };
            libc::sigaltstack(&alt, std::ptr::null_mut());
        }
    }
    // SAFETY: as above.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = handler as *const () as usize;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
    for (sig, name) in SIGNALS {
        // SAFETY: an SA_SIGINFO handler of the matching type, with an empty mask.
        if unsafe { libc::sigaction(sig, &action, std::ptr::null_mut()) } != 0 {
            log!("crash report: cannot handle {name}");
        }
    }
}

/// Logs a panic's place and message; the abort that follows is reported as SIGABRT.
pub fn log_panics() {
    std::panic::set_hook(Box::new(|info| {
        let place = info.location().map(|l| l.to_string()).unwrap_or_default();
        log!("panic at {place}: {}", info.payload_as_str().unwrap_or("no message"));
    }));
}

extern "C" fn handler(sig: c_int, info: *mut libc::siginfo_t, ctx: *mut c_void) {
    let setup = SETUP.get();
    let reporting = setup.is_some() && !REPORTED.swap(true, Ordering::Relaxed);
    if let (true, Some(setup)) = (reporting, setup) {
        // SAFETY: the kernel passes an SA_SIGINFO handler its siginfo and ucontext.
        unsafe { report(setup, sig, &*info, &*(ctx as *const libc::ucontext_t)) };
    }
    let previous = setup.and_then(|s| SIGNALS.iter().position(|(s2, _)| *s2 == sig).map(|i| &s.previous[i]));
    // SAFETY: the action that was in place, with the arguments this handler was given.
    unsafe { chain(sig, previous, info, ctx) };
    // The handler in place went on from the fault: the next one is reported anew.
    if reporting {
        REPORTED.store(false, Ordering::Relaxed);
    }
}

fn report(setup: &Setup, sig: c_int, info: &libc::siginfo_t, ctx: &libc::ucontext_t) {
    let name = SIGNALS.iter().find(|(s, _)| *s == sig).map_or("signal", |(_, n)| n);
    if info.si_code > 0 {
        // SAFETY: a fault's siginfo holds the faulting address.
        let at = unsafe { info.si_addr() } as usize;
        log_unlocked!("crash: {name} at {at:#x}, code {}", info.si_code);
    } else {
        log_unlocked!("crash: {name} raised");
    }
    // SAFETY: pthread_self has no preconditions.
    if unsafe { libc::pthread_self() } != setup.main_thread {
        log_unlocked!("crash: another thread");
    } else {
        let phase = PHASE.load(Ordering::Relaxed);
        match Phase::ALL.get(phase as usize).and_then(|p| p.hook()) {
            Some(hook) => log_unlocked!("crash: main thread, in ftlhelm's {hook} hook"),
            None => log_unlocked!("crash: main thread, outside ftlhelm"),
        }
    }
    let regs = &ctx.uc_mcontext.gregs;
    let rip = regs[libc::REG_RIP as usize] as usize;
    if setup.game.contains(&rip) {
        log_unlocked!("crash: rip {rip:#x} game");
    } else if setup.library.contains(&rip) {
        log_unlocked!("crash: rip {rip:#x} ftlhelm+{:#x}", rip - setup.base);
    } else {
        log_unlocked!("crash: rip {rip:#x}");
    }
    for row in REGISTERS {
        let [a, b, c, d] = row.map(|(reg, name)| (name, regs[reg as usize] as u64));
        log_unlocked!("crash: {}={:#018x} {}={:#018x} {}={:#018x} {}={:#018x}", a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1);
    }
}

/// Hands the signal to the action in place before; a default or ignore is raised again.
/// # Safety
/// `info` and `ctx` must be what the kernel passed the handler for `sig`.
unsafe fn chain(sig: c_int, previous: Option<&libc::sigaction>, info: *mut libc::siginfo_t, ctx: *mut c_void) {
    let Some(prev) = previous.filter(|p| p.sa_sigaction != libc::SIG_DFL && p.sa_sigaction != libc::SIG_IGN) else {
        let default = previous.copied().unwrap_or_else(|| {
            let mut a: libc::sigaction = std::mem::zeroed();
            a.sa_sigaction = libc::SIG_DFL;
            a
        });
        libc::sigaction(sig, &default, std::ptr::null_mut());
        libc::raise(sig);
        return;
    };
    if prev.sa_flags & libc::SA_SIGINFO != 0 {
        let f: extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void) = std::mem::transmute(prev.sa_sigaction);
        f(sig, info, ctx);
    } else {
        let f: extern "C" fn(c_int) = std::mem::transmute(prev.sa_sigaction);
        f(sig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;

    /// Set in the child process the report test runs.
    const CHILD_ENV: &str = "FTLHELM_CRASH_CHILD";
    /// Set in the child process the stack overflow test runs.
    const OVERFLOW_ENV: &str = "FTLHELM_OVERFLOW_CHILD";
    /// An address in the unmapped first page.
    const FAULT_ADDR: usize = 0x10;

    /// Faults with the report installed, in the child process only.
    #[test]
    fn crash_child() {
        if std::env::var_os(CHILD_ENV).is_none() {
            return;
        }
        install(0);
        let _phase = enter(Phase::OnLoop);
        // SAFETY: none; the read faults on purpose, in a child process.
        unsafe { std::ptr::read_volatile(std::ptr::without_provenance::<u8>(FAULT_ADDR)) };
    }

    /// Overflows the stack with the report installed, in the child process only.
    #[test]
    fn overflow_child() {
        if std::env::var_os(OVERFLOW_ENV).is_none() {
            return;
        }
        install(0);
        #[allow(unconditional_recursion)]
        fn sink(depth: usize) -> usize {
            let pad = std::hint::black_box([depth; 64]);
            sink(depth + 1) + pad[depth % pad.len()]
        }
        std::hint::black_box(sink(0));
    }

    /// Runs `test` in a child process with `env` set; its output.
    fn child(test: &str, env: &str) -> std::process::Output {
        let exe = std::env::current_exe().expect("the test binary has a path");
        Command::new(exe)
            .args(["--exact", test, "--nocapture"])
            .env(env, "1")
            .output()
            .expect("the test binary runs again")
    }

    #[test]
    fn a_stack_overflow_is_reported_and_still_kills() {
        let out = child("core::crash::tests::overflow_child", OVERFLOW_ENV);
        let log = String::from_utf8_lossy(&out.stderr);
        assert!(log.contains("crash: SIGSEGV at "), "{log}");
        assert!(out.status.signal().is_some(), "{log}");
    }

    #[test]
    fn a_fault_is_reported_and_still_kills() {
        let out = child("core::crash::tests::crash_child", CHILD_ENV);
        let log = String::from_utf8_lossy(&out.stderr);
        for line in [
            &format!("crash: SIGSEGV at {FAULT_ADDR:#x}, code "),
            "crash: main thread, in ftlhelm's CApp::OnLoop hook",
            " ftlhelm+0x",
            "crash: rax=0x",
        ] {
            assert!(log.contains(line), "no {line:?} in:\n{log}");
        }
        assert_eq!(out.status.signal(), Some(libc::SIGSEGV), "{log}");
    }
}
