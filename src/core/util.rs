//! Clock, a clamp that never panics, the stderr log, and line buffering for the game's
//! stdout.

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

const LOG_PREFIX: &str = "ftlpad: ";

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Seconds since the library loaded.
pub fn now_s() -> f64 {
    epoch().elapsed().as_secs_f64()
}

/// Clamps `v` to `lo..=hi`; returns `lo` for NaN and `hi` when the bounds cross.
pub fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

/// Writes one log line: prefix, seconds since load, the message.
pub fn log_line(args: std::fmt::Arguments) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{LOG_PREFIX}{:7.2} {args}", now_s());
}

macro_rules! log {
    ($($arg:tt)*) => { $crate::core::util::log_line(format_args!($($arg)*)) };
}
pub(crate) use log;

extern "C" {
    static mut stdout: *mut libc::FILE;
}

/// Makes the process's stdout line-buffered, so the game's own messages reach the log
/// even when the game is killed. Must run before anything writes to stdout.
pub fn line_buffer_stdout() {
    // SAFETY: setvbuf on the C stdout before its first use; the null buffer lets libc
    // allocate its own.
    unsafe { libc::setvbuf(std::ptr::addr_of!(stdout).read(), std::ptr::null_mut(), libc::_IOLBF, 0) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clampf_never_panics() {
        assert_eq!(clampf(5.0, 10.0, 0.0), 0.0);
        assert_eq!(clampf(f32::NAN, 1.0, 2.0), 1.0);
    }
}
