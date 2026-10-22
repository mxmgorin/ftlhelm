//! Pad reading through the SIL joystick API: face labels, sticks and triggers as digital
//! inputs, flicks, and the per-frame dispatch.

use crate::config::settings::FLICK_OFF;
use crate::config::layout::{Input, Slot, SlotMode, INPUT_COUNT, SIL_BUTTONS, SLOT_COUNT};
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::actions::{mouse_move, request_exit, tap_key};
use crate::game::api::api;
use crate::game::mem::Obj;
use crate::game::offsets::KEY_SPACE;
use crate::game::state;
use crate::game::zones::ship_center;

/// PortMaster's per-device SDL mapping; its button indices match SIL's for pads whose
/// buttons all sit in the gamepad key range.
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
/// Face buttons the mapping names, with the inputs they are.
const FACE_LABELS: [(&str, Input); 4] =
    [("a", Input::FaceDown), ("b", Input::FaceRight), ("x", Input::FaceLeft), ("y", Input::FaceUp)];

/// One frame of pad state.
#[derive(Default)]
pub struct Pad {
    pub down: [bool; INPUT_COUNT],
    pub stick: [[f32; 2]; 2],
    pub dpad: [i32; 2],
}

pub struct PadState {
    /// SIL button index per face label from the SDL mapping.
    sdl_face: [Option<i32>; 4],
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
            sdl_face: [None; 4],
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
    [horizontal && x <= -STICK_KEY_ON, !horizontal && y <= -STICK_KEY_ON, horizontal && x >= STICK_KEY_ON, !horizontal && y >= STICK_KEY_ON]
}

/// The first connected pad.
pub fn active_pad() -> Option<i32> {
    let a = api();
    // SAFETY: the engine's joystick queries take any index below the count.
    unsafe { (0..(a.joy_count)()).find(|&i| (a.joy_connected)(i) != 0) }
}

fn read_stick(dev: i32, stick: i32) -> (f32, f32) {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    // SAFETY: the engine writes both axes of a stick of a connected pad.
    unsafe { (api().joy_read_stick)(dev, stick, &mut x, &mut y) };
    (x, y)
}

impl PadState {
    /// Reads a:bN .. y:bN out of the SDL mapping string.
    pub fn load_sdl_map(&mut self) {
        let Ok(map) = std::env::var(SDL_MAP_ENV) else {
            return;
        };
        for (i, (label, _)) in FACE_LABELS.iter().enumerate() {
            let token = format!(",{label}:b");
            self.sdl_face[i] = map.find(&token).and_then(|at| {
                let digits: String = map[at + token.len()..].chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            });
        }
        let show = |f: Option<i32>| f.unwrap_or(-1);
        log!("sdl map: a={} b={} x={} y={}", show(self.sdl_face[0]), show(self.sdl_face[1]), show(self.sdl_face[2]), show(self.sdl_face[3]));
    }
}

impl Layer {
    pub fn read_pad(&self, dev: i32) -> Pad {
        let a = api();
        let mut p = Pad::default();
        for b in 0..SIL_BUTTONS {
            // SAFETY: the engine's button queries on a connected pad.
            let idx = unsafe { (a.joy_button_mapping)(dev, b as i32) };
            p.down[b] = idx >= 0 && unsafe { (a.joy_button_state)(dev, idx) } != 0;
        }
        let faces = self.pad.sdl_face;
        if self.cfg.nintendo < 0 && faces.iter().all(Option::is_some) {
            for ((_, input), idx) in FACE_LABELS.iter().zip(faces.into_iter().flatten()) {
                // SAFETY: as above.
                p.down[*input as usize] = unsafe { (a.joy_button_state)(dev, idx) } != 0;
            }
        } else if self.cfg.nintendo != 0 {
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
                // SAFETY: as above.
                if unsafe { (a.joy_button_mapping)(dev, input as i32) } < 0 {
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
    pub fn apply_pad(&mut self, mut p: Pad, t: f64, dt: f32) {
        const DIR: [(f32, f32); 4] = [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0)];
        let mut mode = [SlotMode::Keys; SLOT_COUNT];
        for slot in Slot::ALL {
            let mut m = self.slot_mode(slot);
            // The mode toggle: cursor on the D-pad and the sticks, or zones and flicks.
            if self.wheel.open.is_none() {
                if slot == Slot::Dpad && self.bind.dpad_mouse {
                    m = SlotMode::Mouse;
                }
                if slot != Slot::Dpad && !self.bind.dpad_mouse && m == SlotMode::Mouse {
                    m = SlotMode::Flick;
                }
            }
            if m != SlotMode::Keys {
                p.down[slot.first_input()..slot.first_input() + 4].fill(false);
            }
            mode[slot as usize] = m;
        }
        let in_wheel = self.wheel.open.is_some();
        if in_wheel {
            self.wheel.refresh_wait -= 1;
            if self.wheel.refresh_wait <= 0 {
                self.refresh_wheel();
            }
            self.wheel_point(p.stick[0], p.stick[1]);
            if (state::aiming() && !self.wheel.opened_aiming) || !self.wheel_screen() {
                self.wheel_close();
            }
            if !state::aiming() {
                self.wheel.opened_aiming = false;
            }
        }
        let snapping = self.cfg.snap_dpad
            && self.bind.depth() == 0
            && !self.zones.targets.is_empty()
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
                            0 => self.wheel_step(-1),
                            2 => self.wheel_step(1),
                            1 => self.wheel_power(1),
                            _ => self.wheel_power(-1),
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
                self.release(input, t);
            }
        }

        if !in_wheel {
            for (k, slot) in [Slot::LStick, Slot::RStick].into_iter().enumerate() {
                let [x, y] = p.stick[k];
                match mode[slot as usize] {
                    SlotMode::Mouse => self.move_cursor(x, y, dt),
                    SlotMode::Flick => {
                        let mag = x.hypot(y);
                        let repeat = self.cfg.flick_repeat > 0.0 && t - self.pad.flick_last[k] >= self.cfg.flick_repeat as f64;
                        if mag < FLICK_OFF {
                            self.pad.flick_armed[k] = true;
                        } else if mag >= self.cfg.flick_on && (self.pad.flick_armed[k] || repeat) {
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

        // pause_ff held past the threshold: unpause and run fast.
        if self.bind.pause_ff_since.is_some_and(|s| t - s >= self.cfg.pause_ff_hold as f64) && !self.actions.fast_forward {
            if state::game_paused() {
                tap_key(KEY_SPACE, false);
            }
            self.set_fast(true);
        }

        // Mode changes read from the game: the cursor starts where the payload wants it.
        let now_aiming = state::aiming();
        let now_crew = state::first_selected_crew();
        if now_aiming && !self.pad.prev_aiming {
            if let (Some(gui), Some(enemy)) = (state::gui(), state::ship_of(state::enemy_complete())) {
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

    #[test]
    fn sticks_keep_one_direction_with_hysteresis() {
        assert_eq!(stick_directions(-0.6, 0.1, &[false; 4]), [true, false, false, false]);
        assert_eq!(stick_directions(-0.4, 0.45, &[true, false, false, false]), [true, false, false, false]);
        assert_eq!(stick_directions(-0.2, 0.45, &[true, false, false, false]), [false; 4]);
        assert_eq!(stick_directions(0.1, 0.7, &[false; 4]), [false, false, false, true]);
    }
}
