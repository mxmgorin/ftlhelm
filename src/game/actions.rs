//! What the layer does to the game: keys, Shift, clicks and queued clicks, room doors,
//! crew and weapon cycling, tabs, and the fast speed.

use std::sync::atomic::{AtomicI32, Ordering};

use super::api::api;
use super::offsets::*;
use super::state;
use super::zones::{ship_center, TargetKind};
use crate::core::layer::Layer;
use crate::core::util::log;

const SPEED_LEVEL_NORMAL: i32 = 0;
const STEP_CAP_FAST: f32 = 2.0;
/// Queued clicks: capacity, frames between phases, phases per click.
const MAX_QUEUED_CLICKS: usize = 16;
const QUEUED_CLICK_FRAMES: i32 = 4;
const QUEUED_CLICK_PHASES: i32 = 3;

/// How many holders want Shift down; the GetShiftState hook reads it.
static SHIFT_COUNT: AtomicI32 = AtomicI32::new(0);

pub fn shift_held() -> bool {
    SHIFT_COUNT.load(Ordering::Relaxed) > 0
}

pub fn shift_count() -> i32 {
    SHIFT_COUNT.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
}

/// Clicks queued for later frames, the fast speed's step cap, and its state.
#[derive(Default)]
pub struct Actions {
    queued: Vec<(f32, f32)>,
    queued_phase: i32,
    queued_wait: i32,
    /// The step cap CFPS::OnLoop reads once redirected, and the game's own value.
    pub step_cap: Option<usize>,
    pub step_cap_normal: f32,
    pub fast_forward: bool,
}

impl Actions {
    pub fn clicks_queued(&self) -> bool {
        !self.queued.is_empty()
    }
}

pub fn key_down(code: i32) {
    // SAFETY: the game's key handler on its CApp.
    unsafe { (api().key_down)(state::app_ptr(), code) }
}

pub fn key_up(code: i32) {
    // SAFETY: as for key_down.
    unsafe { (api().key_up)(state::app_ptr(), code) }
}

/// Holds Shift down while any caller wants it, in the engine's key state and in the flag
/// CApp::OnLoop samples once a frame for the hotkeys.
pub fn set_shift(on: bool) {
    let count = SHIFT_COUNT.fetch_add(if on { 1 } else { -1 }, Ordering::Relaxed) + if on { 1 } else { -1 };
    if let Some(app) = state::app() {
        app.set::<u8>(CAPP_SHIFT_HELD, (count > 0) as u8);
    }
    if on && count == 1 {
        key_down(KEY_LSHIFT);
    } else if !on && count == 0 {
        key_up(KEY_LSHIFT);
    }
}

pub fn tap_key(code: i32, shift: bool) {
    if shift {
        set_shift(true);
    }
    key_down(code);
    key_up(code);
    if shift {
        set_shift(false);
    }
}

pub fn mouse_move(wx: i32, wy: i32, dx: i32, dy: i32, left: bool, right: bool) {
    // SAFETY: the game's mouse handler on its CApp.
    unsafe { (api().mouse_move)(state::app_ptr(), wx, wy, dx, dy, left, false, right) }
}

pub fn mouse_button(button: Button, down: bool, wx: i32, wy: i32) {
    let a = api();
    let f = match (button, down) {
        (Button::Left, true) => a.lbutton_down,
        (Button::Left, false) => a.lbutton_up,
        (Button::Right, true) => a.rbutton_down,
        (Button::Right, false) => a.rbutton_up,
    };
    // SAFETY: the game's mouse handler on its CApp.
    unsafe { f(state::app_ptr(), wx, wy) }
}

pub fn request_exit() {
    // SAFETY: the game's exit request on its CApp.
    unsafe { (api().request_exit)(state::app_ptr()) }
}

impl Layer {
    /// Presses or releases a mouse button at the cursor.
    pub fn click(&mut self, button: Button, down: bool) {
        let (wx, wy) = self.to_window(self.cursor.x, self.cursor.y);
        mouse_button(button, down, wx, wy);
        match button {
            Button::Left => self.cursor.l_held = down,
            Button::Right => self.cursor.r_held = down,
        }
    }

    pub fn click_tap(&mut self, button: Button) {
        self.click(button, true);
        self.click(button, false);
    }

    /// Moves the cursor to the player's or the enemy's ship centre, whichever is farther.
    pub fn center_swap(&mut self) {
        let Some(gui) = state::gui() else {
            return;
        };
        let p = state::ship_of(state::player_complete()).and_then(|s| ship_center(s, gui.point(GUI_SHIP_POSITION)));
        let e = state::ship_of(state::enemy_complete()).and_then(|s| ship_center(s, state::enemy_origin(gui)));
        let (cx, cy) = (self.cursor.x, self.cursor.y);
        let dist = |(x, y): (f32, f32)| (x - cx).hypot(y - cy);
        let to = match (p, e) {
            (Some(p), Some(e)) => {
                if dist(p) < dist(e) {
                    e
                } else {
                    p
                }
            }
            (Some(p), None) => p,
            (None, Some(e)) => e,
            (None, None) => return,
        };
        self.cursor.x = to.0;
        self.cursor.y = to.1;
        self.send_cursor();
    }

    /// Moves the cursor to the room of the first selected crew member.
    pub fn cursor_to_selected_crew(&mut self) {
        let Some(member) = state::first_selected_crew() else {
            return;
        };
        let room = member.i32(CREW_ROOM);
        let kind = if member.i32(CREW_SHIP) == PLAYER_SHIP_ID { TargetKind::Room } else { TargetKind::EnemyRoom };
        let Some(t) = self.zones.targets.iter().find(|t| t.id == room && t.kind == kind).copied() else {
            return;
        };
        (self.cursor.x, self.cursor.y) = t.rect.center();
        self.send_cursor();
        if self.debug {
            log!("cursor to crew room {room}");
        }
    }

    fn click_at(&mut self, gx: f32, gy: f32) {
        if self.actions.queued.len() < MAX_QUEUED_CLICKS {
            self.actions.queued.push((gx, gy));
        }
    }

    /// Runs one step of the queued clicks: the game resolves what sits under the pointer a
    /// frame after the move, so each click takes a move, a press and a release.
    pub fn run_queued_clicks(&mut self) {
        let Some(&(gx, gy)) = self.actions.queued.first() else {
            return;
        };
        if self.actions.queued_wait > 0 {
            self.actions.queued_wait -= 1;
            return;
        }
        self.actions.queued_wait = QUEUED_CLICK_FRAMES - 1;
        let (wx, wy) = self.to_window(gx, gy);
        match self.actions.queued_phase {
            0 => mouse_move(wx, wy, 0, 0, false, false),
            1 => mouse_button(Button::Left, true, wx, wy),
            _ => {
                mouse_button(Button::Left, false, wx, wy);
                self.actions.queued.remove(0);
                if self.actions.queued.is_empty() {
                    let (wx, wy) = self.to_window(self.cursor.x, self.cursor.y);
                    mouse_move(wx, wy, 0, 0, false, false);
                }
            }
        }
        self.actions.queued_phase = (self.actions.queued_phase + 1) % QUEUED_CLICK_PHASES;
        self.cursor.last_set = state::mouse_point();
    }

    /// Toggles the doors of the player room under the cursor: closes them all while any is
    /// open, else opens them all, clicking each as the game toggles them.
    pub fn room_doors(&mut self) {
        let Some(under) = self.target_under_cursor().map(|i| self.zones.targets[i]) else {
            return;
        };
        if under.kind != TargetKind::Room {
            return;
        }
        let room = under.id;
        let (Some(gui), Some(ship)) = (state::gui(), state::ship_of(state::player_complete())) else {
            return;
        };
        let origin = gui.point(GUI_SHIP_POSITION);
        let doors: Vec<_> = [ship.at(SHIP_DOORS), ship.at(SHIP_AIRLOCKS)]
            .into_iter()
            .flat_map(|list| (0..list.vec_len(std::mem::size_of::<usize>())).filter_map(move |i| list.vec_obj(i)))
            .filter(|d| d.i32(DOOR_ROOM1) == room || d.i32(DOOR_ROOM2) == room)
            .collect();
        let any_open = doors.iter().any(|d| d.flag(DOOR_OPEN));
        for d in doors.iter().filter(|d| d.flag(DOOR_OPEN) == any_open) {
            self.click_at((origin.x + d.i32(DOOR_X)) as f32, (origin.y + d.i32(DOOR_Y)) as f32);
        }
        if self.debug {
            log!("room {room} doors {}", if any_open { "closed" } else { "opened" });
        }
    }

    /// Runs the game at the configured speed level, or back at the normal one.
    pub fn set_fast(&mut self, on: bool) {
        let level = if on { self.cfg.speed_level } else { SPEED_LEVEL_NORMAL };
        state::fps_control().set(FPS_LEVEL, level);
        if let Some(cap) = self.actions.step_cap {
            let normal = self.actions.step_cap_normal;
            // SAFETY: cap is the float redirect_step_cap placed in the trampoline page.
            unsafe { (cap as *mut f32).write(if on { normal * STEP_CAP_FAST } else { normal }) };
        }
        self.actions.fast_forward = on;
    }
}

/// Selects the living crew member after the first selected one, wrapping.
pub fn select_next_crew() {
    let count = (0..CREW_HOTKEYS).take_while(|&i| state::crew_at(i).is_some()).count();
    if count == 0 {
        return;
    }
    let start = (0..count).find(|&i| state::crew_at(i).is_some_and(state::crew_is_selected));
    for n in 1..=count {
        let i = (start.map_or(count - 1, |s| s) + n) % count;
        if state::crew_at(i).is_some_and(|m| !state::crew_dead(m)) {
            tap_key(KEY_F1 + i as i32, false);
            return;
        }
    }
}

/// Arms the next powered weapon after the armed one, wrapping.
pub fn arm_next() {
    let Some(control) = state::weapon_control() else {
        return;
    };
    let slots = WEAPON_SLOTS as usize;
    let armed = control.obj(WEAPONCTRL_ARMED);
    let start = armed.and_then(|a| (0..slots).rev().find(|&i| state::armament_item(control, i) == Some(a)));
    for n in 1..=slots {
        let i = (start.map_or(slots - 1, |s| s) + n) % slots;
        if state::armament_item(control, i).is_some_and(|w| w.flag(WEAPON_POWERED)) {
            tap_key('1' as i32 + i as i32, false);
            return;
        }
    }
}

/// Steps the open ship or store screen's tab, wrapping.
pub fn tab_step(dir: i32) {
    let Some(tabbed) = state::open_tabbed() else {
        return;
    };
    let n = tabbed.at(TABBED_BUTTONS).vec_len(std::mem::size_of::<usize>()) as i32;
    if n <= 0 {
        return;
    }
    let current = tabbed.get::<u32>(TABBED_CURRENT) as i32;
    // SAFETY: SetTab on the open TabbedWindow with an index below its tab count.
    unsafe { (api().set_tab)(tabbed.ptr(), ((current + dir + n) % n) as u32) }
}
