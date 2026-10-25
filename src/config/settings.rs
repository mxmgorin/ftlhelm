//! Settings with their defaults and ranges, read from the layout's [config] section, and
//! the mtime poll that reloads the layout while the game runs.

use std::path::Path;
use std::time::SystemTime;

use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::ui::text::{TEXT_SCALE_AUTO, TEXT_SCALE_MAX};

const LAYOUT_ENV: &str = "FTLPAD_LAYOUT";
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
/// Zero: one flick per deflection, the stick must return to centre.
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

/// When the layout file was last read.
#[derive(Default)]
pub struct FileWatch {
    last_poll: Option<f64>,
    layout_mtime: Option<SystemTime>,
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
                    Ok(text) => self.load_layout(&text, &path),
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
    fn nan_takes_the_lower_bound() {
        let mut cfg = Settings::default();
        cfg.apply("cursor_speed", "nan");
        cfg.apply("speed_level", "nan");
        assert_eq!(cfg.cursor_speed, CURSOR_SPEED_MIN);
        assert_eq!(cfg.speed_level, SPEED_LEVEL_MIN as i32);
    }
}
