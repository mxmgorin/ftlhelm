//! Settings with their defaults and ranges, read from the layout's [config] section, the
//! mtime poll that reloads the layout while the game runs, and the file beside the layout
//! that keeps what the settings wheel changes.
//!
//! The saved values override the layout's at start. A reload while the game runs takes a
//! saved key from the layout only when the layout's own value for it changed.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::ui::text::{TEXT_SCALE_AUTO, TEXT_SCALE_MAX};
use crate::ui::zoom::{ZOOM_MAX, ZOOM_MIN};

const LAYOUT_ENV: &str = "FTLHELM_LAYOUT";
/// The file beside the layout that keeps the settings wheel's values.
const SAVED_NAME: &str = "ftlhelm-saved.ini";
const SAVED_HEADER: &str = "# Zoom and text size from the game's settings wheel; they override ftlhelm.ini.";
const FILE_POLL_S: f64 = 1.0;

const SPEED_LEVEL_MIN: f32 = 1.0;
const SPEED_LEVEL_MAX: f32 = 6.0;
const RUMBLE_STRENGTH: f32 = 0.4;
const RUMBLE_S: f32 = 0.05;
const RUMBLE_S_MIN: f32 = 0.01;
const RUMBLE_S_MAX: f32 = 0.5;
const STICK_DEADZONE: f32 = 0.12;
const STICK_DEADZONE_MAX: f32 = 0.9;
/// Logical pixels per second at full deflection.
const CURSOR_SPEED: f32 = 800.0;
const CURSOR_SPEED_MIN: f32 = 1.0;
const CURSOR_CURVE: f32 = 2.0;
const CURSOR_CURVE_MIN: f32 = 0.5;
const CURSOR_CURVE_MAX: f32 = 4.0;
const TAP_HOLD_S: f32 = 0.2;
const HOLD_S_MIN: f32 = 0.05;
const HOLD_S_MAX: f32 = 2.0;
pub const FLICK_ON: f32 = 0.8;
pub const FLICK_OFF: f32 = 0.25;
/// The narrowest band a flick threshold keeps above the re-arm level.
const FLICK_GAP: f32 = 0.1;
/// Zero: one flick until the stick re-centres.
const FLICK_REPEAT_S: f32 = 0.0;
/// Zone search cone half-angles: the narrow one first, the wide one when it finds nothing.
const CONE_NARROW_DEG: f32 = 30.0;
const CONE_WIDE_DEG: f32 = 60.0;
const CONE_DEG_MIN: f32 = 5.0;
const CONE_DEG_MAX: f32 = 89.0;
/// Zone search: sideways distance counts this many times the forward distance.
const FOCUS_SIDE_WEIGHT: f32 = 2.0;
const FOCUS_SIDE_WEIGHT_MAX: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct Settings {
    pub cursor_speed: f32,
    pub cursor_deadzone: f32,
    pub cursor_curve: f32,
    pub overlay: bool,
    pub snap_dpad: bool,
    pub autopause_wheel: bool,
    pub autopause_aim: bool,
    pub speed_level: i32,
    pub rumble: bool,
    pub flick_threshold: f32,
    pub flick_repeat: f32,
    /// Which raw pairs to swap for the labels when no SDL mapping names the face buttons:
    /// A/B (east/south) and X/Y (north/west).
    pub swap_ab: bool,
    pub swap_xy: bool,
    pub tap_hold: f32,
    pub focus_cone_narrow: f32,
    pub focus_cone_wide: f32,
    pub focus_side_weight: f32,
    /// Menus, dialogs and the map open with the cursor on their first item.
    pub focus_first: bool,
    /// The cursor moves to the room of a newly selected crew member.
    pub crew_follow: bool,
    /// Wheels open only on the ship view.
    pub wheels_ship_only: bool,
    pub rumble_strength: f32,
    pub rumble_time: f32,
    /// How much larger event and tooltip text is drawn; TEXT_SCALE_AUTO picks by screen size.
    pub text_scale: f32,
    /// The zoom level the ship view starts at.
    pub zoom: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            cursor_speed: CURSOR_SPEED,
            cursor_deadzone: STICK_DEADZONE,
            cursor_curve: CURSOR_CURVE,
            overlay: true,
            snap_dpad: true,
            autopause_wheel: true,
            autopause_aim: false,
            speed_level: SPEED_LEVEL_MAX as i32,
            rumble: true,
            flick_threshold: FLICK_ON,
            flick_repeat: FLICK_REPEAT_S,
            swap_ab: false,
            swap_xy: false,
            tap_hold: TAP_HOLD_S,
            focus_cone_narrow: CONE_NARROW_DEG,
            focus_cone_wide: CONE_WIDE_DEG,
            focus_side_weight: FOCUS_SIDE_WEIGHT,
            focus_first: true,
            crew_follow: false,
            wheels_ship_only: true,
            rumble_strength: RUMBLE_STRENGTH,
            rumble_time: RUMBLE_S,
            text_scale: TEXT_SCALE_AUTO,
            zoom: ZOOM_MIN,
        }
    }
}

/// Where an option's value goes.
enum Field {
    Float(fn(&mut Settings) -> &mut f32, f32, f32),
    Int(fn(&mut Settings) -> &mut i32, f32, f32),
    Bool(fn(&mut Settings) -> &mut bool),
}

/// Config file keys: the field each sets and the range a number is clamped to.
const OPTIONS: &[(&str, Field)] = &[
    ("cursor_speed", Field::Float(|c| &mut c.cursor_speed, CURSOR_SPEED_MIN, f32::INFINITY)),
    ("cursor_deadzone", Field::Float(|c| &mut c.cursor_deadzone, 0.0, STICK_DEADZONE_MAX)),
    ("cursor_curve", Field::Float(|c| &mut c.cursor_curve, CURSOR_CURVE_MIN, CURSOR_CURVE_MAX)),
    ("overlay", Field::Bool(|c| &mut c.overlay)),
    ("snap_dpad", Field::Bool(|c| &mut c.snap_dpad)),
    ("autopause_wheel", Field::Bool(|c| &mut c.autopause_wheel)),
    ("autopause_aim", Field::Bool(|c| &mut c.autopause_aim)),
    ("speed_level", Field::Int(|c| &mut c.speed_level, SPEED_LEVEL_MIN, SPEED_LEVEL_MAX)),
    ("rumble", Field::Bool(|c| &mut c.rumble)),
    ("swap_ab", Field::Bool(|c| &mut c.swap_ab)),
    ("swap_xy", Field::Bool(|c| &mut c.swap_xy)),
    ("flick_threshold", Field::Float(|c| &mut c.flick_threshold, FLICK_OFF + FLICK_GAP, 1.0)),
    ("flick_repeat", Field::Float(|c| &mut c.flick_repeat, 0.0, f32::INFINITY)),
    ("tap_hold", Field::Float(|c| &mut c.tap_hold, HOLD_S_MIN, HOLD_S_MAX)),
    ("focus_cone_narrow", Field::Float(|c| &mut c.focus_cone_narrow, CONE_DEG_MIN, CONE_DEG_MAX)),
    ("focus_cone_wide", Field::Float(|c| &mut c.focus_cone_wide, CONE_DEG_MIN, CONE_DEG_MAX)),
    ("focus_side_weight", Field::Float(|c| &mut c.focus_side_weight, 0.0, FOCUS_SIDE_WEIGHT_MAX)),
    ("focus_first", Field::Bool(|c| &mut c.focus_first)),
    ("crew_follow", Field::Bool(|c| &mut c.crew_follow)),
    ("wheels_ship_only", Field::Bool(|c| &mut c.wheels_ship_only)),
    ("rumble_strength", Field::Float(|c| &mut c.rumble_strength, 0.0, 1.0)),
    ("rumble_time", Field::Float(|c| &mut c.rumble_time, RUMBLE_S_MIN, RUMBLE_S_MAX)),
    ("text_scale", Field::Float(|c| &mut c.text_scale, TEXT_SCALE_AUTO, TEXT_SCALE_MAX)),
    ("zoom", Field::Float(|c| &mut c.zoom, ZOOM_MIN, ZOOM_MAX)),
];

impl Settings {
    /// Sets one option from a "key = value" line; unknown keys and unreadable values are
    /// logged and skipped.
    pub fn apply(&mut self, key: &str, value: &str) {
        let Some(value) = float_prefix(value) else {
            log!("config: bad value for {key}");
            return;
        };
        match OPTIONS.iter().find(|(name, _)| *name == key) {
            Some((_, field)) => self.set(field, value),
            None => log!("config: unknown key {key}"),
        }
    }

    fn set(&mut self, field: &Field, value: f32) {
        match *field {
            Field::Float(f, lo, hi) => *f(self) = clampf(value, lo, hi),
            Field::Int(f, lo, hi) => *f(self) = clampf(value, lo, hi) as i32,
            Field::Bool(f) => *f(self) = value != 0.0,
        }
    }

    /// Every option's current value, as one log line.
    pub fn describe(&mut self) -> String {
        let mut out = String::new();
        for (name, field) in OPTIONS {
            let value = match *field {
                Field::Float(f, ..) => format!("{:.2}", *f(self)),
                Field::Int(f, ..) => f(self).to_string(),
                Field::Bool(f) => (*f(self) as i32).to_string(),
            };
            out.push_str(&format!(" {name}={value}"));
        }
        out
    }
}

/// When the layout file was last polled, and its mtime then.
#[derive(Default)]
pub struct FileWatch {
    last_poll: Option<f64>,
    layout_mtime: Option<SystemTime>,
    /// The saved settings as the layout file last gave them; None before the first read.
    layout_tuned: Option<Tuned>,
    /// The saved settings as last read from or written to the saved file.
    saved: Option<Tuned>,
}

/// The settings the settings wheel changes and the saved file keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuned {
    pub zoom: f32,
    pub text_scale: f32,
}

impl Tuned {
    /// Each value from `layout` where it differs from `before`, the layout's previous
    /// value, else from `running`.
    fn edited(layout: Tuned, before: Tuned, running: Tuned) -> Tuned {
        let pick = |l: f32, b: f32, r: f32| if l != b { l } else { r };
        Tuned {
            zoom: pick(layout.zoom, before.zoom, running.zoom),
            text_scale: pick(layout.text_scale, before.text_scale, running.text_scale),
        }
    }

    /// `base` with the "key = value" lines of saved file text applied over it.
    fn read(text: &str, base: Tuned) -> Tuned {
        let mut cfg = Settings { zoom: base.zoom, text_scale: base.text_scale, ..Settings::default() };
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or_default();
            if let Some((key, value)) = line.split_once('=') {
                cfg.apply(key.trim(), value.trim());
            }
        }
        Tuned { zoom: cfg.zoom, text_scale: cfg.text_scale }
    }

    /// The saved file's text; the values print in full, so they read back the same.
    fn text(self) -> String {
        format!("{SAVED_HEADER}\nzoom = {}\ntext_scale = {}\n", self.zoom, self.text_scale)
    }
}

/// The saved file's path: beside the layout file, when there is one.
fn saved_path() -> Option<PathBuf> {
    std::env::var_os(LAYOUT_ENV).map(|layout| Path::new(&layout).with_file_name(SAVED_NAME))
}

/// Writes `text` to `path` through a file beside it, so a cut write leaves the old file.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let part = path.with_extension("part");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, path)
}

/// Whether the file at `path` changed since `last`; records the new mtime.
fn changed(path: &Path, last: &mut Option<SystemTime>) -> bool {
    let Ok(mtime) = std::fs::metadata(path).and_then(|m| m.modified()) else {
        return false;
    };
    if *last == Some(mtime) {
        return false;
    }
    *last = Some(mtime);
    true
}

impl Layer {
    pub fn tuned(&self) -> Tuned {
        Tuned { zoom: self.zoom.level, text_scale: self.cfg.text_scale }
    }

    fn set_tuned(&mut self, t: Tuned) {
        self.zoom.level = t.zoom;
        self.cfg.text_scale = t.text_scale;
    }

    /// Writes the saved settings when they changed since the saved file was last read or
    /// written.
    pub fn save_tuned(&mut self) {
        let (Some(path), t) = (saved_path(), self.tuned()) else {
            return;
        };
        if self.files.saved == Some(t) {
            return;
        }
        match write_whole(&path, &t.text()) {
            Ok(()) => {
                self.files.saved = Some(t);
                log!("saved: zoom {:.2}, text_scale {:.2}", t.zoom, t.text_scale);
            }
            Err(e) => log!("cannot save {}: {e}", path.display()),
        }
    }

    /// Settles the saved settings after the layout file is read: at the first read the
    /// saved file overrides it, later the keys the layout changed win and are saved.
    fn settle_tuned(&mut self, running: Tuned) {
        let layout = self.tuned();
        match self.files.layout_tuned.replace(layout) {
            None => {
                let saved = saved_path().and_then(|p| std::fs::read_to_string(p).ok());
                let t = saved.map_or(layout, |text| Tuned::read(&text, layout));
                self.set_tuned(t);
                self.files.saved = Some(t);
            }
            Some(before) => {
                self.set_tuned(Tuned::edited(layout, before, running));
                self.save_tuned();
            }
        }
    }

    /// Re-reads the layout when its mtime changes, at most once per poll interval.
    pub fn poll_files(&mut self, t: f64) {
        if self.files.last_poll.is_some_and(|last| t - last < FILE_POLL_S) {
            return;
        }
        self.files.last_poll = Some(t);
        if let Ok(path) = std::env::var(LAYOUT_ENV) {
            if changed(Path::new(&path), &mut self.files.layout_mtime) {
                match std::fs::read_to_string(&path) {
                    // An empty file is a save caught midway; the current layout stays.
                    Ok(text) if text.is_empty() => {}
                    Ok(text) => {
                        let running = self.tuned();
                        self.load_layout(&text, &path);
                        self.settle_tuned(running);
                    }
                    Err(_) => log!("layout: cannot open {path}"),
                }
            }
        }
    }
}

/// Parses the longest leading part of `s` that is a number, as strtof does; returns None
/// when no prefix is.
fn float_prefix(s: &str) -> Option<f32> {
    s.char_indices().map(|(i, c)| i + c.len_utf8()).rev().find_map(|end| s[..end].parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_keep_their_numeric_prefix() {
        let mut cfg = Settings::default();
        cfg.apply("tap_hold", "0.3;");
        cfg.apply("cursor_speed", "900px");
        cfg.apply("speed_level", "3 levels");
        assert_eq!(cfg.tap_hold, 0.3);
        assert_eq!(cfg.cursor_speed, 900.0);
        assert_eq!(cfg.speed_level, 3);
    }

    #[test]
    fn saved_values_round_trip_and_an_edited_layout_key_wins() {
        let base = Tuned { zoom: 1.0, text_scale: 0.0 };
        let tuned = Tuned { zoom: 4.0 / 3.0, text_scale: 1.25 };
        assert_eq!(Tuned::read(&tuned.text(), base), tuned);
        assert_eq!(Tuned::read("zoom = 2 # note\n", base), Tuned { zoom: 2.0, ..base });
        let before = Tuned { zoom: 1.0, text_scale: 0.0 };
        let layout = Tuned { zoom: 1.0, text_scale: 1.5 };
        assert_eq!(Tuned::edited(layout, before, tuned), Tuned { zoom: tuned.zoom, text_scale: 1.5 });
    }

    #[test]
    fn a_whole_write_replaces_the_file() {
        let path = std::env::temp_dir().join(format!("ftlhelm-test-{}.ini", std::process::id()));
        write_whole(&path, "a").expect("a writable temp dir");
        write_whole(&path, "b").expect("a writable temp dir");
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some("b"));
        assert!(!path.with_extension("part").exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nan_takes_the_lower_bound() {
        let mut cfg = Settings::default();
        cfg.apply("cursor_speed", "nan");
        cfg.apply("speed_level", "nan");
        assert_eq!(cfg.cursor_speed, CURSOR_SPEED_MIN);
        assert_eq!(cfg.speed_level, SPEED_LEVEL_MIN as i32);
    }
}
