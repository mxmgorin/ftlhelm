//! Clock, a clamp that never panics, and the stderr log.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clampf_never_panics() {
        assert_eq!(clampf(5.0, 10.0, 0.0), 0.0);
        assert_eq!(clampf(f32::NAN, 1.0, 2.0), 1.0);
    }
}
