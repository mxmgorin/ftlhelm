//! The frame rate log: with `fps_log` set, one line per interval with the frame rate, the
//! frame time, and the share of it the game's own loop and the layer take.

use std::fmt;

use crate::core::layer::Layer;
use crate::core::util::{log, now_s};

const MS_PER_S: f64 = 1000.0;

/// Frame timing gathered since the last line.
#[derive(Default)]
pub struct FpsLog {
    /// When the interval started; zero while the log is off.
    start: f64,
    last: f64,
    frames: u32,
    frame_max: f64,
    game: f64,
    layer: f64,
}

/// One interval's averages, in frames per second and milliseconds per frame.
struct FpsLine {
    fps: f64,
    frame_ms: f64,
    frame_max_ms: f64,
    game_ms: f64,
    layer_ms: f64,
}

impl fmt::Display for FpsLine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "fps {:.1}, frame {:.1} ms (max {:.1}), game loop {:.1} ms, ftlhelm {:.2} ms",
            self.fps, self.frame_ms, self.frame_max_ms, self.game_ms, self.layer_ms
        )
    }
}

impl FpsLog {
    /// Counts a frame starting at `t` while `interval_s` is above zero, logging a line each
    /// time an interval has passed; returns whether the log is on.
    pub fn frame(&mut self, t: f64, interval_s: i32) -> bool {
        if interval_s <= 0 {
            *self = FpsLog::default();
            return false;
        }
        if self.start == 0.0 {
            *self = FpsLog { start: t, last: t, ..FpsLog::default() };
            return true;
        }
        self.frames += 1;
        self.frame_max = self.frame_max.max(t - self.last);
        self.last = t;
        if t - self.start >= f64::from(interval_s) {
            log!("{}", self.line());
            *self = FpsLog { start: t, last: t, ..FpsLog::default() };
        }
        true
    }

    /// Adds seconds the game's own loop took to the current interval.
    pub fn add_game(&mut self, s: f64) {
        self.game += s;
    }

    /// The interval's averages; frames counts the intervals between frame starts.
    fn line(&self) -> FpsLine {
        let span = self.last - self.start;
        let n = f64::from(self.frames);
        FpsLine {
            fps: n / span,
            frame_ms: span / n * MS_PER_S,
            frame_max_ms: self.frame_max * MS_PER_S,
            game_ms: self.game / n * MS_PER_S,
            layer_ms: self.layer / n * MS_PER_S,
        }
    }
}

impl Layer {
    /// The time a timed section starts at, while the frame rate log is on.
    pub fn fps_mark(&self) -> Option<f64> {
        (self.cfg.fps_log > 0).then(now_s)
    }

    /// Adds the time since `mark` to the layer's share of the frame.
    pub fn fps_count_layer(&mut self, mark: Option<f64>) {
        if let Some(t0) = mark {
            self.fps.layer += now_s() - t0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interval_averages_its_frames_and_starts_over() {
        let mut log = FpsLog::default();
        for i in 0..10 {
            assert!(log.frame(1.0 + f64::from(i) * 0.1, 2));
            log.add_game(0.01);
            log.layer += 0.001;
        }
        let line = log.line();
        assert_eq!(log.frames, 9);
        assert!((line.fps - 10.0).abs() < 1e-9);
        assert!((line.frame_ms - 100.0).abs() < 1e-9);
        assert!((line.frame_max_ms - 100.0).abs() < 1e-6);
        assert!(line.game_ms > 10.0 && line.layer_ms > 1.0);
        assert!(log.frame(3.0, 2));
        assert_eq!((log.start, log.frames, log.game), (3.0, 0, 0.0));
    }

    #[test]
    fn off_keeps_nothing() {
        let mut log = FpsLog::default();
        log.frame(1.0, 5);
        log.frame(1.5, 5);
        assert!(!log.frame(2.0, 0));
        assert_eq!((log.start, log.frames), (0.0, 0));
    }
}
