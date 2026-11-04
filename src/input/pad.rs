//! Pad reading through the SIL joystick API: face labels, sticks and triggers as digital
//! inputs, flicks, and the per-frame dispatch.

use crate::config::layout::{Input, Slot, SlotMode, INPUT_COUNT, SIL_BUTTONS, SLOT_COUNT};
use crate::config::settings::{AUTOPAUSE_AIM, FLICK_OFF};
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::actions::{mouse_move, toggle_pause};
use crate::game::api::api;
use crate::game::mem::Obj;
use crate::game::offsets::{
    EVDEV_INFO_SIZE, EVDEV_KEY_BITS, EVDEV_KEY_CODES, EVDEV_NAME, EVDEV_NAME_LEN, SILJOY_EVENT_DEV, SILJOY_KEYMAP_HIGH,
    SILJOY_KEYMAP_LOW, SILJOY_NUM_BUTTONS, SILJOY_PRODUCT, SILJOY_SIZE, SILJOY_VENDOR, SIL_TABLE_LEN,
};
use crate::game::state;
use crate::game::zones::ship_center;
use crate::input::sdl_map::{
    self, Resolved, SdlMap, BTN_JOYSTICK, BTN_TRIGGER_HAPPY, SIL_KEYMAP_HIGH_LEN, SIL_KEYMAP_LOW_LEN,
};
use crate::ui::overlay::{Notice, PAD_LOST_NOTICE};

/// PortMaster's per-device SDL mapping.
const SDL_MAP_ENV: &str = "SDL_GAMECONTROLLERCONFIG";
/// The engine's pulse strength at `rumble` 1; `rumble` reaches the engine's full strength.
pub const RUMBLE_STRENGTH: f32 = 0.4;
pub const RUMBLE_MAX: f32 = 1.0 / RUMBLE_STRENGTH;
/// A bump rumbles this many times as long as a buzz.
const BUMP_TIMES: f32 = 3.0;
const STICK_KEY_ON: f32 = 0.5;
const STICK_KEY_OFF: f32 = 0.3;
/// SIL reports an analog trigger as a stick axis resting at -1.
const TRIGGER_ON: f32 = 0.0;
/// Frames between synthetic mouse moves that refresh the button zones.
const BUTTON_REFRESH_FRAMES: u64 = 6;
/// The D-pad's directions in input order: left, up, right, down.
const DPAD_DIRS: [(f32, f32); 4] = [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0)];
/// A held direction steps the focus again after FOCUS_REPEAT_DELAY_S, then at the interval
/// of the `focus_repeat` level, slowest first.
const FOCUS_REPEAT_DELAY_S: f64 = 0.35;
pub const FOCUS_REPEAT_S: [f64; 3] = [0.3, 0.2, 0.12];
/// The triggers in an open wheel, with the step each takes as D-pad down and up do.
const WHEEL_TRIGGERS: [(Input, i32); 2] = [(Input::L2, -1), (Input::R2, 1)];
const FACE_BUTTONS: [Input; 4] = [Input::FaceDown, Input::FaceRight, Input::FaceLeft, Input::FaceUp];
const LEFT_STICK: i32 = 0;
const RIGHT_STICK: i32 = 1;
const TRIGGER_STICK: i32 = 2;
/// The pad Steam Input feeds: Valve's vendor id and the product id of its virtual pad.
const STEAM_VIRTUAL_PAD: (u16, u16) = (0x28de, 0x11ff);

/// SIL pads, one bit per table entry.
type PadSet = u64;
const _: () = assert!(SIL_TABLE_LEN <= PadSet::BITS as usize, "a pad set holds every SIL pad");

#[derive(Default)]
pub struct PadFrame {
    pub down: [bool; INPUT_COUNT],
    pub stick: [[f32; 2]; 2],
    pub dpad: [i32; 2],
}

/// Whether a pad frame holds any input: a button, the D-pad, or a stick past `deadzone`.
fn any_input(p: &PadFrame, deadzone: f32) -> bool {
    p.down.iter().any(|&d| d) || p.dpad != [0, 0] || p.stick.iter().any(|s| s[0].hypot(s[1]) > deadzone)
}

pub struct PadState {
    sdl: Option<SdlMap>,
    /// The mapping resolved for one pad, keyed by SIL pad and input device.
    resolved: Option<((i32, i32), Resolved)>,
    /// The engine's raw button behind each SIL button of one pad, keyed the same way.
    sil_mapping: Option<((i32, i32), [i32; SIL_BUTTONS])>,
    held: [bool; INPUT_COUNT],
    /// The A/B and X/Y swaps reads apply: the settings', taken up while no face button is down.
    swaps: (bool, bool),
    dpad_spent: [bool; 4],
    trigger_spent: [bool; WHEEL_TRIGGERS.len()],
    /// The D-pad direction whose focus step repeats while held, and when it steps next.
    dpad_repeat: Option<(usize, f64)>,
    /// When each stick held past its flick steps the focus again.
    flick_next: [f64; 2],
    flick_armed: [bool; 2],
    prev_aiming: bool,
    /// The aim autopause paused the game for the aim under way; the aim's end resumes it.
    pub aim_paused: bool,
    prev_crew: Option<Obj>,
    frame: u64,
    /// The active pad.
    pub index: Option<i32>,
    /// The connected pads and those with a button down, as last seen.
    connected: PadSet,
    down: PadSet,
}

/// What changed among the pads since the frame before.
#[derive(Default)]
struct PadChange {
    /// The active pad went away.
    lost: bool,
    /// A button went down on some pad.
    pressed: bool,
    /// A pad became active while none was.
    gained: bool,
}

impl Default for PadState {
    fn default() -> Self {
        PadState {
            sdl: None,
            resolved: None,
            sil_mapping: None,
            held: [false; INPUT_COUNT],
            swaps: (false, false),
            dpad_spent: [false; 4],
            trigger_spent: [false; WHEEL_TRIGGERS.len()],
            dpad_repeat: None,
            flick_next: [f64::INFINITY; 2],
            flick_armed: [true; 2],
            prev_aiming: false,
            aim_paused: false,
            prev_crew: None,
            frame: 0,
            index: None,
            connected: 0,
            down: 0,
        }
    }
}

/// Reads a stick as four directions with hysteresis; only the dominant axis is active.
fn stick_directions(x: f32, y: f32, was: &[bool]) -> [bool; 4] {
    if let Some(i) = was.iter().position(|&w| w) {
        let v = if i == 0 || i == 2 { x } else { y };
        let mut out = [false; 4];
        out[i] = if i <= 1 { v <= -STICK_KEY_OFF } else { v >= STICK_KEY_OFF };
        return out;
    }
    let horizontal = x.abs() >= y.abs();
    [
        horizontal && x <= -STICK_KEY_ON,
        !horizontal && y <= -STICK_KEY_ON,
        horizontal && x >= STICK_KEY_ON,
        !horizontal && y >= STICK_KEY_ON,
    ]
}

fn bit(dev: i32) -> PadSet {
    1 << dev
}

fn lowest(pads: PadSet) -> Option<i32> {
    (pads != 0).then(|| pads.trailing_zeros() as i32)
}

/// The pads of a set, lowest first.
fn each(pads: PadSet) -> impl Iterator<Item = i32> {
    (0..SIL_TABLE_LEN as i32).filter(move |&dev| pads & bit(dev) != 0)
}

fn joystick(dev: i32) -> Option<Obj> {
    // SAFETY: SIL's pad table, indexed by a connected pad.
    unsafe { Obj::table_entry(api().sil_joysticks, dev, SILJOY_SIZE) }
}

/// Whether pad `dev` is connected.
fn pad_connected(dev: i32) -> bool {
    let a = api();
    // SAFETY: the engine's pad count, and its query on an index below it.
    unsafe { dev < (a.joy_count)() && (a.joy_connected)(dev) != 0 }
}

/// Whether a frame of a pad holds a SIL button or a D-pad direction down.
fn holds_press(p: &PadFrame) -> bool {
    p.down[..SIL_BUTTONS].iter().any(|&d| d) || p.dpad != [0, 0]
}

/// Whether a button or D-pad direction of a connected pad is down.
fn any_button_down(dev: i32) -> bool {
    let a = api();
    let buttons = joystick(dev).map_or(0, |j| j.i32(SILJOY_NUM_BUTTONS));
    let (mut x, mut y) = (0, 0);
    // SAFETY: the engine's queries on a connected pad, its buttons below its count.
    unsafe {
        (a.joy_read_dpad)(dev, &mut x, &mut y);
        x != 0 || y != 0 || (0..buttons).any(|b| (a.joy_button_state)(dev, b) != 0)
    }
}

/// A connected pad's device name, vendor and product.
fn identity(dev: i32) -> (String, u16, u16) {
    let Some(joy) = joystick(dev) else {
        return (String::new(), 0, 0);
    };
    // SAFETY: SIL's device table, indexed by the pad's device.
    let info = unsafe { Obj::table_entry(api().sil_event_info, joy.i32(SILJOY_EVENT_DEV), EVDEV_INFO_SIZE) };
    let name = info.map_or_else(String::new, |info| {
        let bytes: [u8; EVDEV_NAME_LEN] = info.get(EVDEV_NAME);
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(EVDEV_NAME_LEN);
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    });
    (name, joy.get(SILJOY_VENDOR), joy.get(SILJOY_PRODUCT))
}

/// A pointer source's mode under the mode toggle: with `free_cursor` the D-pad and every
/// pointer stick move the cursor, else the sticks flick and the D-pad keeps the layout's
/// mode. A stick the layout binds as keys stays keys.
pub fn toggled_mode(slot: Slot, layout_mode: SlotMode, free_cursor: bool) -> SlotMode {
    match (slot, layout_mode, free_cursor) {
        (Slot::Dpad, _, true) => SlotMode::Mouse,
        (Slot::Dpad, m, false) => m,
        (_, SlotMode::Keys, _) => SlotMode::Keys,
        (_, _, true) => SlotMode::Mouse,
        (_, _, false) => SlotMode::Flick,
    }
}

fn read_stick(dev: i32, stick: i32) -> (f32, f32) {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    // SAFETY: the engine writes both axes of a stick of a connected pad.
    unsafe { (api().joy_read_stick)(dev, stick, &mut x, &mut y) };
    (x, y)
}

impl PadState {
    /// Reads the SDL mapping's button entries.
    pub fn load_sdl_map(&mut self) {
        self.sdl = std::env::var(SDL_MAP_ENV).ok().and_then(|m| sdl_map::parse(&m));
        log!("sdl map: {}", if self.sdl.is_some() { "buttons named" } else { "none" });
    }

    /// Resolves the SDL mapping against the pad's key codes, once per pad and device.
    fn resolve_sdl_map(&mut self, dev: i32) {
        let Some(map) = &self.sdl else {
            return;
        };
        let a = api();
        let Some(joy) = joystick(dev) else {
            return;
        };
        let event_dev = joy.i32(SILJOY_EVENT_DEV);
        if self.resolved.as_ref().is_some_and(|(k, _)| *k == (dev, event_dev)) {
            return;
        }
        // SAFETY: SIL's device table, indexed by the pad's device.
        let Some(info) = (unsafe { Obj::table_entry(a.sil_event_info, event_dev, EVDEV_INFO_SIZE) }) else {
            return;
        };
        let bits: [u8; EVDEV_KEY_CODES / 8] = info.get(EVDEV_KEY_BITS);
        let codes: Vec<u16> =
            (0..EVDEV_KEY_CODES as u16).filter(|&c| bits[c as usize / 8] >> (c % 8) & 1 != 0).collect();
        let sil_button = |code: u16| {
            let slot = if (BTN_JOYSTICK..BTN_JOYSTICK + SIL_KEYMAP_LOW_LEN).contains(&code) {
                SILJOY_KEYMAP_LOW + (code - BTN_JOYSTICK) as usize
            } else if (BTN_TRIGGER_HAPPY..BTN_TRIGGER_HAPPY + SIL_KEYMAP_HIGH_LEN).contains(&code) {
                SILJOY_KEYMAP_HIGH + (code - BTN_TRIGGER_HAPPY) as usize
            } else {
                return None;
            };
            let raw = joy.get::<i8>(slot);
            (raw >= 0).then_some(raw as i32)
        };
        let r = sdl_map::resolve(map, &codes, sil_button);
        let show = |i: Input| r.buttons[i as usize].unwrap_or(-1);
        log!(
            "sdl map on pad {dev}: a={} b={} x={} y={} start={} back={}, {} order",
            show(Input::FaceDown),
            show(Input::FaceRight),
            show(Input::FaceLeft),
            show(Input::FaceUp),
            show(Input::Start),
            show(Input::Select),
            if r.ascending { "ascending" } else { "gamepad-first" }
        );
        self.resolved = Some(((dev, event_dev), r));
    }

    /// Reads the engine's mapping of the pad's SIL buttons, once per pad and device.
    fn resolve_sil_mapping(&mut self, dev: i32) {
        let key = (dev, joystick(dev).map_or(-1, |j| j.i32(SILJOY_EVENT_DEV)));
        if self.sil_mapping.is_some_and(|(k, _)| k == key) {
            return;
        }
        let a = api();
        // SAFETY: the engine's button mapping on a connected pad.
        let mapping = std::array::from_fn(|b| unsafe { (a.joy_button_mapping)(dev, b as i32) });
        self.sil_mapping = Some((key, mapping));
    }

    /// Follows the connected pads, logging each that comes or goes. The active pad gives way
    /// to one starting a press while it has none down; a lost one to the first left.
    /// `active_held` stands in for the active pad's own scan, read from its frame.
    fn follow_pads(&mut self, active_held: Option<bool>) -> PadChange {
        let a = api();
        // SAFETY: the engine's pad count.
        let count = unsafe { (a.joy_count)() }.clamp(0, SIL_TABLE_LEN as i32);
        let (mut connected, mut down) = (0, 0);
        for dev in 0..count {
            // SAFETY: the engine's joystick queries take any index below the count.
            if unsafe { (a.joy_connected)(dev) } != 0 {
                connected |= bit(dev);
                let held = match active_held {
                    Some(held) if Some(dev) == self.index => held,
                    _ => any_button_down(dev),
                };
                if held {
                    down |= bit(dev);
                }
            }
        }
        let mut change = PadChange::default();
        for dev in each(connected & !self.connected) {
            let (name, vendor, product) = identity(dev);
            log!("pad {dev}: {name} ({vendor:04x}:{product:04x})");
            if (vendor, product) == STEAM_VIRTUAL_PAD {
                log!("pad {dev}: fed by Steam Input");
            }
        }
        for dev in each(self.connected & !connected) {
            log!("pad {dev}: gone");
        }
        let pressed = down & !self.down;
        (self.connected, self.down) = (connected, down);
        change.pressed = pressed != 0;
        let active = self.index.filter(|&dev| connected & bit(dev) != 0);
        change.lost = self.index.is_some() && active.is_none();
        let next = match active {
            Some(dev) if down & bit(dev) != 0 => Some(dev),
            Some(dev) => lowest(pressed).or(Some(dev)),
            None => lowest(pressed).or(lowest(connected)),
        };
        if let Some(dev) = next.filter(|&dev| Some(dev) != self.index) {
            log!("pad {dev}: active");
        }
        change.gained = self.index.is_none() && next.is_some();
        self.index = next;
        change
    }
}

impl Layer {
    /// Reads the active pad, its SDL mapping resolved on first sight; an idle frame while none
    /// is connected. The help at start waits for a pad and the ship view; losing the active
    /// pad pauses the ship view; `dt` ages the notices.
    pub fn poll_pad(&mut self, dt: f32) -> PadFrame {
        // The active pad first: whether it holds a press decides a handover.
        let was = self.pad.index.filter(|&dev| pad_connected(dev));
        let first = was.map(|dev| self.read_active(dev));
        let change = self.pad.follow_pads(first.as_ref().map(holds_press));
        self.age_notice(dt, change.pressed);
        if self.cfg.help_at_start && !self.help.shown && self.pad.index.is_some() && state::ship_view() {
            self.show_help(true);
        }
        if change.lost {
            if state::ship_view() && !state::game_paused() {
                toggle_pause();
                log!("pad lost: paused");
            }
            self.notice = Some(Notice::until_press(PAD_LOST_NOTICE));
        }
        let Some(dev) = self.pad.index else {
            return PadFrame::default();
        };
        match first {
            Some(p) if was == Some(dev) => p,
            _ => self.read_active(dev),
        }
    }

    /// Reads pad `dev`, its mappings resolved on first sight; a changed swap waits for the
    /// face buttons to be up, so none turns into another while held.
    fn read_active(&mut self, dev: i32) -> PadFrame {
        self.pad.resolve_sdl_map(dev);
        self.pad.resolve_sil_mapping(dev);
        let p = self.read_pad(dev);
        if !FACE_BUTTONS.iter().any(|&b| p.down[b as usize]) {
            self.pad.swaps = (self.cfg.swap_ab, self.cfg.swap_xy);
        }
        p
    }

    /// Whether the A/B and X/Y swaps reach the active pad: not while an SDL mapping names its
    /// buttons.
    pub fn face_swaps_apply(&self) -> bool {
        !self.pad.resolved.as_ref().is_some_and(|(k, _)| Some(k.0) == self.pad.index)
    }

    fn read_pad(&self, dev: i32) -> PadFrame {
        let a = api();
        let mut p = PadFrame::default();
        let sdl = self.pad.resolved.as_ref().filter(|(k, _)| k.0 == dev).map(|(_, r)| r);
        let mapping = self.pad.sil_mapping.filter(|(k, _)| k.0 == dev).map(|(_, m)| m);
        let raw_button = |b: usize| match sdl {
            Some(r) => r.buttons[b].unwrap_or(-1),
            None => mapping.map_or(-1, |m| m[b]),
        };
        // SAFETY: the engine's button queries on a connected pad.
        let pressed = |raw: i32| raw >= 0 && unsafe { (a.joy_button_state)(dev, raw) } != 0;
        for b in 0..SIL_BUTTONS {
            p.down[b] = pressed(raw_button(b));
        }
        if sdl.is_none() {
            if self.pad.swaps.0 {
                p.down.swap(Input::FaceDown as usize, Input::FaceRight as usize);
            }
            if self.pad.swaps.1 {
                p.down.swap(Input::FaceLeft as usize, Input::FaceUp as usize);
            }
        }
        // SAFETY: as above.
        if unsafe { (a.joy_num_sticks)(dev) } > TRIGGER_STICK {
            let (l2, r2) = read_stick(dev, TRIGGER_STICK);
            for (input, value) in [(Input::L2, l2), (Input::R2, r2)] {
                if raw_button(input as usize) < 0 {
                    p.down[input as usize] = value > TRIGGER_ON;
                }
            }
        }
        // SAFETY: the engine writes both D-pad axes.
        unsafe { (a.joy_read_dpad)(dev, &mut p.dpad[0], &mut p.dpad[1]) };
        p.down[Input::DpadLeft as usize] = p.dpad[0] < 0;
        p.down[Input::DpadRight as usize] = p.dpad[0] > 0;
        p.down[Input::DpadUp as usize] = p.dpad[1] < 0;
        p.down[Input::DpadDown as usize] = p.dpad[1] > 0;
        if let Some(r) = sdl {
            for (k, raw) in r.dpad.iter().enumerate() {
                p.down[Input::DpadLeft as usize + k] |= pressed(raw.unwrap_or(-1));
            }
            let axis = |neg: Input, pos: Input| p.down[pos as usize] as i32 - p.down[neg as usize] as i32;
            p.dpad = [axis(Input::DpadLeft, Input::DpadRight), axis(Input::DpadUp, Input::DpadDown)];
        }
        for (k, (stick, first)) in [(LEFT_STICK, Input::LsLeft), (RIGHT_STICK, Input::RsLeft)].into_iter().enumerate() {
            let (x, y) = read_stick(dev, stick);
            p.stick[k] = [x, y];
            let f = first as usize;
            let dirs = stick_directions(x, y, &self.pad.held[f..f + 4]);
            p.down[f..f + 4].copy_from_slice(&dirs);
        }
        p
    }

    pub fn buzz(&self) {
        self.rumble_for(self.cfg.rumble_time);
    }

    /// Rumbles longer than a buzz, for a step that hit a limit.
    pub fn bump(&self) {
        self.rumble_for(self.cfg.rumble_time * BUMP_TIMES);
    }

    fn rumble_for(&self, seconds: f32) {
        if let (true, Some(dev)) = (self.cfg.rumble > 0.0, self.pad.index) {
            let s = self.cfg.rumble * RUMBLE_STRENGTH;
            // SAFETY: the engine's rumble on a connected pad.
            unsafe { (api().joy_rumble)(dev, s, s, seconds) };
        }
    }

    /// Makes the game hit-test its buttons at the cursor: a mouse move in place, in window
    /// pixels of the current transform.
    fn refresh_buttons(&mut self) {
        (self.cursor.last_wx, self.cursor.last_wy) = self.to_window(self.cursor.x, self.cursor.y);
        mouse_move(self.cursor.last_wx, self.cursor.last_wy, 0, 0, false, false);
    }

    /// Applies one frame of pad state: pointer sources move the cursor or flick between
    /// zones, sticks point an open wheel, everything else goes through the layout.
    pub fn apply_pad(&mut self, mut p: PadFrame, t: f64, dt: f32) {
        if any_input(&p, self.cfg.cursor_deadzone) {
            self.leave_touch();
        }
        let mode = self.pointer_modes(&mut p);
        let in_wheel = self.wheel.open.is_some();
        if in_wheel {
            self.tick_wheel(&p);
        }
        let snapping = self.cfg.snap_dpad && !self.zones.list.is_empty() && mode[Slot::Dpad as usize] == SlotMode::Keys;
        self.pad.frame += 1;
        if self.pad.frame.is_multiple_of(BUTTON_REFRESH_FRAMES)
            && self.cursor.synced
            && !self.cursor.l_held
            && !self.cursor.r_held
            && !self.actions.clicks_queued()
        {
            self.refresh_buttons();
        }
        self.settle_pending(t);
        self.apply_buttons(&p, t, in_wheel, snapping);
        if in_wheel || self.help.shown {
            self.spend_flicks(&p);
        } else {
            self.move_pointers(&p, mode, t, dt);
            self.repeat_dpad(t);
            self.repeat_beam(self.held_direction(&p, mode));
        }
        self.follow_game_modes();
    }

    /// Each pointer source's mode this frame, after the mode toggle; the directions of a
    /// source that does not act as keys are cleared from `p`.
    fn pointer_modes(&self, p: &mut PadFrame) -> [SlotMode; SLOT_COUNT] {
        let mut mode = [SlotMode::Keys; SLOT_COUNT];
        for slot in Slot::ALL {
            let m = if self.wheel.open.is_some() {
                self.slot_mode(slot)
            } else {
                toggled_mode(slot, self.slot_mode(slot), self.bind.dpad_mouse)
            };
            if m != SlotMode::Keys {
                p.down[slot.first_input()..slot.first_input() + 4].fill(false);
            }
            mode[slot as usize] = m;
        }
        mode
    }

    /// Refreshes the open wheel, points it with the sticks, and closes it when aiming starts
    /// other than at picked weapons, or the screen no longer allows it.
    fn tick_wheel(&mut self, p: &PadFrame) {
        self.wheel.refresh_wait -= 1;
        if self.wheel.refresh_wait <= 0 {
            self.refresh_wheel();
        }
        self.point_wheel(p.stick[0], p.stick[1]);
        let new_aim = state::aiming() && !self.wheel.opened_aiming && !self.wheel.picking;
        if new_aim || !self.open_wheel_allowed() {
            self.close_wheel();
        }
        if !state::aiming() {
            self.wheel.opened_aiming = false;
        }
    }

    /// Turns button changes into presses and releases; with a wheel open, or snapping on and
    /// no state held, the D-pad steps the wheel, a held armament box or the focus instead,
    /// and its release is swallowed. A trigger pressed in a wheel steps it as D-pad down or up.
    fn apply_buttons(&mut self, p: &PadFrame, t: f64, in_wheel: bool, snapping: bool) {
        for input in Input::ALL {
            let i = input as usize;
            if p.down[i] == self.pad.held[i] {
                continue;
            }
            self.pad.held[i] = p.down[i];
            if p.down[i] {
                self.settle_pages();
            }
            // The help takes every press; that press does nothing else.
            if self.help.shown && p.down[i] {
                self.help_press(input);
                continue;
            }
            if (Input::DpadLeft as usize..=Input::DpadDown as usize).contains(&i) {
                let d = i - Input::DpadLeft as usize;
                if p.down[i] && (in_wheel || (snapping && self.bind.depth() == 0)) {
                    self.pad.dpad_spent[d] = true;
                    if in_wheel {
                        match d {
                            0 => self.wheel_sideways(-1),
                            2 => self.wheel_sideways(1),
                            1 => self.adjust_plate(1),
                            _ => self.adjust_plate(-1),
                        }
                    } else if self.focus_drag(DPAD_DIRS[d].0, DPAD_DIRS[d].1) {
                        self.pad.dpad_repeat = None;
                    } else {
                        self.focus_move(DPAD_DIRS[d].0, DPAD_DIRS[d].1);
                        self.pad.dpad_repeat = Some((d, self.next_step_at(t, true)));
                    }
                    continue;
                }
                if !p.down[i] && self.pad.dpad_spent[d] {
                    self.pad.dpad_spent[d] = false;
                    continue;
                }
            }
            if let Some(k) = WHEEL_TRIGGERS.iter().position(|&(trigger, _)| trigger == input) {
                if p.down[i] && in_wheel {
                    self.pad.trigger_spent[k] = true;
                    self.adjust_plate(WHEEL_TRIGGERS[k].1);
                    continue;
                }
                if !p.down[i] && self.pad.trigger_spent[k] {
                    self.pad.trigger_spent[k] = false;
                    continue;
                }
            }
            if p.down[i] {
                self.press(input, t);
            } else {
                self.release(input);
            }
        }
    }

    /// The direction a D-pad step or a flicking stick holds this frame, the D-pad first.
    fn held_direction(&self, p: &PadFrame, mode: [SlotMode; SLOT_COUNT]) -> Option<(f32, f32)> {
        let dpad =
            (0..DPAD_DIRS.len()).find(|&d| self.pad.held[Input::DpadLeft as usize + d] && self.pad.dpad_spent[d]);
        if let Some(d) = dpad {
            return Some(DPAD_DIRS[d]);
        }
        [Slot::LStick, Slot::RStick]
            .into_iter()
            .enumerate()
            .filter(|&(_, slot)| mode[slot as usize] == SlotMode::Flick)
            .map(|(k, _)| p.stick[k])
            .find(|&[x, y]| x.hypot(y) >= self.cfg.flick_threshold)
            .map(|[x, y]| (x, y))
    }

    /// Moves the cursor or flicks between zones from the sticks and a cursor-mode D-pad.
    fn move_pointers(&mut self, p: &PadFrame, mode: [SlotMode; SLOT_COUNT], t: f64, dt: f32) {
        for (k, slot) in [Slot::LStick, Slot::RStick].into_iter().enumerate() {
            let [x, y] = p.stick[k];
            match mode[slot as usize] {
                SlotMode::Mouse => self.move_cursor(x, y, dt),
                SlotMode::Flick => {
                    let mag = x.hypot(y);
                    let first = self.pad.flick_armed[k];
                    // A beam being set repeats its own steps.
                    let repeat = !self.beam_active() && t >= self.pad.flick_next[k];
                    if mag < FLICK_OFF {
                        self.pad.flick_armed[k] = true;
                    } else if mag >= self.cfg.flick_threshold && (first || repeat) {
                        self.pad.flick_armed[k] = false;
                        self.pad.flick_next[k] = self.next_step_at(t, first);
                        self.focus_move(x, y);
                    }
                }
                SlotMode::Keys => {}
            }
        }
        if mode[Slot::Dpad as usize] == SlotMode::Mouse && (p.dpad[0] != 0 || p.dpad[1] != 0) {
            self.move_cursor(p.dpad[0] as f32, p.dpad[1] as f32, dt);
        }
    }

    /// Spends the flick of every stick off centre: it flicks again once re-centred.
    fn spend_flicks(&mut self, p: &PadFrame) {
        for (k, [x, y]) in p.stick.into_iter().enumerate() {
            if x.hypot(y) >= FLICK_OFF {
                self.pad.flick_armed[k] = false;
                self.pad.flick_next[k] = f64::INFINITY;
            }
        }
    }

    /// Returns when a direction that stepped the focus at `t` steps again while held, after
    /// its `first` step or a repeat; never with focus_repeat off.
    fn next_step_at(&self, t: f64, first: bool) -> f64 {
        let level = usize::try_from(self.cfg.focus_repeat - 1).ok();
        match level.and_then(|i| FOCUS_REPEAT_S.get(i)) {
            Some(_) if first => t + FOCUS_REPEAT_DELAY_S,
            Some(&every) => t + every,
            None => f64::INFINITY,
        }
    }

    /// Steps the focus again the way the held D-pad direction stepped it, once due; not while
    /// a beam being set repeats its own steps.
    fn repeat_dpad(&mut self, t: f64) {
        let Some((d, at)) = self.pad.dpad_repeat else {
            return;
        };
        if !self.pad.held[Input::DpadLeft as usize + d] {
            self.pad.dpad_repeat = None;
        } else if t >= at && !self.beam_active() {
            self.pad.dpad_repeat = Some((d, self.next_step_at(t, false)));
            self.focus_move(DPAD_DIRS[d].0, DPAD_DIRS[d].1);
        }
    }

    /// Reacts to mode changes read from the game: a new aim moves the cursor to the enemy
    /// ship, a new crew selection with crew_follow to the member's room. A volley is one aim;
    /// an aim taken in an open wheel counts once the wheel closes.
    fn follow_game_modes(&mut self) {
        let now_aiming = state::aiming() || self.actions.volley.is_some();
        let now_crew = state::first_selected_crew();
        if self.wheel.open.is_none() {
            // Resumed by hand: a later pause is the player's.
            if !state::game_paused() {
                self.pad.aim_paused = false;
            }
            if now_aiming && !self.pad.prev_aiming {
                if let (Some(gui), Some(enemy)) = (state::gui(), state::enemy_ship()) {
                    if let Some((ex, ey)) = ship_center(enemy, state::enemy_origin(gui)) {
                        (self.cursor.x, self.cursor.y) = (ex, ey);
                        self.send_cursor();
                    }
                }
                if self.cfg.autopauses(AUTOPAUSE_AIM) && !state::game_paused() {
                    toggle_pause();
                    self.pad.aim_paused = true;
                }
            } else if !now_aiming && self.pad.aim_paused {
                toggle_pause();
                self.pad.aim_paused = false;
            }
            self.pad.prev_aiming = now_aiming;
        }
        if now_crew.is_some() && now_crew != self.pad.prev_crew && self.cfg.crew_follow {
            self.cursor_to_selected_crew();
        }
        self.pad.prev_crew = now_crew;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::Layout;
    use crate::core::util::now_s;
    use crate::game::api::Rect;
    use crate::game::fake::{calls, set_pad, set_pads, with_game, Call, FakePad};
    use crate::game::offsets::{layout, KEY_SPACE, MOUSE_AIMING};
    use crate::game::zones::{Zone, ZoneKind};
    use crate::input::bindings::Source;

    const DT: f32 = 1.0 / 60.0;

    /// Sets the stand-in pad's raw buttons apart from the SIL buttons they stand behind.
    const RAW_OFFSET: i32 = 100;

    fn raw(input: Input) -> i32 {
        input as i32 + RAW_OFFSET
    }

    fn held(p: &PadFrame) -> Vec<Input> {
        Input::ALL.into_iter().filter(|&i| p.down[i as usize]).collect()
    }

    /// A pad at `index` with A mapped, held when `pressed`.
    fn pad(index: i32, pressed: bool) -> FakePad {
        let a = raw(Input::FaceDown);
        FakePad {
            index: Some(index),
            mapping: vec![a],
            held: if pressed { vec![a] } else { vec![] },
            ..FakePad::default()
        }
    }

    #[test]
    fn polling_reads_the_connected_pad_or_idles() {
        with_game(|_| {
            let mut l = Layer::default();
            let idle = l.poll_pad(0.0);
            assert_eq!((l.pad.index, held(&idle)), (None, vec![]));
            set_pad(FakePad {
                index: Some(0),
                mapping: vec![raw(Input::Home)],
                held: vec![raw(Input::Home)],
                ..FakePad::default()
            });
            let frame = l.poll_pad(0.0);
            assert_eq!((l.pad.index, held(&frame)), (Some(0), vec![Input::Home]));
        });
    }

    #[test]
    fn a_connected_pad_reads_through_the_engine_mapping() {
        with_game(|_| {
            let mut mapping: Vec<i32> = (0..SIL_BUTTONS as i32).map(|b| b + RAW_OFFSET).collect();
            for trigger in [Input::L2, Input::R2] {
                mapping[trigger as usize] = -1;
            }
            set_pad(FakePad {
                index: Some(1),
                mapping,
                held: vec![raw(Input::Start), raw(Input::FaceDown)],
                sticks: [[-0.6, 0.1], [0.0, 0.9], [0.5, -1.0]],
                dpad: [1, 0],
                ..FakePad::default()
            });
            let mut l = Layer::default();
            l.poll_pad(0.0);
            assert_eq!(l.pad.index, Some(1));
            let p = l.read_pad(1);
            let expected = [Input::Start, Input::FaceDown, Input::L2, Input::DpadRight, Input::LsLeft, Input::RsDown];
            assert_eq!(held(&p), expected);
            assert_eq!((p.stick, p.dpad), ([[-0.6, 0.1], [0.0, 0.9]], [1, 0]));
            l.pad.swaps.0 = true;
            assert_eq!(held(&l.read_pad(1))[..2], [Input::Start, Input::FaceRight]);
        });
    }

    #[test]
    fn a_changed_swap_waits_for_the_face_buttons_to_be_up() {
        with_game(|_| {
            let a = raw(Input::FaceDown);
            let mapping: Vec<i32> = (0..SIL_BUTTONS as i32).map(|b| b + RAW_OFFSET).collect();
            let pad = |held: Vec<i32>| FakePad { index: Some(0), mapping: mapping.clone(), held, ..FakePad::default() };
            let mut l = Layer::default();
            set_pad(pad(vec![a]));
            l.poll_pad(0.0);
            l.cfg.swap_ab = true;
            assert_eq!(held(&l.poll_pad(0.0)), [Input::FaceDown], "A held through the change stays A");
            set_pad(pad(vec![]));
            l.poll_pad(0.0);
            set_pad(pad(vec![a]));
            assert_eq!(held(&l.poll_pad(0.0)), [Input::FaceRight], "the next press is swapped");
        });
    }

    #[test]
    fn a_new_press_on_another_pad_takes_over_while_the_active_one_has_none_down() {
        with_game(|_| {
            let mut l = Layer::default();
            let mut step = |pads: [bool; 2]| {
                set_pads(vec![pad(0, pads[0]), pad(2, pads[1])]);
                l.poll_pad(0.0);
                l.pad.index
            };
            assert_eq!(step([false, false]), Some(0), "the first connected pad starts");
            assert_eq!(step([true, true]), Some(0), "the active pad keeps a shared press");
            assert_eq!(step([false, true]), Some(0), "a held button is no new press");
            step([false, false]);
            assert_eq!(step([false, true]), Some(2));
            assert_eq!(step([true, true]), Some(2), "the active pad holds its press");
        });
    }

    #[test]
    fn the_help_at_start_waits_for_a_pad_on_the_ship_view_until_it_is_closed_once() {
        with_game(|g| {
            let mut l = Layer::default();
            set_pads(vec![pad(0, false)]);
            l.poll_pad(0.0);
            assert!(!l.help.shown, "no ship view: no help");
            set_pads(vec![]);
            l.poll_pad(0.0);
            g.player_manager();
            l.poll_pad(0.0);
            assert!(!l.help.shown, "no pad: no help");
            set_pads(vec![pad(0, false)]);
            l.poll_pad(0.0);
            assert!(l.help.shown);
            l.show_help(false);
            set_pads(vec![]);
            l.poll_pad(0.0);
            set_pads(vec![pad(0, false)]);
            l.poll_pad(0.0);
            assert!(!l.help.shown, "closed once: not again");
        });
    }

    #[test]
    fn the_aim_autopause_lasts_until_the_aim_ends_unless_resumed_by_hand() {
        with_game(|g| {
            let mut l = Layer::default();
            l.cfg.autopause = AUTOPAUSE_AIM;
            let space = [Call::KeyDown(KEY_SPACE), Call::KeyUp(KEY_SPACE)];
            let frame = |l: &mut Layer, aiming: i32, paused: bool| {
                g.mouse.set(MOUSE_AIMING, aiming);
                g.gui.set(layout().gui_paused, paused);
                l.follow_game_modes();
                calls()
            };
            assert_eq!(frame(&mut l, 1, false), space, "the aim pauses");
            assert_eq!(frame(&mut l, 1, true), []);
            assert_eq!(frame(&mut l, 0, true), space, "its end resumes");
            frame(&mut l, 1, false);
            frame(&mut l, 1, false);
            assert_eq!(frame(&mut l, 0, true), [], "paused by hand after a resume by hand");
        });
    }

    #[test]
    fn a_stick_held_through_a_wheel_flicks_only_after_it_recentres() {
        with_game(|_| {
            let mut l = Layer::default();
            let layout = "[controls]\nleft_analog = flick\n[wheel:settings]\nkind = settings\nup = set overlay\n";
            l.layout = Layout::parse(layout).0;
            let zone = |x| Zone { rect: Rect { x, y: 100, w: 40, h: 40 }, id: 0, kind: ZoneKind::Button };
            l.zones.list = vec![zone(100), zone(200)];
            (l.cursor.x, l.cursor.y) = (120.0, 120.0);
            l.send_cursor();
            let stick = |l: &mut Layer, x: f32| {
                let mut p = PadFrame::default();
                p.stick[0] = [x, 0.0];
                l.apply_pad(p, now_s(), DT);
                l.zone_under_cursor()
            };
            l.open_wheel(0, Source::Plate);
            assert_eq!(stick(&mut l, 1.0), Some(0), "the stick points the wheel");
            l.close_wheel();
            assert_eq!(stick(&mut l, 1.0), Some(0), "held through the close");
            stick(&mut l, 0.0);
            assert_eq!(stick(&mut l, 1.0), Some(1), "a fresh flick");
        });
    }

    /// A layer with a row of zones and the cursor on the first, the left stick flicking.
    fn zone_row() -> Layer {
        let mut l = Layer::default();
        l.layout = Layout::parse("[controls]\nleft_analog = flick\n").0;
        let zone = |i| Zone { rect: Rect { x: 100 * i, y: 100, w: 40, h: 40 }, id: 0, kind: ZoneKind::Button };
        l.zones.list = (1..=6).map(zone).collect();
        (l.cursor.x, l.cursor.y) = (120.0, 120.0);
        l.send_cursor();
        l
    }

    fn frame(l: &mut Layer, dpad_right: bool, stick: [f32; 2], t: f64) -> Option<usize> {
        let mut p = PadFrame::default();
        p.down[Input::DpadRight as usize] = dpad_right;
        p.stick[0] = stick;
        l.apply_pad(p, t, DT);
        l.zone_under_cursor()
    }

    #[test]
    fn a_held_direction_repeats_its_step_after_a_delay_at_the_level_interval() {
        with_game(|_| {
            let right = [1.0, 0.0];
            for (dpad, stick) in [(true, [0.0; 2]), (false, right)] {
                let mut l = zone_row();
                l.cfg.focus_repeat = 2;
                let at = |l: &mut Layer, t| frame(l, dpad, stick, t);
                assert_eq!(at(&mut l, 1.0), Some(1), "the press steps");
                assert_eq!(at(&mut l, 1.0 + FOCUS_REPEAT_DELAY_S - 0.01), Some(1), "the delay first");
                let first = 1.0 + FOCUS_REPEAT_DELAY_S;
                assert_eq!(at(&mut l, first), Some(2), "then a repeat");
                assert_eq!(at(&mut l, first + FOCUS_REPEAT_S[1] - 0.01), Some(2));
                assert_eq!(at(&mut l, first + FOCUS_REPEAT_S[1]), Some(3), "then the level's interval");
                assert_eq!(frame(&mut l, false, [0.0; 2], first + 1.0), Some(3), "the release stops it");
            }
            let mut l = zone_row();
            l.cfg.focus_repeat = 0;
            assert_eq!(frame(&mut l, false, right, 1.0), Some(1));
            assert_eq!(frame(&mut l, false, right, 9.0), Some(1), "off: once per push");
        });
    }

    #[test]
    fn a_stick_resting_off_centre_re_arms_its_flick() {
        with_game(|_| {
            let mut l = zone_row();
            let rest = [-0.3, 0.1];
            assert_eq!(frame(&mut l, false, [1.0, 0.0], 1.0), Some(1));
            frame(&mut l, false, rest, 1.1);
            assert_eq!(frame(&mut l, false, [1.0, 0.0], 1.2), Some(2));
        });
    }

    #[test]
    fn triggers_in_a_wheel_step_the_plate_and_keep_their_own_actions_out() {
        with_game(|_| {
            let mut l = Layer::default();
            let layout = "[controls]\nl2 = precision | room_doors\nr2 = fast_forward | j\n\
                          [wheel:settings]\nkind = settings\nup = set overlay\n";
            l.layout = Layout::parse(layout).0;
            let mut p = PadFrame::default();
            let mut set = |l: &mut Layer, input: Input, down: bool, in_wheel: bool| {
                p.down[input as usize] = down;
                l.apply_buttons(&p, 1.0, in_wheel, false);
            };
            set(&mut l, Input::L2, true, false);
            l.settle_pending(f64::MAX);
            assert!(l.bind.precision, "held before the wheel");
            l.open_wheel(0, Source::Plate);
            l.wheel.sel = Some(0);
            set(&mut l, Input::L2, false, true);
            assert!(!l.bind.precision, "its release still ends the hold");
            set(&mut l, Input::L2, true, true);
            assert!(!l.cfg.overlay, "L2 steps down");
            set(&mut l, Input::R2, true, true);
            assert!(l.cfg.overlay, "R2 steps up");
            l.close_wheel();
            calls();
            set(&mut l, Input::L2, false, false);
            set(&mut l, Input::R2, false, false);
            assert!(!l.bind.precision);
            assert_eq!(calls(), [], "no doors, no jump on the releases");
        });
    }

    #[test]
    fn losing_the_active_pad_pauses_the_ship_view_and_says_so_until_a_press() {
        with_game(|g| {
            g.player_manager();
            let mut l = Layer::default();
            l.cfg.help_at_start = false;
            set_pads(vec![pad(0, false), pad(1, false)]);
            l.poll_pad(0.0);
            set_pads(vec![pad(1, false)]);
            l.poll_pad(0.0);
            assert_eq!(calls(), [Call::KeyDown(KEY_SPACE), Call::KeyUp(KEY_SPACE)]);
            assert_eq!(l.pad.index, Some(1));
            l.cursor.synced = true;
            l.poll_pad(f32::MAX);
            assert!(l.notice.is_some());
            set_pads(vec![pad(1, true)]);
            l.poll_pad(0.0);
            assert!(l.notice.is_none());
        });
    }

    #[test]
    fn a_pads_identity_is_its_name_and_ids() {
        with_game(|_| {
            let name = "Steam Virtual Gamepad";
            set_pads(vec![FakePad { name, id: STEAM_VIRTUAL_PAD, ..pad(0, false) }]);
            assert_eq!(identity(0), (name.to_string(), STEAM_VIRTUAL_PAD.0, STEAM_VIRTUAL_PAD.1));
        });
    }

    #[test]
    fn a_press_while_a_page_button_waits_acts_in_the_page() {
        with_game(|_| {
            let mut l = Layer::default();
            l.load_layout("[controls]\nl1 = hold_state page | pause\nup = 1\n[controls:page]\nup = 2\n", "test");
            let mut p = PadFrame::default();
            p.down[Input::L1 as usize] = true;
            l.apply_buttons(&p, 1.0, false, true);
            p.down[Input::DpadUp as usize] = true;
            l.apply_buttons(&p, 1.05, false, true);
            p.down = [false; INPUT_COUNT];
            l.apply_buttons(&p, 1.1, false, true);
            assert_eq!(l.bind.depth(), 0);
            assert_eq!(calls(), [Call::KeyDown('2' as i32), Call::KeyUp('2' as i32)]);
        });
    }

    #[test]
    fn the_toggle_switches_the_dpad_and_both_sticks() {
        for stick in [Slot::LStick, Slot::RStick] {
            for m in [SlotMode::Mouse, SlotMode::Flick] {
                assert_eq!(toggled_mode(stick, m, false), SlotMode::Flick);
                assert_eq!(toggled_mode(stick, m, true), SlotMode::Mouse);
            }
            assert_eq!(toggled_mode(stick, SlotMode::Keys, true), SlotMode::Keys);
        }
        assert_eq!(toggled_mode(Slot::Dpad, SlotMode::Keys, false), SlotMode::Keys);
        assert_eq!(toggled_mode(Slot::Dpad, SlotMode::Keys, true), SlotMode::Mouse);
    }

    #[test]
    fn sticks_keep_one_direction_with_hysteresis() {
        assert_eq!(stick_directions(-0.6, 0.1, &[false; 4]), [true, false, false, false]);
        assert_eq!(stick_directions(-0.4, 0.45, &[true, false, false, false]), [true, false, false, false]);
        assert_eq!(stick_directions(-0.2, 0.45, &[true, false, false, false]), [false; 4]);
        assert_eq!(stick_directions(0.1, 0.7, &[false; 4]), [false, false, false, true]);
    }
}
