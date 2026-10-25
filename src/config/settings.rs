//! Settings with their defaults and ranges, read from the layout's [config] section, the
//! mtime poll that reloads the layout while the game runs, and the write of the settings
//! wheel's values back into that section.

use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;

use crate::config::layout::{CONFIG_SECTION, DEFAULT_LAYOUT};
use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::ui::text::{TEXT_SCALE_AUTO, TEXT_SCALE_MAX};
use crate::ui::zoom::{ZOOM_AUTO, ZOOM_MAX};

const LAYOUT_ENV: &str = "FTLHELM_LAYOUT";
/// The keys the settings wheel writes back.
const ZOOM_KEY: &str = "zoom";
const TEXT_SCALE_KEY: &str = "text_scale";
const FILE_POLL_S: f64 = 1.0;

const SPEED_LEVEL_MIN: f32 = 1.0;
const SPEED_LEVEL_MAX: f32 = 6.0;
const RUMBLE_STRENGTH: f32 = 0.4;
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
    /// The ship view's zoom level; ZOOM_AUTO picks by screen size.
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
            zoom: ZOOM_AUTO,
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
    (TEXT_SCALE_KEY, Field::Float(|c| &mut c.text_scale, TEXT_SCALE_AUTO, TEXT_SCALE_MAX)),
    (ZOOM_KEY, Field::Float(|c| &mut c.zoom, ZOOM_AUTO, ZOOM_MAX)),
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
    /// The settings wheel's values as the layout file holds them.
    written: Option<Tuned>,
    /// Writing the default layout failed; no retry until a file appears at the path.
    default_failed: bool,
}

/// The settings the settings wheel changes and writes into the layout file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuned {
    pub zoom: f32,
    pub text_scale: f32,
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
    pub fn tuned(&self) -> Tuned {
        Tuned { zoom: self.zoom.level, text_scale: self.cfg.text_scale }
    }

    /// Writes the settings wheel's values into the layout file's [config] section when they
    /// differ from what it holds.
    pub fn save_tuned(&mut self) {
        let (Ok(path), t) = (std::env::var(LAYOUT_ENV), self.tuned()) else {
            return;
        };
        if self.files.written == Some(t) {
            return;
        }
        let path = Path::new(&path);
        let values = [(ZOOM_KEY, t.zoom.to_string()), (TEXT_SCALE_KEY, t.text_scale.to_string())];
        match std::fs::read_to_string(path).and_then(|text| write_whole(path, &set_config(&text, &values))) {
            Ok(()) => {
                self.files.written = Some(t);
                // The layer's own write is no edit to reload.
                self.files.layout_mtime = mtime(path);
                log!("saved: zoom {:.2}, text_scale {:.2}", t.zoom, t.text_scale);
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
    fn nan_takes_the_lower_bound() {
        let mut cfg = Settings::default();
        cfg.apply("cursor_speed", "nan");
        cfg.apply("speed_level", "nan");
        assert_eq!(cfg.cursor_speed, CURSOR_SPEED_MIN);
        assert_eq!(cfg.speed_level, SPEED_LEVEL_MIN as i32);
    }
}
