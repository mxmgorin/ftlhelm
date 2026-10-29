//! Settings with their defaults and ranges, read from the layout's [config] section, the
//! mtime poll that reloads the layout while the game runs, and the write of the layer's own
//! values back into that section.

use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;

use crate::config::layout::{CONFIG_SECTION, DEFAULT_LAYOUT};
use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::input::pad::RUMBLE_MAX;
use crate::ui::overlay::{Notice, LAYOUT_NOTICE_S, LAYOUT_UPDATED_NOTICE};
use crate::ui::shrink::{SHARPEN_DEFAULT, SHARPEN_MAX};
use crate::ui::text::{TEXT_SCALE_AUTO, TEXT_SCALE_MAX};
use crate::ui::wheel::{Backdrop, WheelPlace, BACKDROPS, WHEEL_PLACES};
use crate::ui::zoom::{ZOOM_AUTO, ZOOM_MAX};

const LAYOUT_ENV: &str = "FTLHELM_LAYOUT";
/// Keys named outside the options table.
const ZOOM_KEY: &str = "zoom";
const TEXT_SCALE_KEY: &str = "text_scale";
const HELP_AT_START_KEY: &str = "help_at_start";
pub const OVERLAY_KEY: &str = "overlay";
pub const RUMBLE_KEY: &str = "rumble";
pub const SHARPEN_KEY: &str = "sharpen";
pub const SPEED_LEVEL_KEY: &str = "speed_level";
pub const FOCUS_BRACKETS_KEY: &str = "focus_brackets";
pub const CREW_MARKER_KEY: &str = "crew_marker";
pub const HUD_SHIFT_KEY: &str = "hud_shift";
pub const FRAME_FILTER_KEY: &str = "frame_filter";
pub const IPAD_BEACONS_KEY: &str = "ipad_beacons";
const FILE_POLL_S: f64 = 1.0;
/// The layout's format in its [config] section; a file with an older or no version gives way
/// to the default, its settings carried over and the file kept with PREVIOUS_LAYOUT_EXT.
const LAYOUT_VERSION_KEY: &str = "layout_version";
const LAYOUT_VERSION: i32 = 3;
const PREVIOUS_LAYOUT_EXT: &str = "old.ini";

const SPEED_LEVEL_MIN: f32 = 1.0;
const SPEED_LEVEL_MAX: f32 = 6.0;
/// Pulses at RUMBLE_STRENGTH by default.
const RUMBLE: f32 = 1.0;
const RUMBLE_S: f32 = 0.05;
const RUMBLE_S_MIN: f32 = 0.01;
const RUMBLE_S_MAX: f32 = 0.5;
const STICK_DEADZONE: f32 = 0.25;
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
const TOUCH_LONG_PRESS_S: f32 = 0.5;
/// Window pixels a finger may drift and still tap.
const TOUCH_SLOP: f32 = 12.0;
/// Window pixels a touch on nothing reaches for the nearest target.
const TOUCH_SNAP: f32 = 32.0;
/// Window pixels a finger on a power box travels per bar.
const TOUCH_STEP: f32 = 16.0;
const TOUCH_PX_MAX: f32 = 100.0;
/// The most times its size the game's pointer is drawn by default, and the highest setting.
const POINTER_SCALE_MAX: f32 = 2.0;
const POINTER_SCALE_LIMIT: f32 = 4.0;
/// The numbers `set` moves, by key, and their steps.
const STEPS: &[(&str, f32)] =
    &[(RUMBLE_KEY, RUMBLE_STEP), (SHARPEN_KEY, SHARPEN_STEP), (SPEED_LEVEL_KEY, SPEED_LEVEL_STEP)];
const RUMBLE_STEP: f32 = 0.5;
const SHARPEN_STEP: f32 = 0.25;
const SPEED_LEVEL_STEP: f32 = 1.0;
/// Room for float error when a value is matched to a step.
const STEP_SLACK: f32 = 0.001;

#[derive(Clone, Debug)]
pub struct Settings {
    pub cursor_speed: f32,
    pub cursor_deadzone: f32,
    pub cursor_curve: f32,
    /// The badge, the focus brackets and the crew markers show, each as its own switch allows.
    pub overlay: bool,
    /// The mode badge on the game's pause row.
    pub badge: bool,
    /// A zone a focus move lands on is framed with brackets in place of the pointer.
    pub focus_brackets: bool,
    /// Selected crew are marked on the ship view.
    pub crew_marker: bool,
    /// The most times its size the game's pointer is drawn where the frame is shown smaller.
    pub pointer_scale_max: f32,
    pub snap_dpad: bool,
    pub autopause_wheel: bool,
    pub autopause_aim: bool,
    pub speed_level: i32,
    /// Pulse strength in times RUMBLE_STRENGTH; 0: no pulses.
    pub rumble: f32,
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
    /// A click on a room steps the selection through its crew, unless crew is selected elsewhere.
    pub room_crew_pick: bool,
    /// Wheels open only on the ship view.
    pub wheels_ship_only: bool,
    pub wheel_place: WheelPlace,
    pub wheel_backdrop: Backdrop,
    pub rumble_time: f32,
    /// How much larger event and tooltip text is drawn; TEXT_SCALE_AUTO picks by screen size.
    pub text_scale: f32,
    /// The ship view's zoom level; ZOOM_AUTO picks by screen size.
    pub zoom: f32,
    /// The HUD's edge panels move toward the middle on a screen narrower than the frame.
    pub hud_shift: bool,
    /// The map's beacon icons and the crew station buttons in the iPad's larger art.
    pub ipad_beacons: bool,
    pub ipad_station_buttons: bool,
    /// The help shows at start, until it is first closed.
    pub help_at_start: bool,
    /// A frame the screen shrinks is area-averaged; off, the game filters it.
    pub frame_filter: bool,
    /// The unsharp mask's amount over the area-averaged frame.
    pub sharpen: f32,
    /// Touches act as the mouse.
    pub touch: bool,
    /// Seconds a finger stays down for its lift to right-click.
    pub touch_long_press: f32,
    /// Window pixels a finger may drift and still tap; past them, it drags.
    pub touch_slop: f32,
    /// Window pixels a touch on nothing reaches for the nearest target; 0: none.
    pub touch_snap: f32,
    /// Window pixels a finger moving up or down on a power box travels per bar; 0: none.
    pub touch_step: f32,
    /// The ship view shows a pause button while touch is in use.
    pub pause_button: bool,
    /// Rings mark a finger held toward a right click and the point a touch clicked.
    pub touch_marks: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            cursor_speed: CURSOR_SPEED,
            cursor_deadzone: STICK_DEADZONE,
            cursor_curve: CURSOR_CURVE,
            overlay: true,
            badge: true,
            focus_brackets: true,
            crew_marker: true,
            pointer_scale_max: POINTER_SCALE_MAX,
            snap_dpad: true,
            autopause_wheel: true,
            autopause_aim: false,
            speed_level: SPEED_LEVEL_MAX as i32,
            rumble: RUMBLE,
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
            room_crew_pick: true,
            wheels_ship_only: true,
            wheel_place: WheelPlace::default(),
            wheel_backdrop: Backdrop::default(),
            rumble_time: RUMBLE_S,
            text_scale: TEXT_SCALE_AUTO,
            zoom: ZOOM_AUTO,
            hud_shift: true,
            ipad_beacons: true,
            ipad_station_buttons: true,
            help_at_start: true,
            frame_filter: true,
            sharpen: SHARPEN_DEFAULT,
            touch: true,
            touch_long_press: TOUCH_LONG_PRESS_S,
            touch_slop: TOUCH_SLOP,
            touch_snap: TOUCH_SNAP,
            touch_step: TOUCH_STEP,
            pause_button: true,
            touch_marks: true,
        }
    }
}

/// Where an option's value goes.
enum Field {
    Float(fn(&mut Settings) -> &mut f32, f32, f32),
    Int(fn(&mut Settings) -> &mut i32, f32, f32),
    Bool(fn(&mut Settings) -> &mut bool),
    Backdrop(fn(&mut Settings) -> &mut Backdrop),
    Place(fn(&mut Settings) -> &mut WheelPlace),
}

/// Config file keys: the field each sets and the range a number is clamped to.
const OPTIONS: &[(&str, Field)] = &[
    ("cursor_speed", Field::Float(|c| &mut c.cursor_speed, CURSOR_SPEED_MIN, f32::INFINITY)),
    ("cursor_deadzone", Field::Float(|c| &mut c.cursor_deadzone, 0.0, STICK_DEADZONE_MAX)),
    ("cursor_curve", Field::Float(|c| &mut c.cursor_curve, CURSOR_CURVE_MIN, CURSOR_CURVE_MAX)),
    (OVERLAY_KEY, Field::Bool(|c| &mut c.overlay)),
    ("badge", Field::Bool(|c| &mut c.badge)),
    (FOCUS_BRACKETS_KEY, Field::Bool(|c| &mut c.focus_brackets)),
    (CREW_MARKER_KEY, Field::Bool(|c| &mut c.crew_marker)),
    ("pointer_scale_max", Field::Float(|c| &mut c.pointer_scale_max, 1.0, POINTER_SCALE_LIMIT)),
    ("snap_dpad", Field::Bool(|c| &mut c.snap_dpad)),
    ("autopause_wheel", Field::Bool(|c| &mut c.autopause_wheel)),
    ("autopause_aim", Field::Bool(|c| &mut c.autopause_aim)),
    (SPEED_LEVEL_KEY, Field::Int(|c| &mut c.speed_level, SPEED_LEVEL_MIN, SPEED_LEVEL_MAX)),
    (RUMBLE_KEY, Field::Float(|c| &mut c.rumble, 0.0, RUMBLE_MAX)),
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
    ("room_crew_pick", Field::Bool(|c| &mut c.room_crew_pick)),
    ("wheels_ship_only", Field::Bool(|c| &mut c.wheels_ship_only)),
    ("wheel_place", Field::Place(|c| &mut c.wheel_place)),
    ("wheel_backdrop", Field::Backdrop(|c| &mut c.wheel_backdrop)),
    ("rumble_time", Field::Float(|c| &mut c.rumble_time, RUMBLE_S_MIN, RUMBLE_S_MAX)),
    (TEXT_SCALE_KEY, Field::Float(|c| &mut c.text_scale, TEXT_SCALE_AUTO, TEXT_SCALE_MAX)),
    (ZOOM_KEY, Field::Float(|c| &mut c.zoom, ZOOM_AUTO, ZOOM_MAX)),
    (HUD_SHIFT_KEY, Field::Bool(|c| &mut c.hud_shift)),
    (IPAD_BEACONS_KEY, Field::Bool(|c| &mut c.ipad_beacons)),
    ("ipad_station_buttons", Field::Bool(|c| &mut c.ipad_station_buttons)),
    (HELP_AT_START_KEY, Field::Bool(|c| &mut c.help_at_start)),
    (FRAME_FILTER_KEY, Field::Bool(|c| &mut c.frame_filter)),
    (SHARPEN_KEY, Field::Float(|c| &mut c.sharpen, 0.0, SHARPEN_MAX)),
    ("touch", Field::Bool(|c| &mut c.touch)),
    ("touch_long_press", Field::Float(|c| &mut c.touch_long_press, HOLD_S_MIN, HOLD_S_MAX)),
    ("touch_slop", Field::Float(|c| &mut c.touch_slop, 0.0, TOUCH_PX_MAX)),
    ("touch_snap", Field::Float(|c| &mut c.touch_snap, 0.0, TOUCH_PX_MAX)),
    ("touch_step", Field::Float(|c| &mut c.touch_step, 0.0, TOUCH_PX_MAX)),
    ("pause_button", Field::Bool(|c| &mut c.pause_button)),
    ("touch_marks", Field::Bool(|c| &mut c.touch_marks)),
];

/// Settings as the layout file writes them, by key, in table order.
pub type Values = Vec<(&'static str, String)>;

/// A setting `set` changes, as it stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tuning {
    Switch(bool),
    /// A number moved in `step`s up to `max`, from 0 or a step above it.
    Steps {
        value: f32,
        step: f32,
        max: f32,
    },
}

/// The index of setting `key` when `set` may change it: a switch or a number with a step.
pub fn tunable(key: &str) -> Option<usize> {
    let i = OPTIONS.iter().position(|(name, _)| *name == key)?;
    (is_switch(i) || step_of(i).is_some()).then_some(i)
}

/// Whether setting `i` is a switch.
pub fn is_switch(i: usize) -> bool {
    matches!(OPTIONS[i].1, Field::Bool(_))
}

/// The key of setting `i`.
pub fn option_name(i: usize) -> &'static str {
    OPTIONS[i].0
}

fn step_of(i: usize) -> Option<f32> {
    STEPS.iter().find(|(key, _)| *key == OPTIONS[i].0).map(|&(_, step)| step)
}

impl Settings {
    /// Sets one option from a "key = value" line; unknown keys and unreadable values are
    /// logged and skipped.
    pub fn apply(&mut self, key: &str, value: &str) {
        if key == LAYOUT_VERSION_KEY {
            return;
        }
        let Some((_, field)) = OPTIONS.iter().find(|(name, _)| *name == key) else {
            log!("config: unknown key {key}");
            return;
        };
        if self.set(field, value).is_none() {
            log!("config: bad value for {key}");
        }
    }

    /// Sets `field` from a number's prefix or a name's first word; None when it reads as neither.
    fn set(&mut self, field: &Field, value: &str) -> Option<()> {
        match *field {
            Field::Float(f, lo, hi) => *f(self) = clampf(float_prefix(value)?, lo, hi),
            Field::Int(f, lo, hi) => *f(self) = clampf(float_prefix(value)?, lo, hi) as i32,
            Field::Bool(f) => *f(self) = float_prefix(value)? != 0.0,
            Field::Backdrop(f) => *f(self) = by_name(&BACKDROPS, value)?,
            Field::Place(f) => *f(self) = by_name(&WHEEL_PLACES, value)?,
        }
        Some(())
    }

    /// Every option's current value, as one log line.
    pub fn describe(&mut self) -> String {
        let mut out = String::new();
        for (name, field) in OPTIONS {
            let value = match *field {
                Field::Float(f, ..) => format!("{:.2}", *f(self)),
                _ => self.value_text(field),
            };
            out.push_str(&format!(" {name}={value}"));
        }
        out
    }

    /// Every option's value as the layout file writes it.
    pub fn values(&mut self) -> Values {
        OPTIONS.iter().map(|(name, field)| (*name, self.value_text(field))).collect()
    }

    /// `field`'s value as the layout file writes it.
    fn value_text(&mut self, field: &Field) -> String {
        match *field {
            Field::Float(f, ..) => f(self).to_string(),
            Field::Int(f, ..) => f(self).to_string(),
            Field::Bool(f) => (*f(self) as i32).to_string(),
            Field::Backdrop(f) => name_of(&BACKDROPS, *f(self)),
            Field::Place(f) => name_of(&WHEEL_PLACES, *f(self)),
        }
    }

    /// A number field's value and range; None for other fields.
    fn number(&mut self, field: &Field) -> Option<(f32, f32, f32)> {
        match *field {
            Field::Float(f, lo, hi) => Some((*f(self), lo, hi)),
            Field::Int(f, lo, hi) => Some((*f(self) as f32, lo, hi)),
            _ => None,
        }
    }

    /// Sets a number field to `value` within its range.
    fn set_number(&mut self, field: &Field, value: f32) {
        match *field {
            Field::Float(f, lo, hi) => *f(self) = clampf(value, lo, hi),
            Field::Int(f, lo, hi) => *f(self) = clampf(value, lo, hi).round() as i32,
            _ => {}
        }
    }

    /// Tunable setting `i` as it stands.
    pub fn tuning(&mut self, i: usize) -> Tuning {
        let field = &OPTIONS[i].1;
        if let Field::Bool(f) = *field {
            return Tuning::Switch(*f(self));
        }
        let (value, _, max) = self.number(field).expect("tunable() admits switches and numbers only");
        Tuning::Steps { value, step: step_of(i).expect("tunable() admits numbers with a step only"), max }
    }

    /// Moves tunable setting `i` to the step above or below its value, within its range; a
    /// switch turns on or off.
    pub fn step(&mut self, i: usize, up: bool) {
        let field = &OPTIONS[i].1;
        if let Field::Bool(f) = *field {
            *f(self) = up;
            return;
        }
        let (Some((value, ..)), Some(step)) = (self.number(field), step_of(i)) else {
            return;
        };
        let at = value / step;
        let next = if up { (at + STEP_SLACK).floor() + 1.0 } else { (at - STEP_SLACK).ceil() - 1.0 };
        self.set_number(field, next * step);
    }

    /// Moves tunable setting `i` to its next value: a switch flips, a number steps up and
    /// from its top back to its lowest.
    pub fn cycle(&mut self, i: usize) {
        let field = &OPTIONS[i].1;
        if let Field::Bool(f) = *field {
            let on = *f(self);
            *f(self) = !on;
            return;
        }
        match self.number(field) {
            Some((value, lo, hi)) if value >= hi - STEP_SLACK => self.set_number(field, lo),
            _ => self.step(i, true),
        }
    }
}

/// The value `value`'s first word names in `names`.
fn by_name<T: Copy>(names: &[(&str, T)], value: &str) -> Option<T> {
    let word = value.split_whitespace().next()?;
    names.iter().find(|(name, _)| *name == word).map(|&(_, v)| v)
}

/// `value`'s name in `names`.
fn name_of<T: PartialEq>(names: &[(&str, T)], value: T) -> String {
    names.iter().find(|(_, v)| *v == value).map_or("?", |(name, _)| name).to_string()
}

/// When the layout file was last polled, and its mtime then.
#[derive(Default)]
pub struct FileWatch {
    last_poll: Option<f64>,
    layout_mtime: Option<SystemTime>,
    /// The settings as the layout file holds them.
    written: Option<Values>,
    /// Writing the default layout failed; no retry until a file appears at the path.
    default_failed: bool,
}

/// The section a "[NAME]" line opens, as the layout parser reads it.
fn section_name(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix('[')?;
    rest.split(']').next()
}

/// `line`, a "key = value  # comment" line with its '=' at `eq`, holding `value` instead.
fn with_value(line: &str, eq: usize, value: &str) -> String {
    let after = &line[eq + 1..];
    let lead = after.len() - after.trim_start_matches([' ', '\t']).len();
    let rest = &after[lead..];
    let end = rest.find('#').map_or(rest.trim_end().len(), |c| rest[..c].trim_end().len());
    format!("{}{}{value}{}", &line[..=eq], &after[..lead], &rest[end..])
}

/// `text` with each of `values` set in its [config] section: a key's lines take the value
/// and keep their comments, a missing key gets a line at the section's end, and a missing
/// section is added at the top.
fn set_config(text: &str, values: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    if !lines.iter().any(|l| section_name(l) == Some(CONFIG_SECTION)) {
        lines.insert(0, format!("[{CONFIG_SECTION}]\n"));
    }
    let mut found = vec![false; values.len()];
    let (mut in_config, mut section_end) = (false, 0);
    for (i, line) in lines.iter_mut().enumerate() {
        if let Some(name) = section_name(line) {
            in_config = name == CONFIG_SECTION;
            if in_config {
                section_end = i + 1;
            }
            continue;
        }
        if !in_config || line.trim().is_empty() {
            continue;
        }
        section_end = i + 1;
        let Some(eq) = line.find('=') else {
            continue;
        };
        if let Some(j) = values.iter().position(|(key, _)| *key == line[..eq].trim()) {
            *line = with_value(line, eq, &values[j].1);
            found[j] = true;
        }
    }
    if section_end > 0 && !lines[section_end - 1].ends_with('\n') {
        lines[section_end - 1].push('\n');
    }
    let missing = values.iter().zip(&found).filter(|(_, &f)| !f).map(|((key, value), _)| format!("{key} = {value}\n"));
    let tail = lines.split_off(section_end);
    lines.extend(missing);
    lines.extend(tail);
    lines.concat()
}

/// The `key = value` lines of `text`'s [config] section, as (key, the value as written)
/// without comments.
fn config_lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut in_config = false;
    text.lines().filter_map(move |line| {
        if let Some(name) = section_name(line) {
            in_config = name == CONFIG_SECTION;
            return None;
        }
        let (key, value) = line.split_once('=').filter(|_| in_config)?;
        Some((key.trim(), value.split('#').next().unwrap_or_default().trim()))
    })
}

/// The layout version `text` declares; 0 without one.
fn layout_version(text: &str) -> i32 {
    config_lines(text)
        .find(|(key, _)| *key == LAYOUT_VERSION_KEY)
        .and_then(|(_, value)| float_prefix(value))
        .map_or(0, |v| v as i32)
}

/// The default layout holding the settings `old` sets.
fn upgraded(old: &str) -> String {
    let values: Vec<(&str, String)> = config_lines(old)
        .filter_map(|(key, value)| {
            OPTIONS.iter().find(|(name, _)| *name == key).map(|(name, _)| (*name, value.to_string()))
        })
        .collect();
    set_config(DEFAULT_LAYOUT, &values)
}

/// The entries of `now` whose values differ from `before`'s, both in table order.
fn differing(before: &Values, now: &Values) -> Values {
    now.iter().zip(before).filter(|(n, b)| n != b).map(|(n, _)| n.clone()).collect()
}

/// Writes `text` to `path` through a file beside it, so a cut write leaves the old file.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let part = path.with_extension("part");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, path)
}

/// Writes `text` to `path` when no file is there; returns whether it wrote.
fn write_if_missing(path: &Path, text: &str) -> std::io::Result<bool> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == ErrorKind::NotFound => write_whole(path, text).map(|()| true),
        _ => Ok(false),
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Whether the file at `path` changed since `last`; records the new mtime.
fn changed(path: &Path, last: &mut Option<SystemTime>) -> bool {
    let Some(mtime) = mtime(path) else {
        return false;
    };
    if *last == Some(mtime) {
        return false;
    }
    *last = Some(mtime);
    true
}

impl Layer {
    /// The settings as the layer holds them, the zoom at its level, as the layout file writes
    /// them.
    pub fn tuned(&self) -> Values {
        let mut cfg = self.cfg.clone();
        cfg.zoom = self.zoom.level;
        cfg.values()
    }

    /// Moves tunable setting `i` a step up or down, or without `up` to its next value.
    pub fn tune(&mut self, i: usize, up: Option<bool>) {
        match up {
            Some(up) => self.cfg.step(i, up),
            None => self.cfg.cycle(i),
        }
        log!("{} {}", option_name(i), self.cfg.value_text(&OPTIONS[i].1));
    }

    /// Writes the settings that differ from what the layout file holds into its [config]
    /// section.
    pub fn save_tuned(&mut self) {
        let Ok(path) = std::env::var(LAYOUT_ENV) else {
            return;
        };
        let now = self.tuned();
        let before = self.files.written.clone().unwrap_or_else(|| Settings::default().values());
        let changed = differing(&before, &now);
        if changed.is_empty() {
            return;
        }
        let path = Path::new(&path);
        match std::fs::read_to_string(path).and_then(|text| write_whole(path, &set_config(&text, &changed))) {
            Ok(()) => {
                self.files.written = Some(now);
                // The layer's own write is no edit to reload.
                self.files.layout_mtime = mtime(path);
                log!("saved:{}", changed.iter().map(|(key, value)| format!(" {key}={value}")).collect::<String>());
            }
            Err(e) => log!("cannot save {}: {e}", path.display()),
        }
    }

    /// Writes the default layout where the layout file is missing, so deleting the file resets
    /// the layout; a failed write is logged once until a file appears.
    fn restore_default(&mut self, path: &Path) {
        if self.files.default_failed && !path.exists() {
            return;
        }
        match write_if_missing(path, DEFAULT_LAYOUT) {
            Ok(wrote) => {
                self.files.default_failed = false;
                if wrote {
                    log!("layout: default written to {}", path.display());
                }
            }
            Err(e) => {
                self.files.default_failed = true;
                log!("layout: cannot write the default to {}: {e}", path.display());
            }
        }
    }

    /// Replaces the layout file at `path`, holding `text`, with the upgraded default when its
    /// version is older than LAYOUT_VERSION, the old file kept beside it; returns the text to
    /// load, the old one when the replacement fails.
    fn upgrade_layout(&mut self, path: &Path, text: String) -> String {
        let version = layout_version(&text);
        if version >= LAYOUT_VERSION {
            return text;
        }
        let new = upgraded(&text);
        let previous = path.with_extension(PREVIOUS_LAYOUT_EXT);
        match write_whole(&previous, &text).and_then(|()| write_whole(path, &new)) {
            Ok(()) => {
                // The layer's own write is no edit to reload.
                self.files.layout_mtime = mtime(path);
                self.notice = Some(Notice::for_seconds(LAYOUT_UPDATED_NOTICE, LAYOUT_NOTICE_S));
                log!("layout: version {version} replaced by the default, the old file kept as {}", previous.display());
                new
            }
            Err(e) => {
                log!("layout: cannot replace version {version} at {}: {e}", path.display());
                text
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
            self.restore_default(Path::new(&path));
            if changed(Path::new(&path), &mut self.files.layout_mtime) {
                match std::fs::read_to_string(&path) {
                    // An empty file is a save caught midway; the current layout stays.
                    Ok(text) if text.is_empty() => {}
                    Ok(text) => {
                        let text = self.upgrade_layout(Path::new(&path), text);
                        self.load_layout(&text, &path);
                        self.files.written = Some(self.tuned());
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
    use crate::config::layout::Layout;

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
    fn the_wheel_backdrop_goes_by_name_and_keeps_its_value_on_a_bad_one() {
        let mut cfg = Settings::default();
        assert_eq!(cfg.wheel_backdrop, Backdrop::Octagon);
        cfg.apply("wheel_backdrop", "circle_dark   # try");
        assert_eq!(cfg.wheel_backdrop, Backdrop::CircleDark);
        cfg.apply("wheel_backdrop", "hexagon");
        cfg.apply("wheel_backdrop", "1");
        assert_eq!(cfg.wheel_backdrop, Backdrop::CircleDark);
        assert!(cfg.describe().contains(" wheel_backdrop=circle_dark "));
    }

    #[test]
    fn the_wheel_place_goes_by_name() {
        let mut cfg = Settings::default();
        assert_eq!(cfg.wheel_place, WheelPlace::Center);
        cfg.apply("wheel_place", "cursor");
        cfg.apply("wheel_place", "corner");
        assert_eq!(cfg.wheel_place, WheelPlace::Cursor);
        assert!(cfg.values().contains(&("wheel_place", "cursor".to_string())));
    }

    #[test]
    fn config_values_take_their_lines_and_keep_the_comments() {
        let text = "# head\n[config]\nzoom = 1   # start level\ntap_hold = 0.2\n\n[controls]\na = space\n";
        let out = set_config(text, &[(ZOOM_KEY, "2".to_string()), (TEXT_SCALE_KEY, "1.5".to_string())]);
        assert_eq!(
            out,
            "# head\n[config]\nzoom = 2   # start level\ntap_hold = 0.2\ntext_scale = 1.5\n\n[controls]\na = space\n"
        );
        let (_, cfg) = Layout::parse(&out);
        assert_eq!((cfg.zoom, cfg.text_scale), (2.0, 1.5));
        assert_eq!(
            set_config("[controls]\na = space", &[(ZOOM_KEY, "2".to_string())]),
            "[config]\nzoom = 2\n[controls]\na = space"
        );
    }

    #[test]
    fn the_shipped_layout_declares_the_current_version() {
        assert_eq!(layout_version(DEFAULT_LAYOUT), LAYOUT_VERSION);
        assert_eq!(layout_version("[controls]\na = space\n"), 0);
    }

    #[test]
    fn an_upgrade_takes_the_default_with_the_old_settings() {
        let old = "[config]\nzoom = 2  # mine\ncursor_speed = 900\nbogus = 1\n[controls]\na = space\n";
        let new = upgraded(old);
        assert_eq!(layout_version(&new), LAYOUT_VERSION);
        let (layout, cfg) = Layout::parse(&new);
        assert_eq!((cfg.zoom, cfg.cursor_speed), (2.0, 900.0));
        assert_eq!(layout.wheels.len(), Layout::parse(DEFAULT_LAYOUT).0.wheels.len());
        assert!(!new.contains("bogus") && !new.contains("a = space"));
    }

    #[test]
    fn an_old_layout_file_gives_way_and_stays_beside_it() {
        let path = std::env::temp_dir().join(format!("ftlhelm-test-upgrade-{}.ini", std::process::id()));
        let previous = path.with_extension(PREVIOUS_LAYOUT_EXT);
        let old = "[config]\nzoom = 2\n";
        std::fs::write(&path, old).expect("a writable temp dir");
        let mut l = Layer::default();
        let text = l.upgrade_layout(&path, old.to_string());
        assert_eq!(text, upgraded(old));
        assert_eq!(std::fs::read_to_string(&path).ok(), Some(text.clone()));
        assert_eq!(std::fs::read_to_string(&previous).ok().as_deref(), Some(old));
        assert!(l.notice.is_some());
        l.notice = None;
        assert_eq!(l.upgrade_layout(&path, text.clone()), text, "a current file stays");
        assert!(l.notice.is_none());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&previous);
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
    fn a_missing_file_gets_the_text_and_an_existing_one_stays() {
        let path = std::env::temp_dir().join(format!("ftlhelm-test-missing-{}.ini", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(write_if_missing(&path, "a").expect("a writable temp dir"));
        assert!(!write_if_missing(&path, "b").expect("a writable temp dir"));
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some("a"));
        let _ = std::fs::remove_file(&path);
        assert!(write_if_missing(&path.join("ftlhelm.ini"), "a").is_err());
    }

    #[test]
    fn the_shipped_layout_holds_the_built_in_defaults() {
        let (_, mut shipped) = Layout::parse(DEFAULT_LAYOUT);
        assert_eq!(shipped.describe(), Settings::default().describe());
    }

    #[test]
    fn every_setting_is_in_the_shipped_layout_and_the_docs() {
        let docs = include_str!("../../docs/CONFIGURATION.md");
        for (key, _) in OPTIONS {
            assert!(DEFAULT_LAYOUT.lines().any(|l| l.starts_with(&format!("{key} ="))), "{key} not in ftlhelm.ini");
            assert!(docs.contains(&format!("`{key}`")), "{key} not in CONFIGURATION.md");
        }
    }

    #[test]
    fn the_frame_filter_switches_off_and_sharpen_stays_in_range() {
        let mut cfg = Settings::default();
        cfg.apply("frame_filter", "0");
        assert!(!cfg.frame_filter);
        cfg.apply("sharpen", "3");
        assert_eq!(cfg.sharpen, SHARPEN_MAX);
        cfg.apply("sharpen", "-1");
        assert_eq!(cfg.sharpen, 0.0);
    }

    #[test]
    fn tunable_numbers_step_on_their_grid_and_cycle_back() {
        let mut cfg = Settings::default();
        let rumble = tunable(RUMBLE_KEY).expect("rumble has a step");
        assert_eq!(cfg.tuning(rumble), Tuning::Steps { value: RUMBLE, step: RUMBLE_STEP, max: RUMBLE_MAX });
        cfg.step(rumble, true);
        assert_eq!(cfg.rumble, RUMBLE + RUMBLE_STEP);
        cfg.rumble = 0.3;
        cfg.step(rumble, true);
        assert_eq!(cfg.rumble, RUMBLE_STEP, "off the grid, the next step up");
        cfg.rumble = 0.3;
        cfg.step(rumble, false);
        assert_eq!(cfg.rumble, 0.0);
        cfg.step(rumble, false);
        assert_eq!(cfg.rumble, 0.0, "the bottom holds");
        cfg.rumble = RUMBLE_MAX;
        cfg.cycle(rumble);
        assert_eq!(cfg.rumble, 0.0, "the top cycles to the lowest");
        let speed = tunable(SPEED_LEVEL_KEY).expect("speed_level has a step");
        cfg.cycle(speed);
        assert_eq!(cfg.speed_level, SPEED_LEVEL_MIN as i32);
        cfg.cycle(speed);
        assert_eq!(cfg.speed_level, SPEED_LEVEL_MIN as i32 + 1);
        let overlay = tunable(OVERLAY_KEY).expect("a switch");
        cfg.cycle(overlay);
        assert_eq!(cfg.tuning(overlay), Tuning::Switch(false));
        cfg.step(overlay, true);
        assert!(cfg.overlay);
        assert_eq!((tunable("cursor_speed"), tunable("bogus")), (None, None));
    }

    #[test]
    fn only_the_changed_settings_are_written() {
        let mut cfg = Settings::default();
        let before = cfg.values();
        (cfg.sharpen, cfg.overlay) = (0.75, false);
        let changed = differing(&before, &cfg.values());
        assert_eq!(changed, [(OVERLAY_KEY, "0".to_string()), (SHARPEN_KEY, "0.75".to_string())]);
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
