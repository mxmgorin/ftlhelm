//! Layout parsing: gptokeyb2-style [controls] states, HOLD | TAP bindings, builtins, and
//! [wheel:NAME] sections with plate labels and icons.

use crate::config::settings::Settings;
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::offsets::{KEY_ENTER, KEY_ESCAPE, KEY_F1, KEY_F_COUNT, KEY_LSHIFT, KEY_SPACE};

const CONFIG_SECTION: &str = "config";
const BASE_SECTION: &str = "controls";
const STATE_SECTION: &str = "controls:";
const WHEEL_SECTION: &str = "wheel:";
pub const MAX_STATES: usize = 8;
pub const MAX_WHEELS: usize = 8;
pub const WHEEL_SLOTS: usize = 8;
/// Labels and names keep this many characters, as the plates have room for.
pub const LABEL_CHARS: usize = 23;

/// Digital inputs: SIL's named buttons in enum order, then the D-pad and both sticks read
/// as four directions each.
#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    Home, Start, Select, FaceUp, FaceLeft, FaceRight, FaceDown,
    L1, R1, L2, R2, L3, R3,
    DpadLeft, DpadUp, DpadRight, DpadDown,
    LsLeft, LsUp, LsRight, LsDown,
    RsLeft, RsUp, RsRight, RsDown,
}

/// Counts through the last variant; a variant added after it takes its place here.
pub const INPUT_COUNT: usize = Input::RsDown as usize + 1;
/// The inputs that are SIL buttons, the rest being read from the D-pad and sticks.
pub const SIL_BUTTONS: usize = Input::R3 as usize + 1;

impl Input {
    #[rustfmt::skip]
    pub const ALL: [Input; INPUT_COUNT] = [
        Input::Home, Input::Start, Input::Select, Input::FaceUp, Input::FaceLeft, Input::FaceRight,
        Input::FaceDown, Input::L1, Input::R1, Input::L2, Input::R2, Input::L3, Input::R3,
        Input::DpadLeft, Input::DpadUp, Input::DpadRight, Input::DpadDown,
        Input::LsLeft, Input::LsUp, Input::LsRight, Input::LsDown,
        Input::RsLeft, Input::RsUp, Input::RsRight, Input::RsDown,
    ];

    /// Input names as the ini spells them; face buttons by label, A being FaceDown.
    #[rustfmt::skip]
    pub const INI_NAMES: [&'static str; INPUT_COUNT] = [
        "guide", "start", "back", "y", "x", "b", "a", "l1", "r1", "l2", "r2", "l3", "r3",
        "left", "up", "right", "down",
        "left_analog_left", "left_analog_up", "left_analog_right", "left_analog_down",
        "right_analog_left", "right_analog_up", "right_analog_right", "right_analog_down",
    ];

    pub fn ini_name(self) -> &'static str {
        Input::INI_NAMES[self as usize]
    }
}

/// Whole-pointer sources a state can bind to mouse_movement or flick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Dpad,
    LStick,
    RStick,
}

/// Counts through the last variant; a variant added after it takes its place here.
pub const SLOT_COUNT: usize = Slot::RStick as usize + 1;

impl Slot {
    pub const ALL: [Slot; SLOT_COUNT] = [Slot::Dpad, Slot::LStick, Slot::RStick];
    const INI_NAMES: [&'static str; SLOT_COUNT] = ["dpad", "left_analog", "right_analog"];

    /// The first of the source's four directions, in left, up, right, down order.
    pub fn first_input(self) -> usize {
        match self {
            Slot::Dpad => Input::DpadLeft as usize,
            Slot::LStick => Input::LsLeft as usize,
            Slot::RStick => Input::RsLeft as usize,
        }
    }
}

/// What a whole-pointer source does: keys per direction, the cursor, or flicks between zones.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SlotMode {
    #[default]
    Keys,
    Mouse,
    Flick,
}

#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Builtin {
    Cancel, ContextClick, ArmNext, AimCrew, RepeatAbility, Precision, CenterSwap,
    PrevTab, NextTab, Help, RoomDoors, Pause, FastForward, DpadMode,
}

impl Builtin {
    /// Counts through the last variant; a variant added after it takes its place here.
    const COUNT: usize = Builtin::DpadMode as usize + 1;
    #[rustfmt::skip]
    pub const ALL: [Builtin; Builtin::COUNT] = [
        Builtin::Cancel, Builtin::ContextClick, Builtin::ArmNext, Builtin::AimCrew,
        Builtin::RepeatAbility, Builtin::Precision, Builtin::CenterSwap, Builtin::PrevTab,
        Builtin::NextTab, Builtin::Help, Builtin::RoomDoors, Builtin::Pause, Builtin::FastForward,
        Builtin::DpadMode,
    ];
    #[rustfmt::skip]
    const NAMES: [&'static str; Builtin::COUNT] = [
        "cancel", "context_click", "arm_next", "aim_crew", "repeat_ability",
        "precision", "center_swap", "prev_tab", "next_tab", "help", "room_doors", "pause",
        "fast_forward", "dpad_mode",
    ];

    pub fn name(self) -> &'static str {
        Builtin::NAMES[self as usize]
    }

    fn from_name(name: &str) -> Option<Builtin> {
        Builtin::NAMES.iter().position(|&n| n == name).map(|i| Builtin::ALL[i])
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Key { code: i32, shift: bool },
    LClick,
    RClick,
    State(usize),
    Wheel(usize),
    Builtin(Builtin),
}

/// A button's action, and the action a short press runs instead when there is one.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Binding {
    pub hold: Action,
    pub tap: Option<Action>,
}

#[derive(Clone, Debug)]
pub struct State {
    pub name: String,
    /// Lookups fall through to the state below.
    pub parent: bool,
    pub bind: [Option<Binding>; INPUT_COUNT],
    pub slot: [SlotMode; SLOT_COUNT],
}

impl State {
    fn new(name: &str) -> State {
        State { name: name.to_string(), parent: false, bind: [None; INPUT_COUNT], slot: [SlotMode::Keys; SLOT_COUNT] }
    }
}

/// An action wheel closes after performing a plate, the others stay open; crew and
/// systems fill their plates from the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WheelKind {
    #[default]
    Action,
    Power,
    Crew,
    Systems,
    Settings,
}

impl WheelKind {
    #[rustfmt::skip]
    const NAMES: [(&'static str, WheelKind); 5] = [
        ("action", WheelKind::Action), ("power", WheelKind::Power), ("crew", WheelKind::Crew),
        ("systems", WheelKind::Systems), ("settings", WheelKind::Settings),
    ];
}

/// A plate icon: a game image path under img/, and a region of it in texture pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct IconSpec {
    pub path: String,
    pub region: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub struct PlateDef {
    pub action: Action,
    pub label: String,
    pub icon: Option<IconSpec>,
}

#[derive(Clone, Debug, Default)]
pub struct WheelDef {
    pub name: String,
    pub kind: WheelKind,
    /// Eight plates clockwise from the top.
    pub plates: [Option<PlateDef>; WHEEL_SLOTS],
}

impl WheelDef {
    const SLOT_NAMES: [&'static str; WHEEL_SLOTS] =
        ["up", "up_right", "right", "down_right", "down", "down_left", "left", "up_left"];
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub states: Vec<State>,
    pub wheels: Vec<WheelDef>,
}

#[rustfmt::skip]
const KEY_NAMES: &[(&str, i32)] = &[
    ("space", KEY_SPACE), ("enter", KEY_ENTER), ("return", KEY_ENTER), ("esc", KEY_ESCAPE),
    ("escape", KEY_ESCAPE), ("tab", 9), ("backspace", 8), ("delete", 127), ("up", 273),
    ("down", 274), ("right", 275), ("left", 276), ("insert", 277), ("home", 278), ("end", 279),
    ("pageup", 280), ("pagedown", 281), ("shift", KEY_LSHIFT), ("lshift", KEY_LSHIFT),
    ("rshift", 303), ("ctrl", 306), ("lctrl", 306), ("alt", 308), ("lalt", 308), ("/", 47),
    ("-", 45), ("=", 61), (",", 44), (".", 46), (";", 59), ("'", 39), ("[", 91), ("]", 93),
    ("\\", 92), ("`", 96), ("+", 43),
];

/// Maps an ini key name to the game's key code.
fn key_code(name: &str) -> Option<i32> {
    let bytes = name.as_bytes();
    if bytes.len() == 1 && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit()) {
        return Some(bytes[0] as i32);
    }
    if bytes.len() <= 3 && bytes.first() == Some(&b'f') && bytes.get(1).is_some_and(u8::is_ascii_digit) {
        if let Ok(n) = name[1..].parse::<i32>() {
            if (1..=KEY_F_COUNT).contains(&n) {
                return Some(KEY_F1 + n - 1);
            }
        }
    }
    KEY_NAMES.iter().find(|(n, _)| *n == name).map(|&(_, c)| c)
}

/// Names a key code as the ini spells it.
pub fn key_name(code: i32) -> Option<&'static str> {
    KEY_NAMES.iter().find(|(_, c)| *c == code).map(|&(n, _)| n)
}

/// Keeps the first `LABEL_CHARS` characters.
pub fn truncate_label(s: &str) -> String {
    s.chars().take(LABEL_CHARS).collect()
}

impl Layout {
    fn find_state(&mut self, name: &str) -> Option<usize> {
        if let Some(i) = self.states.iter().position(|s| s.name == name) {
            return Some(i);
        }
        if self.states.len() >= MAX_STATES {
            return None;
        }
        self.states.push(State::new(name));
        Some(self.states.len() - 1)
    }

    fn find_wheel(&mut self, name: &str) -> Option<usize> {
        if let Some(i) = self.wheels.iter().position(|w| w.name == name) {
            return Some(i);
        }
        if self.wheels.len() >= MAX_WHEELS {
            return None;
        }
        self.wheels.push(WheelDef { name: name.to_string(), ..WheelDef::default() });
        Some(self.wheels.len() - 1)
    }

    /// Parses one action ("key [add_shift]", mouse_left, mouse_right, hold_state NAME,
    /// wheel NAME, a builtin name) for the named input.
    fn parse_action(&mut self, key: &str, value: &str) -> Option<Action> {
        let mut toks = value.split_whitespace();
        let tok = toks.next()?;
        let mut action = if let Some(b) = Builtin::from_name(tok) {
            Action::Builtin(b)
        } else if tok == "mouse_left" {
            Action::LClick
        } else if tok == "mouse_right" {
            Action::RClick
        } else if tok == "hold_state" || tok == "wheel" {
            let idx =
                toks.next().and_then(|name| if tok == "wheel" { self.find_wheel(name) } else { self.find_state(name) });
            let Some(idx) = idx else {
                log!("layout: bad {tok} for {key}");
                return None;
            };
            if tok == "wheel" {
                Action::Wheel(idx)
            } else {
                Action::State(idx)
            }
        } else {
            let Some(code) = key_code(tok) else {
                log!("layout: unknown key {tok} for {key}");
                return None;
            };
            Action::Key { code, shift: false }
        };
        for extra in toks {
            match (&mut action, extra) {
                (Action::Key { shift, .. }, "add_shift") => *shift = true,
                _ => log!("layout: {extra} ignored for {key}"),
            }
        }
        Some(action)
    }

    /// Parses "HOLD | TAP" or a single action.
    fn parse_value(&mut self, key: &str, value: &str) -> Option<Binding> {
        let (hold, tap) = match value.split_once('|') {
            Some((hold, tap)) => (hold, self.parse_action(key, tap.trim())),
            None => (value, None),
        };
        let hold = self.parse_action(key, hold.trim())?;
        Some(Binding { hold, tap })
    }

    /// Parses one "input = value" line of a controls section into state `si`.
    fn parse_binding(&mut self, si: usize, key: &str, value: &str) {
        if key == "overlay" {
            self.states[si].parent = value == "parent";
            return;
        }
        if let Some(slot) = Slot::INI_NAMES.iter().position(|&n| n == key) {
            self.states[si].slot[slot] = match value {
                "mouse_movement" => SlotMode::Mouse,
                "flick" => SlotMode::Flick,
                other => {
                    if !other.is_empty() {
                        log!("layout: {key} = {other} not supported");
                    }
                    SlotMode::Keys
                }
            };
            return;
        }
        let Some(input) = Input::INI_NAMES.iter().position(|&n| n == key) else {
            log!("layout: unknown input {key}");
            return;
        };
        let binding = self.parse_value(key, value);
        self.states[si].bind[input] = binding;
    }

    /// Parses one line of a wheel section: "kind = NAME" or a direction with a binding, a
    /// quoted label and an optional icon.
    fn parse_wheel_line(&mut self, wi: usize, key: &str, value: &str) {
        if key == "kind" {
            match WheelKind::NAMES.iter().find(|(n, _)| *n == value) {
                Some(&(_, kind)) => self.wheels[wi].kind = kind,
                None => log!("layout: unknown wheel kind {value}"),
            }
            return;
        }
        let Some(slot) = WheelDef::SLOT_NAMES.iter().position(|&n| n == key) else {
            log!("layout: unknown wheel direction {key}");
            return;
        };
        let (action_text, label, icon) = match value.split_once('"') {
            Some((action, rest)) => match rest.split_once('"') {
                Some((label, icon)) => (action, label, parse_icon(icon.trim())),
                None => (action, rest, None),
            },
            None => (value, "", None),
        };
        let Some(binding) = self.parse_value(key, action_text) else {
            self.wheels[wi].plates[slot] = None;
            return;
        };
        let label = if label.is_empty() { key } else { label };
        self.wheels[wi].plates[slot] = Some(PlateDef { action: binding.hold, label: truncate_label(label), icon });
    }

    /// Parses layout text in the gptokeyb2 ini format, plus [wheel:NAME] sections, the base
    /// state at index 0; returns it with its [config] section over the default settings.
    /// A '#' starts a comment only outside quotes.
    pub fn parse(text: &str) -> (Layout, Settings) {
        let mut set = Layout::default();
        let mut settings = Settings::default();
        let base = set.find_state(BASE_SECTION).expect("an empty layout has room for the base state");
        let mut state_defined = [false; MAX_STATES];
        let mut wheel_defined = [false; MAX_WHEELS];
        enum Section {
            None,
            Config,
            State(usize),
            Wheel(usize),
        }
        let mut section = Section::None;
        for raw in text.lines() {
            let mut quoted = false;
            let end = raw
                .char_indices()
                .find(|&(_, c)| {
                    if c == '"' {
                        quoted = !quoted;
                    }
                    c == '#' && !quoted
                })
                .map_or(raw.len(), |(i, _)| i);
            let line = raw[..end].trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let name = rest.split(']').next().unwrap_or(rest);
                section = if name == CONFIG_SECTION {
                    Section::Config
                } else if name == BASE_SECTION {
                    Section::State(base)
                } else if let Some(state) = name.strip_prefix(STATE_SECTION) {
                    match set.find_state(state) {
                        Some(i) => {
                            state_defined[i] = true;
                            Section::State(i)
                        }
                        None => Section::None,
                    }
                } else if let Some(wheel) = name.strip_prefix(WHEEL_SECTION) {
                    match set.find_wheel(wheel) {
                        Some(i) => {
                            wheel_defined[i] = true;
                            Section::Wheel(i)
                        }
                        None => Section::None,
                    }
                } else {
                    Section::None
                };
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match section {
                Section::State(i) => set.parse_binding(i, key, value),
                Section::Wheel(i) => set.parse_wheel_line(i, key, value),
                Section::Config => settings.apply(key, value),
                Section::None => {}
            }
        }
        for (i, s) in set.states.iter().enumerate().skip(base + 1) {
            if !state_defined[i] {
                log!("layout: state {} has no section", s.name);
            }
        }
        for (i, w) in set.wheels.iter().enumerate() {
            if !wheel_defined[i] {
                log!("layout: wheel {} has no section", w.name);
            }
        }
        (set, settings)
    }
}

/// Parses a plate icon, "PATH" or "PATH@X,Y,W,H" with a region in texture pixels.
fn parse_icon(spec: &str) -> Option<IconSpec> {
    if spec.is_empty() {
        return None;
    }
    let (path, region) = match spec.split_once('@') {
        Some((path, region)) => {
            let nums: Vec<f32> = region.split(',').filter_map(|n| n.trim().parse().ok()).collect();
            let region = <[f32; 4]>::try_from(nums).ok();
            if region.is_none() {
                log!("layout: bad icon region {spec}");
            }
            (path, region)
        }
        None => (spec, None),
    };
    Some(IconSpec { path: path.to_string(), region })
}

impl Layer {
    /// Replaces the layout and the settings with those parsed from `text`, closing any wheel
    /// and dropping pushed states and pending presses.
    pub fn load_layout(&mut self, text: &str, origin: &str) {
        let (layout, cfg) = Layout::parse(text);
        self.close_wheel();
        log!("layout: {} states, {} wheels from {origin}", layout.states.len(), layout.wheels.len());
        self.cfg = cfg;
        log!("config:{}", self.cfg.describe());
        self.layout = layout;
        self.bind.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_lists_follow_declaration_order() {
        assert!(Input::ALL.iter().enumerate().all(|(i, &input)| input as usize == i));
        assert!(Slot::ALL.iter().enumerate().all(|(i, &slot)| slot as usize == i));
        assert!(Builtin::ALL.iter().enumerate().all(|(i, &b)| b as usize == i));
    }

    #[test]
    fn hold_and_tap() {
        let (l, _) = Layout::parse("[controls]\nx = wheel weapons | arm_next\n[wheel:weapons]\nkind = power\n");
        let b = l.states[0].bind[Input::FaceLeft as usize].expect("x is bound");
        assert_eq!(b.hold, Action::Wheel(0));
        assert_eq!(b.tap, Some(Action::Builtin(Builtin::ArmNext)));
        assert_eq!(l.wheels[0].kind, WheelKind::Power);
    }

    #[test]
    fn plate_with_label_and_icon_region() {
        let (l, _) =
            Layout::parse("[wheel:ship]\nup = z \"Open doors\" systemUI/button_door_bottom_off.png@10,33,20,20 # c\n");
        let p = l.wheels[0].plates[0].as_ref().expect("up is bound");
        assert_eq!(p.action, Action::Key { code: 'z' as i32, shift: false });
        assert_eq!(p.label, "Open doors");
        let icon = p.icon.as_ref().expect("an icon");
        assert_eq!(icon.path, "systemUI/button_door_bottom_off.png");
        assert_eq!(icon.region, Some([10.0, 33.0, 20.0, 20.0]));
    }

    #[test]
    fn keys_shift_and_slots() {
        let (l, _) =
            Layout::parse("[controls]\na = f3 add_shift\nleft_analog = mouse_movement\nright_analog = flick\nb = /\n");
        let s = &l.states[0];
        assert_eq!(
            s.bind[Input::FaceDown as usize].map(|b| b.hold),
            Some(Action::Key { code: KEY_F1 + 2, shift: true })
        );
        assert_eq!(
            s.bind[Input::FaceRight as usize].map(|b| b.hold),
            Some(Action::Key { code: '/' as i32, shift: false })
        );
        assert_eq!(s.slot, [SlotMode::Keys, SlotMode::Mouse, SlotMode::Flick]);
    }

    #[test]
    fn hold_state_pages() {
        let (l, _) = Layout::parse(
            "[controls]\nl1 = hold_state depower\n[controls:depower]\noverlay = parent\na = 1 add_shift\n",
        );
        assert_eq!(l.states.len(), 2);
        assert!(l.states[1].parent);
        assert_eq!(l.states[0].bind[Input::L1 as usize].map(|b| b.hold), Some(Action::State(1)));
    }

    #[test]
    fn hash_inside_quotes_is_kept() {
        let (l, _) = Layout::parse("[wheel:w]\nup = a \"No # comment\"\n");
        assert_eq!(l.wheels[0].plates[0].as_ref().map(|p| p.label.as_str()), Some("No # comment"));
    }

    #[test]
    fn config_section_sets_the_settings() {
        let (l, cfg) = Layout::parse("[config]\ntap_hold = 0.3 # s\nswap_ab = 0\n[controls]\na = space\n");
        assert_eq!(cfg.tap_hold, 0.3);
        assert!(!cfg.swap_ab);
        assert_eq!(cfg.cursor_speed, Settings::default().cursor_speed);
        assert!(l.states[0].bind[Input::FaceDown as usize].is_some());
    }

    #[test]
    fn shipped_scheme_reads_its_settings() {
        let (l, cfg) = Layout::parse(include_str!("../../config/ftlpad.ini"));
        assert_eq!(cfg.cursor_deadzone, 0.25);
        assert!(l.states[0].bind[Input::FaceDown as usize].is_some());
        assert!(!l.wheels.is_empty());
    }
}
