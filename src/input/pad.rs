//! Pad reading through the SIL joystick API: face labels, sticks and triggers as digital
//! inputs, flicks, and the per-frame dispatch.

use crate::config::layout::{Input, Slot, SlotMode, INPUT_COUNT, SIL_BUTTONS, SLOT_COUNT};
use crate::config::settings::FLICK_OFF;
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::actions::{mouse_move, request_exit, tap_key};
use crate::game::api::api;
use crate::game::mem::Obj;
use crate::game::offsets::{
    EVDEV_INFO_SIZE, EVDEV_KEY_BITS, EVDEV_KEY_CODES, KEY_SPACE, SILJOY_EVENT_DEV, SILJOY_KEYMAP_HIGH,
    SILJOY_KEYMAP_LOW, SILJOY_SIZE,
};
use crate::game::state;
use crate::game::zones::ship_center;
use crate::input::sdl_map::{
    self, Resolved, SdlMap, BTN_JOYSTICK, BTN_TRIGGER_HAPPY, SIL_KEYMAP_HIGH_LEN, SIL_KEYMAP_LOW_LEN,
};

/// PortMaster's per-device SDL mapping.
const SDL_MAP_ENV: &str = "SDL_GAMECONTROLLERCONFIG";
const STICK_KEY_ON: f32 = 0.5;
const STICK_KEY_OFF: f32 = 0.3;
/// SIL reports an analog trigger as a stick axis resting at -1.
const TRIGGER_ON: f32 = 0.0;
const EXIT_HOLD_S: f64 = 0.6;
/// Frames between synthetic mouse moves that refresh the button zones.
const BUTTON_REFRESH_FRAMES: u64 = 6;
const LEFT_STICK: i32 = 0;
const RIGHT_STICK: i32 = 1;
const TRIGGER_STICK: i32 = 2;

/// One frame of pad state.
#[derive(Default)]
pub struct PadFrame {
    pub down: [bool; INPUT_COUNT],
    pub stick: [[f32; 2]; 2],
    pub dpad: [i32; 2],
}

pub struct PadState {
    sdl: Option<SdlMap>,
    /// The mapping resolved for one pad, keyed by SIL pad and input device.
    resolved: Option<((i32, i32), Resolved)>,
    held: [bool; INPUT_COUNT],
    dpad_spent: [bool; 4],
    flick_last: [f64; 2],
    flick_armed: [bool; 2],
    prev_aiming: bool,
    prev_crew: Option<Obj>,
    chord_since: Option<f64>,
    exit_requested: bool,
    frame: u64,
    pub index: Option<i32>,
}

impl Default for PadState {
    fn default() -> Self {
        PadState {
            sdl: None,
            resolved: None,
            held: [false; INPUT_COUNT],
            dpad_spent: [false; 4],
            flick_last: [0.0; 2],
            flick_armed: [true; 2],
            prev_aiming: false,
            prev_crew: None,
            chord_since: None,
            exit_requested: false,
            frame: 0,
            index: None,
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

/// The first connected pad.
pub fn active_pad() -> Option<i32> {
    let a = api();
    // SAFETY: the engine's joystick queries take any index below the count.
    unsafe { (0..(a.joy_count)()).find(|&i| (a.joy_connected)(i) != 0) }
}

/// A pointer source's mode under the mode toggle: with `free_cursor` the D-pad and every
/// pointer stick move the cursor, else the sticks flick and the D-pad keeps the layout's
/// mode. A stick the layout binds as keys stays keys.
fn toggled_mode(slot: Slot, layout_mode: SlotMode, free_cursor: bool) -> SlotMode {
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
    pub fn resolve_sdl_map(&mut self, dev: i32) {
        let Some(map) = &self.sdl else {
            return;
        };
        let a = api();
        // SAFETY: SIL's pad table, indexed by a connected pad.
        let Some(joy) = (unsafe { Obj::table_entry(a.sil_joysticks, dev, SILJOY_SIZE) }) else {
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
}

impl Layer {
    pub fn read_pad(&self, dev: i32) -> PadFrame {
        let a = api();
        let mut p = PadFrame::default();
        let sdl = self.pad.resolved.as_ref().filter(|(k, _)| k.0 == dev).map(|(_, r)| r);
        // SAFETY: the engine's button queries on a connected pad.
        let raw_button = |b: usize| match sdl {
            Some(r) => r.buttons[b].unwrap_or(-1),
            None => unsafe { (a.joy_button_mapping)(dev, b as i32) },
        };
        // SAFETY: as above.
        let pressed = |raw: i32| raw >= 0 && unsafe { (a.joy_button_state)(dev, raw) } != 0;
        for b in 0..SIL_BUTTONS {
            p.down[b] = pressed(raw_button(b));
        }
        if sdl.is_none() {
            if self.cfg.swap_ab {
                p.down.swap(Input::FaceDown as usize, Input::FaceRight as usize);
            }
            if self.cfg.swap_xy {
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
        if let (true, Some(dev)) = (self.cfg.rumble, self.pad.index) {
            let s = self.cfg.rumble_strength;
            // SAFETY: the engine's rumble on a connected pad.
            unsafe { (api().joy_rumble)(dev, s, s, self.cfg.rumble_time) };
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
        let mode = self.pointer_modes(&mut p);
        let in_wheel = self.wheel.open.is_some();
        if in_wheel {
            self.tick_wheel(&p);
        }
        let snapping = self.cfg.snap_dpad
            && self.bind.depth() == 0
            && !self.zones.list.is_empty()
            && mode[Slot::Dpad as usize] == SlotMode::Keys;
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
        if !in_wheel {
            self.move_pointers(&p, mode, t, dt);
        }
        self.follow_game_modes();
        self.watch_exit_chord(t);
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
    /// or the screen no longer allows it.
    fn tick_wheel(&mut self, p: &PadFrame) {
        self.wheel.refresh_wait -= 1;
        if self.wheel.refresh_wait <= 0 {
            self.refresh_wheel();
        }
        self.point_wheel(p.stick[0], p.stick[1]);
        if (state::aiming() && !self.wheel.opened_aiming) || !self.wheel_screen() {
            self.close_wheel();
        }
        if !state::aiming() {
            self.wheel.opened_aiming = false;
        }
    }

    /// Turns button changes into presses and releases; with a wheel open or snapping on,
    /// the D-pad steps the wheel or the focus instead, and its release is swallowed.
    fn apply_buttons(&mut self, p: &PadFrame, t: f64, in_wheel: bool, snapping: bool) {
        const DIR: [(f32, f32); 4] = [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0)];
        for input in Input::ALL {
            let i = input as usize;
            if p.down[i] == self.pad.held[i] {
                continue;
            }
            self.pad.held[i] = p.down[i];
            // Any button closes the help; that press does nothing else.
            if self.help_shown && p.down[i] && i < SIL_BUTTONS {
                self.help_shown = false;
                continue;
            }
            if (Input::DpadLeft as usize..=Input::DpadDown as usize).contains(&i) {
                let d = i - Input::DpadLeft as usize;
                if p.down[i] && (in_wheel || snapping) {
                    self.pad.dpad_spent[d] = true;
                    if in_wheel {
                        match d {
                            0 => self.step_wheel(-1),
                            2 => self.step_wheel(1),
                            1 => self.adjust_plate(1),
                            _ => self.adjust_plate(-1),
                        }
                    } else {
                        self.focus_move(DIR[d].0, DIR[d].1);
                    }
                    continue;
                }
                if !p.down[i] && self.pad.dpad_spent[d] {
                    self.pad.dpad_spent[d] = false;
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

    /// Moves the cursor or flicks between zones from the sticks and a cursor-mode D-pad.
    fn move_pointers(&mut self, p: &PadFrame, mode: [SlotMode; SLOT_COUNT], t: f64, dt: f32) {
        for (k, slot) in [Slot::LStick, Slot::RStick].into_iter().enumerate() {
            let [x, y] = p.stick[k];
            match mode[slot as usize] {
                SlotMode::Mouse => self.move_cursor(x, y, dt),
                SlotMode::Flick => {
                    let mag = x.hypot(y);
                    let repeat =
                        self.cfg.flick_repeat > 0.0 && t - self.pad.flick_last[k] >= self.cfg.flick_repeat as f64;
                    if mag < FLICK_OFF {
                        self.pad.flick_armed[k] = true;
                    } else if mag >= self.cfg.flick_threshold && (self.pad.flick_armed[k] || repeat) {
                        self.pad.flick_armed[k] = false;
                        self.pad.flick_last[k] = t;
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

    /// Reacts to mode changes read from the game: the cursor starts where the payload wants it.
    fn follow_game_modes(&mut self) {
        let now_aiming = state::aiming();
        let now_crew = state::first_selected_crew();
        if now_aiming && !self.pad.prev_aiming {
            if let (Some(gui), Some(enemy)) = (state::gui(), state::enemy_ship()) {
                if let Some((ex, ey)) = ship_center(enemy, state::enemy_origin(gui)) {
                    (self.cursor.x, self.cursor.y) = (ex, ey);
                    self.send_cursor();
                }
            }
            if self.cfg.autopause_aim && !state::game_paused() {
                tap_key(KEY_SPACE, false);
            }
        }
        if now_crew.is_some() && now_crew != self.pad.prev_crew && self.cfg.crew_follow {
            self.cursor_to_selected_crew();
        }
        self.pad.prev_aiming = now_aiming;
        self.pad.prev_crew = now_crew;
    }

    /// Requests the exit once Select and Start are held together long enough.
    fn watch_exit_chord(&mut self, t: f64) {
        if self.pad.held[Input::Select as usize] && self.pad.held[Input::Start as usize] {
            let since = *self.pad.chord_since.get_or_insert(t);
            if !self.pad.exit_requested && t - since >= EXIT_HOLD_S {
                self.pad.exit_requested = true;
                log!("Select + Start: exit");
                request_exit();
            }
        } else {
            self.pad.chord_since = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{set_pad, with_game, FakePad};

    /// Sets the stand-in pad's raw buttons apart from the SIL buttons they stand behind.
    const RAW_OFFSET: i32 = 100;

    fn raw(input: Input) -> i32 {
        input as i32 + RAW_OFFSET
    }

    fn held(p: &PadFrame) -> Vec<Input> {
        Input::ALL.into_iter().filter(|&i| p.down[i as usize]).collect()
    }

    #[test]
    fn the_first_connected_pad_reads_through_the_engine_mapping() {
        with_game(|_| {
            assert_eq!(active_pad(), None);
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
            });
            assert_eq!(active_pad(), Some(1));
            let mut l = Layer::default();
            let p = l.read_pad(1);
            let expected = [Input::Start, Input::FaceDown, Input::L2, Input::DpadRight, Input::LsLeft, Input::RsDown];
            assert_eq!(held(&p), expected);
            assert_eq!((p.stick, p.dpad), ([[-0.6, 0.1], [0.0, 0.9]], [1, 0]));
            l.cfg.swap_ab = true;
            assert_eq!(held(&l.read_pad(1))[..2], [Input::Start, Input::FaceRight]);
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
