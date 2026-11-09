//! Clocks since load and per frame, a clamp that never panics, the log on stderr and in
//! its file, and line buffering for the game's stdout.

use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

const LOG_PREFIX: &str = "ftlhelm: ";
/// The previous run's log keeps the log file's name with this extension.
const PREVIOUS_LOG_EXT: &str = "old.log";
const HEARTBEAT_S: f64 = 10.0;
const MAX_DT: f64 = 0.05;
/// Longest line log_line_unlocked writes, newline excluded.
const LINE_MAX: usize = 255;

static LOG_FILE: OnceLock<File> = OnceLock::new();

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Seconds since the library loaded.
pub fn now_s() -> f64 {
    epoch().elapsed().as_secs_f64()
}

/// One frame's timing.
pub struct Frame {
    /// Seconds since the frame before, capped; zero on the first.
    pub dt: f32,
    /// Frames so far, this one included.
    pub count: u64,
    /// Whether a heartbeat interval has passed since the last beat.
    pub beat: bool,
}

/// Frame timing kept from one frame to the next.
#[derive(Default)]
pub struct FrameClock {
    last_t: f64,
    last_beat: f64,
    frames: u64,
}

impl FrameClock {
    /// Counts a frame at `t` seconds; returns its timing.
    pub fn tick(&mut self, t: f64) -> Frame {
        self.frames += 1;
        let dt = if self.last_t > 0.0 { (t - self.last_t).min(MAX_DT) as f32 } else { 0.0 };
        self.last_t = t;
        let beat = t - self.last_beat >= HEARTBEAT_S;
        if beat {
            self.last_beat = t;
        }
        Frame { dt, count: self.frames, beat }
    }
}

/// The text of a NUL-terminated byte buffer; the whole buffer when no NUL ends it.
pub fn nul_terminated(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Clamps `v` to `lo..=hi`; returns `lo` for NaN and `hi` when the bounds cross.
pub fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

/// Writes one log line: prefix, seconds since load, the message.
pub fn log_line(args: std::fmt::Arguments) {
    let line = format!("{LOG_PREFIX}{:7.2} {args}\n", now_s());
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
    if let Some(mut file) = LOG_FILE.get() {
        let _ = file.write_all(line.as_bytes());
    }
}

/// A line formatted on the stack; what does not fit is cut, leaving room for the newline.
struct StackLine {
    bytes: [u8; LINE_MAX + 1],
    len: usize,
}

impl std::fmt::Write for StackLine {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let n = s.len().min(LINE_MAX - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Writes one log line as log_line does, taking no lock and allocating nothing, so a
/// signal handler may call it. A line past LINE_MAX bytes is cut.
pub fn log_line_unlocked(args: std::fmt::Arguments) {
    let mut line = StackLine { bytes: [0; LINE_MAX + 1], len: 0 };
    let _ = std::fmt::Write::write_fmt(&mut line, format_args!("{LOG_PREFIX}{:7.2} {args}", now_s()));
    line.bytes[line.len] = b'\n';
    let fds = [Some(libc::STDERR_FILENO), LOG_FILE.get().map(|f| f.as_raw_fd())];
    for fd in fds.into_iter().flatten() {
        // SAFETY: writes the initialized head of a live buffer.
        unsafe { libc::write(fd, line.bytes.as_ptr().cast(), line.len + 1) };
    }
}

macro_rules! log {
    ($($arg:tt)*) => { $crate::core::util::log_line(format_args!($($arg)*)) };
}
pub(crate) use log;

macro_rules! log_unlocked {
    ($($arg:tt)*) => { $crate::core::util::log_line_unlocked(format_args!($($arg)*)) };
}
pub(crate) use log_unlocked;

/// Starts the log file at `path`, the previous run's file kept beside it; the lines after
/// go to it as well as to stderr. Logs a failure.
pub fn open_log_file(path: &Path) {
    let _ = std::fs::rename(path, path.with_extension(PREVIOUS_LOG_EXT));
    match File::create(path) {
        Ok(file) => {
            let _ = LOG_FILE.set(file);
        }
        Err(e) => log!("cannot write the log {}: {e}", path.display()),
    }
}

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
    fn the_frame_clock_caps_the_step_and_beats_once_an_interval() {
        let mut clock = FrameClock::default();
        let timing = |f: Frame| (f.dt, f.count, f.beat);
        let step = 1.0 / 32.0;
        assert_eq!(timing(clock.tick(1.0)), (0.0, 1, false));
        assert_eq!(timing(clock.tick(1.0 + step)), (step as f32, 2, false));
        assert_eq!(timing(clock.tick(2.0)), (MAX_DT as f32, 3, false));
        assert_eq!(timing(clock.tick(HEARTBEAT_S)), (MAX_DT as f32, 4, true));
        assert!(!clock.tick(HEARTBEAT_S + 1.0).beat);
        assert!(clock.tick(2.0 * HEARTBEAT_S).beat);
    }

    #[test]
    fn clampf_never_panics() {
        assert_eq!(clampf(5.0, 10.0, 0.0), 0.0);
        assert_eq!(clampf(f32::NAN, 1.0, 2.0), 1.0);
    }
}
