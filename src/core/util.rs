//! Clock and the stderr log.

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

/// Writes one log line: prefix, seconds since load, the message.
pub fn log_line(args: std::fmt::Arguments) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{LOG_PREFIX}{:7.2} {args}", now_s());
}

macro_rules! log {
    ($($arg:tt)*) => { $crate::core::util::log_line(format_args!($($arg)*)) };
}
pub(crate) use log;
