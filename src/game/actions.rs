//! What the layer does to the game: keys, Shift, clicks and queued clicks, room doors,
//! crew and weapon cycling, volleys, tabs, and the fast speed.

use std::sync::atomic::{AtomicI32, Ordering};

use super::api::api;
use super::mem::Obj;
use super::offsets::*;
use super::state;
use super::zones::{ship_center, ZoneKind};
use crate::core::layer::Layer;
use crate::core::util::log;

const SPEED_LEVEL_NORMAL: i32 = 0;
const STEP_CAP_FAST: f32 = 2.0;
/// Queued clicks: capacity, frames between phases, phases per click.
const MAX_QUEUED_CLICKS: usize = 16;
const QUEUED_CLICK_FRAMES: i32 = 4;
const QUEUED_CLICK_PHASES: i32 = 3;

/// How many holders want Shift down; outside the layer, as the game queries it mid-hook.
static SHIFT_COUNT: AtomicI32 = AtomicI32::new(0);

pub fn shift_held() -> bool {
    SHIFT_COUNT.load(Ordering::Relaxed) > 0
}

pub fn shift_count() -> i32 {
    SHIFT_COUNT.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    Left,
    Right,
}

/// A volley under way: weapons armed in turn, each taking a target before the next.
#[derive(Debug, Default, PartialEq)]
pub struct Volley {
    /// The slot armed now, whether the volley clicked to aim it, and whether it took a target.
    armed: usize,
    clicked: bool,
    took: bool,
    /// The slots still to arm, in order.
    rest: Vec<usize>,
    /// Where the first weapon aimed by a point took its target, in frame pixels.
    at: Option<(f32, f32)>,
}

/// Clicks queued for later frames, the volley, the fast speed's step cap, and its state.
#[derive(Default)]
pub struct Actions {
    queued: Vec<(f32, f32)>,
    queued_phase: i32,
    queued_wait: i32,
    pub volley: Option<Volley>,
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
    let delta = if on { 1 } else { -1 };
    let count = SHIFT_COUNT.fetch_add(delta, Ordering::Relaxed) + delta;
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

/// Toggles the game's pause, as its pause key does.
pub fn toggle_pause() {
    tap_key(KEY_SPACE, false);
}

pub fn mouse_move(wx: i32, wy: i32, dx: i32, dy: i32, left: bool, right: bool) {
    // SAFETY: the game's mouse handler on its CApp.
    unsafe { (api().mouse_move)(state::app_ptr(), wx, wy, dx, dy, left, false, right) }
}

pub fn mouse_button(button: MouseButton, down: bool, wx: i32, wy: i32) {
    let a = api();
    let f = match (button, down) {
        (MouseButton::Left, true) => a.lbutton_down,
        (MouseButton::Left, false) => a.lbutton_up,
        (MouseButton::Right, true) => a.rbutton_down,
        (MouseButton::Right, false) => a.rbutton_up,
    };
    // SAFETY: the game's mouse handler on its CApp.
    unsafe { f(state::app_ptr(), wx, wy) }
}

impl Layer {
    /// Presses or releases a mouse button at the cursor.
    pub fn click(&mut self, button: MouseButton, down: bool) {
        let (wx, wy) = self.to_window(self.cursor.x, self.cursor.y);
        mouse_button(button, down, wx, wy);
        match button {
            MouseButton::Left => self.cursor.l_held = down,
            MouseButton::Right => self.cursor.r_held = down,
        }
    }

    pub fn click_tap(&mut self, button: MouseButton) {
        self.click(button, true);
        self.click(button, false);
    }

    /// Moves the cursor to the player's or the enemy's ship centre, whichever is farther.
    pub fn center_swap(&mut self) {
        let Some(gui) = state::gui() else {
            return;
        };
        let p = state::player_ship().and_then(|s| ship_center(s, state::player_origin(gui)));
        let e = state::enemy_ship().and_then(|s| ship_center(s, state::enemy_origin(gui)));
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
        let kind = if member.i32(CREW_SHIP) == PLAYER_SHIP_ID { ZoneKind::Room } else { ZoneKind::EnemyRoom };
        let Some(t) = self.zones.list.iter().find(|t| t.id == room && t.kind == kind).copied() else {
            return;
        };
        (self.cursor.x, self.cursor.y) = t.rect.center();
        self.send_cursor();
        self.mark_focus();
        if self.debug {
            log!("cursor to crew room {room}");
        }
    }

    /// Steps the selection through the crew of the room under the cursor unless crew is
    /// selected elsewhere or `room_crew_pick` is off; returns whether it did.
    pub fn pick_room_crew(&mut self) -> bool {
        if !self.cfg.room_crew_pick {
            return false;
        }
        let Some(zone) = self.zone_under_cursor().map(|i| self.zones.list[i]) else {
            return false;
        };
        let on_player = match zone.kind {
            ZoneKind::Room => true,
            ZoneKind::EnemyRoom => false,
            _ => return false,
        };
        let crew: Vec<(usize, Obj)> = state::boxed_crew()
            .into_iter()
            .filter(|&(i, m)| {
                i < CREW_HOTKEYS
                    && !state::crew_dead(m)
                    && m.i32(CREW_ROOM) == zone.id
                    && (m.i32(CREW_SHIP) == PLAYER_SHIP_ID) == on_player
            })
            .collect();
        let in_room = |m: Obj| crew.iter().position(|&(_, c)| c == m);
        let selected: Option<Vec<usize>> =
            state::selected_crew().map_or(Some(Vec::new()), |v| v.vec_objs().map(|(_, m)| in_room(m)).collect());
        let Some(selected) = selected.filter(|_| !crew.is_empty()) else {
            return false;
        };
        let key = |p: usize| KEY_F1 + crew[p].0 as i32;
        let pick = next_room_pick(crew.len(), &selected);
        match pick {
            RoomPick::One(p) => tap_key(key(p), false),
            RoomPick::All => (0..crew.len()).for_each(|p| tap_key(key(p), p > 0)),
        }
        if self.debug {
            log!("room {} crew pick {pick:?}", zone.id);
        }
        true
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
            1 => mouse_button(MouseButton::Left, true, wx, wy),
            _ => {
                mouse_button(MouseButton::Left, false, wx, wy);
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

    /// Starts a volley: selects every powered weapon, which clears their targets, and leaves
    /// the first armed. Weapons aimed by a point come first in slot order, the beams last.
    pub fn arm_all(&mut self) {
        self.actions.volley = None;
        let Some(control) = state::weapon_control() else {
            return;
        };
        let powered: Vec<(usize, Obj)> = (0..WEAPON_SLOTS as usize)
            .filter_map(|i| Some((i, state::armament_item(control, i)?)))
            .filter(|(_, w)| w.flag(WEAPON_POWERED))
            .collect();
        let (beams, points): (Vec<_>, Vec<_>) = powered.into_iter().partition(|&(_, w)| state::weapon_is_beam(w));
        let order: Vec<usize> = points.iter().chain(&beams).map(|&(i, _)| i).collect();
        // The first is selected last, so it stays armed.
        for &slot in order.iter().rev() {
            select_weapon(control, slot);
        }
        // A weapon the game refuses to arm drops out with those before it.
        let Some(armed) = state::armed_slot(control) else {
            return;
        };
        let Some(pos) = order.iter().position(|&s| s == armed) else {
            return;
        };
        self.actions.volley = Some(Volley { armed, rest: order[pos + 1..].to_vec(), ..Default::default() });
        if self.debug {
            log!("volley: slots {:?}", &order[pos..]);
        }
    }

    /// Moves the volley on: notes when its armed weapon takes a target, and arms the next once
    /// the clicks are done and the button is up; ends it on a dropped or missed aim.
    /// Must run before the game's OnLoop of a frame with a click: a charged weapon fires there.
    pub fn run_volley(&mut self) {
        let Some(v) = &self.actions.volley else {
            return;
        };
        let Some(control) = state::weapon_control() else {
            self.end_volley("dropped");
            return;
        };
        let armed = state::armed_slot(control);
        if !v.took {
            if armed == Some(v.armed) {
                if v.clicked && !self.actions.clicks_queued() {
                    self.end_volley("missed");
                }
                return;
            }
            let weapon = state::armament_item(control, v.armed);
            if armed.is_some() || !weapon.is_some_and(state::weapon_aimed) {
                self.end_volley("dropped");
                return;
            }
            let point = weapon.is_some_and(|w| !state::weapon_is_beam(w));
            let at = v.at.or_else(|| point.then(state::mouse_point).map(|p| (p.x as f32, p.y as f32)));
            if let Some(v) = self.actions.volley.as_mut() {
                (v.took, v.at) = (true, at);
            }
        } else if armed.is_some() {
            self.end_volley("dropped");
            return;
        }
        if !self.actions.clicks_queued() && !self.cursor.l_held {
            self.arm_volley_next(control);
        }
    }

    /// Arms the volley's next weapon the game lets arm; one aimed by a point gets a click
    /// where the first took its target.
    fn arm_volley_next(&mut self, control: Obj) {
        while let Some(v) = self.actions.volley.as_mut() {
            if v.rest.is_empty() {
                self.end_volley("done");
                return;
            }
            let slot = v.rest.remove(0);
            if !select_weapon(control, slot) {
                continue;
            }
            let beam = state::armament_item(control, slot).is_some_and(state::weapon_is_beam);
            let click = v.at.filter(|_| !beam);
            (v.armed, v.clicked, v.took) = (slot, click.is_some(), false);
            if let Some((x, y)) = click {
                self.click_at(x, y);
            }
            return;
        }
    }

    fn end_volley(&mut self, why: &str) {
        self.actions.volley = None;
        if self.debug {
            log!("volley {why}");
        }
    }

    /// Toggles the doors of the player room under the cursor by clicking them: closes the
    /// open ones while any is open, else opens all.
    pub fn room_doors(&mut self) {
        let Some(under) = self.zone_under_cursor().map(|i| self.zones.list[i]) else {
            return;
        };
        if under.kind != ZoneKind::Room {
            return;
        }
        let room = under.id;
        let (Some(gui), Some(ship)) = (state::gui(), state::player_ship()) else {
            return;
        };
        let origin = state::player_origin(gui);
        let doors: Vec<_> = [ship.at(SHIP_DOORS), ship.at(SHIP_AIRLOCKS)]
            .into_iter()
            .flat_map(|list| list.vec_objs().map(|(_, door)| door))
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

/// A step of the crew selection in one room.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RoomPick {
    /// The room's crew member at this position, alone.
    One(usize),
    All,
}

/// The step after `selected`, positions among a room's `count` crew: each member alone in
/// turn, then all of them, then the first again.
fn next_room_pick(count: usize, selected: &[usize]) -> RoomPick {
    match selected {
        [] => RoomPick::One(0),
        [p] if p + 1 < count => RoomPick::One(p + 1),
        [_] if count == 1 => RoomPick::One(0),
        s if s.len() == count => RoomPick::One(0),
        _ => RoomPick::All,
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
        let i = (start.unwrap_or(count - 1) + n) % count;
        if state::crew_at(i).is_some_and(|m| !state::crew_dead(m)) {
            tap_key(KEY_F1 + i as i32, false);
            return;
        }
    }
}

/// Arms the first powered weapon without a target, or the first powered one when all hold
/// one; with a weapon armed, the next powered one after it, wrapping.
pub fn arm_next() {
    let Some(control) = state::weapon_control() else {
        return;
    };
    let slots = WEAPON_SLOTS as usize;
    let powered = |i: usize| state::armament_item(control, i).filter(|w| w.flag(WEAPON_POWERED));
    let pick = match state::armed_slot(control) {
        Some(armed) => (1..=slots).map(|n| (armed + n) % slots).find(|&i| powered(i).is_some()),
        None => (0..slots)
            .find(|&i| powered(i).is_some_and(|w| !state::weapon_aimed(w)))
            .or_else(|| (0..slots).find(|&i| powered(i).is_some())),
    };
    if let Some(i) = pick {
        tap_key(KEY_FIRST_WEAPON + i as i32, false);
    }
}

/// Selects the weapon in `slot`, which clears its target; returns whether it is armed after.
fn select_weapon(control: Obj, slot: usize) -> bool {
    tap_key(KEY_FIRST_WEAPON + slot as i32, false);
    state::armed_slot(control) == Some(slot)
}

/// Steps the open ship or store screen's tab, wrapping.
pub fn tab_step(dir: i32) {
    let Some(tabbed) = state::open_tabbed() else {
        return;
    };
    let n = tabbed.at(TABBED_BUTTONS).ptr_count() as i32;
    if n <= 0 {
        return;
    }
    let current = tabbed.get::<u32>(TABBED_CURRENT) as i32;
    // SAFETY: SetTab on the open TabbedWindow with an index below its tab count.
    unsafe { (api().set_tab)(tabbed.ptr(), ((current + dir + n) % n) as u32) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::util::now_s;
    use crate::game::api::{Point, Rect};
    use crate::game::fake::{calls, with_game, Call, Game};
    use crate::game::zones::Zone;
    use std::mem::size_of;

    const ORIGIN: Point = Point { x: 100, y: 50 };
    const ROOM: i32 = 4;

    fn at(p: Point) -> (f32, f32) {
        ((ORIGIN.x + p.x) as f32, (ORIGIN.y + p.y) as f32)
    }

    /// A weapon box per slot, None for an empty one: whether its weapon is powered, a beam,
    /// and aimed. Returns the weapon control.
    fn weapons(g: &mut Game, slots: &[Option<(bool, bool, bool)>]) -> Obj {
        let mut boxes = Vec::new();
        for &slot in slots {
            let Some((powered, beam, aimed)) = slot else {
                boxes.push(None);
                continue;
            };
            let bp = g.arena.alloc(WEAPONBP_TYPE_NAME + size_of::<usize>());
            g.string(bp, WEAPONBP_TYPE_NAME, if beam { BEAM_TYPE_NAME } else { "LASER" });
            let weapon = g.arena.alloc(WEAPON_TARGETS + 3 * size_of::<usize>());
            weapon.set(WEAPON_BLUEPRINT, bp.addr());
            weapon.set(WEAPON_POWERED, powered as u8);
            if aimed {
                g.arena.vector(weapon, WEAPON_TARGETS, &[[0.0f32; 2]]);
            }
            let armament_box = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
            armament_box.set(ARMAMENTBOX_ITEM, weapon.addr());
            boxes.push(Some(armament_box));
        }
        let control = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
        g.arena.ptr_vector(control, ARMAMENT_BOXES, &boxes);
        control
    }

    /// The weapon in `slot` takes a target, its aim done.
    fn aim(g: &mut Game, control: Obj, slot: usize) {
        control.set(WEAPONCTRL_ARMED, 0usize);
        let weapon = state::armament_item(control, slot).expect("a weapon in the slot");
        g.arena.vector(weapon, WEAPON_TARGETS, &[[0.0f32; 2]]);
    }

    fn key(slot: usize) -> i32 {
        KEY_FIRST_WEAPON + slot as i32
    }

    fn keys_down() -> Vec<i32> {
        calls().into_iter().filter_map(|c| if let Call::KeyDown(k) = c { Some(k) } else { None }).collect()
    }

    #[test]
    fn the_next_weapon_is_the_first_without_a_target_or_the_one_after_the_armed() {
        with_game(|g| {
            let (aimed, unpowered, free) =
                (Some((true, false, true)), Some((false, false, false)), Some((true, false, false)));
            let control = weapons(g, &[aimed, unpowered, free, aimed]);
            arm_next();
            assert_eq!(keys_down(), [key(2)]);
            arm_next();
            assert_eq!(keys_down(), [key(3)], "the next after the armed one");
            aim(g, control, 2);
            arm_next();
            assert_eq!(keys_down(), [key(0)], "all hold a target");
        });
    }

    #[test]
    fn a_volley_aims_every_weapon_where_the_first_aimed_and_the_beams_last() {
        with_game(|g| {
            let (beam, laser, unpowered) =
                (Some((true, true, false)), Some((true, false, true)), Some((false, false, false)));
            let control = weapons(g, &[beam, laser, unpowered, laser]);
            let mut l = Layer::default();
            l.arm_all();
            assert_eq!(keys_down(), [key(0), key(3), key(1)], "the first selected last");
            assert_eq!(l.actions.volley, Some(Volley { armed: 1, rest: vec![3, 0], ..Default::default() }));
            l.run_volley();
            assert_eq!(calls(), [], "the first waits for the player");
            let target = Point { x: 300, y: 200 };
            g.mouse.set(0, target);
            aim(g, control, 1);
            l.run_volley();
            assert_eq!(keys_down(), [key(3)]);
            assert_eq!(l.actions.queued, [(target.x as f32, target.y as f32)]);
            l.run_volley();
            assert_eq!(calls(), [], "waits for the click");
            l.actions.queued.clear();
            aim(g, control, 3);
            l.run_volley();
            assert_eq!(keys_down(), [key(0)]);
            assert!(l.actions.queued.is_empty(), "a beam waits for its line");
            aim(g, control, 0);
            l.run_volley();
            assert_eq!(l.actions.volley, None);
        });
    }

    #[test]
    fn a_volley_moves_on_at_the_release_though_the_weapon_fired_before() {
        with_game(|g| {
            let laser = Some((true, false, false));
            let control = weapons(g, &[laser, laser]);
            let mut l = Layer::default();
            l.arm_all();
            calls();
            l.cursor.l_held = true;
            aim(g, control, 0);
            l.run_volley();
            assert_eq!(calls(), [], "waits for the release");
            let fired = state::armament_item(control, 0).expect("a weapon in slot 0");
            g.arena.vector::<[f32; 2]>(fired, WEAPON_TARGETS, &[]);
            l.cursor.l_held = false;
            l.run_volley();
            assert_eq!(keys_down(), [key(1)]);
            assert_eq!(l.actions.queued.len(), 1);
        });
    }

    #[test]
    fn a_volley_ends_on_a_disarm_another_weapon_or_a_missed_click() {
        with_game(|g| {
            let laser = Some((true, false, false));
            let control = weapons(g, &[laser, laser, laser]);
            let mut l = Layer::default();
            l.arm_all();
            control.set(WEAPONCTRL_ARMED, 0usize);
            l.run_volley();
            assert_eq!(l.actions.volley, None, "disarmed without a target");
            l.arm_all();
            tap_key(key(2), false);
            l.run_volley();
            assert_eq!(l.actions.volley, None, "another weapon armed");
            l.arm_all();
            aim(g, control, 0);
            l.run_volley();
            l.actions.queued.clear();
            l.run_volley();
            assert_eq!(l.actions.volley, None, "the click took no target");
            assert_eq!(state::armed_slot(control), Some(1), "the weapon stays armed");
        });
    }

    #[test]
    fn room_doors_close_the_open_doors_of_the_room_then_open_them_all() {
        with_game(|g| {
            g.gui.set(GUI_SHIP_POSITION, ORIGIN);
            let ship = g.player_manager().at(SHIPMANAGER_SHIP);
            let (p_open, p_shut, p_other, p_lock) =
                (Point { x: 10, y: 20 }, Point { x: 30, y: 40 }, Point { x: 50, y: 60 }, Point { x: 70, y: 80 });
            let open = g.door((ROOM, 5), p_open, true);
            let shut = g.door((6, ROOM), p_shut, false);
            let other = g.door((1, 2), p_other, true);
            let airlock = g.door((ROOM, -1), p_lock, true);
            g.arena.ptr_vector(ship, SHIP_DOORS, &[Some(open), None, Some(shut), Some(other)]);
            g.arena.ptr_vector(ship, SHIP_AIRLOCKS, &[Some(airlock)]);
            let mut l = Layer::default();
            l.zones.list = vec![Zone { rect: Rect { x: 0, y: 0, w: 100, h: 100 }, id: ROOM, kind: ZoneKind::Room }];
            (l.cursor.last_x, l.cursor.last_y) = (50, 50);
            l.room_doors();
            assert_eq!(l.actions.queued, [at(p_open), at(p_lock)]);
            for door in [open, airlock] {
                door.set(DOOR_OPEN, 0u8);
            }
            l.actions.queued.clear();
            l.room_doors();
            assert_eq!(l.actions.queued, [at(p_open), at(p_shut), at(p_lock)]);
        });
    }

    #[test]
    fn room_doors_skip_a_room_of_the_enemy() {
        with_game(|g| {
            let ship = g.player_manager().at(SHIPMANAGER_SHIP);
            let door = g.door((ROOM, 5), Point { x: 10, y: 20 }, true);
            g.arena.ptr_vector(ship, SHIP_DOORS, &[Some(door)]);
            let mut l = Layer::default();
            l.zones.list =
                vec![Zone { rect: Rect { x: 0, y: 0, w: 100, h: 100 }, id: ROOM, kind: ZoneKind::EnemyRoom }];
            (l.cursor.last_x, l.cursor.last_y) = (50, 50);
            l.room_doors();
            assert!(l.actions.queued.is_empty());
        });
    }

    #[test]
    fn a_room_pick_takes_each_member_alone_then_all() {
        use RoomPick::*;
        assert_eq!(next_room_pick(1, &[]), One(0));
        assert_eq!(next_room_pick(1, &[0]), One(0));
        assert_eq!(next_room_pick(2, &[0]), One(1));
        assert_eq!(next_room_pick(2, &[1]), All);
        assert_eq!(next_room_pick(2, &[0, 1]), One(0));
        assert_eq!(next_room_pick(3, &[2, 0]), All, "a part of the room");
    }

    /// A living player crew member in `room` and the crew box holding it.
    fn boxed_member(g: &mut Game, room: i32) -> (Obj, Obj) {
        let member = g.crew("Crew", "human", PLAYER_SHIP_ID, false);
        member.set(CREW_ROOM, room);
        let crew_box = g.arena.alloc(CREWBOX_CREW + size_of::<usize>());
        crew_box.set(CREWBOX_CREW, member.addr());
        (member, crew_box)
    }

    fn select(g: &mut Game, crew: &[Obj]) {
        let items: Vec<_> = crew.iter().copied().map(Some).collect();
        g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &items);
    }

    #[test]
    fn the_cursor_follows_selected_crew_to_its_room_and_frames_it() {
        with_game(|g| {
            let (member, _) = boxed_member(g, ROOM);
            select(g, &[member]);
            let mut l = Layer::default();
            l.zones.list = vec![Zone { rect: Rect { x: 0, y: 0, w: 100, h: 100 }, id: ROOM, kind: ZoneKind::Room }];
            l.cursor_to_selected_crew();
            assert_eq!((l.cursor.last_x, l.cursor.last_y), (50, 50));
            assert_eq!(l.focused_zone(now_s()).map(|(i, _)| i), Some(0));
        });
    }

    #[test]
    fn a_room_pick_selects_by_hotkey_unless_crew_is_selected_elsewhere() {
        with_game(|g| {
            let (away, away_box) = boxed_member(g, ROOM + 1);
            let (first, first_box) = boxed_member(g, ROOM);
            let (second, second_box) = boxed_member(g, ROOM);
            g.arena.ptr_vector(
                g.gui,
                GUI_CREW_CONTROL + CREW_BOXES,
                &[Some(away_box), Some(first_box), Some(second_box)],
            );
            let mut l = Layer::default();
            let zone = |id: i32| Zone { rect: Rect { x: 0, y: 0, w: 100, h: 100 }, id, kind: ZoneKind::Room };
            l.zones.list = vec![zone(ROOM)];
            (l.cursor.last_x, l.cursor.last_y) = (50, 50);
            let (f_first, f_second) = (KEY_F1 + 1, KEY_F1 + 2);
            l.cfg.room_crew_pick = false;
            assert!(!l.pick_room_crew(), "switched off");
            l.cfg.room_crew_pick = true;
            assert!(l.pick_room_crew());
            assert_eq!(calls(), [Call::KeyDown(f_first), Call::KeyUp(f_first)]);
            select(g, &[first]);
            assert!(l.pick_room_crew());
            assert_eq!(calls(), [Call::KeyDown(f_second), Call::KeyUp(f_second)]);
            select(g, &[second]);
            assert!(l.pick_room_crew());
            #[rustfmt::skip]
            let all = [
                Call::KeyDown(f_first), Call::KeyUp(f_first),
                Call::KeyDown(KEY_LSHIFT), Call::KeyDown(f_second), Call::KeyUp(f_second), Call::KeyUp(KEY_LSHIFT),
            ];
            assert_eq!(calls(), all);
            select(g, &[away]);
            assert!(!l.pick_room_crew(), "the selection is ordered here instead");
            select(g, &[]);
            l.zones.list = vec![zone(ROOM + 2)];
            assert!(!l.pick_room_crew(), "no crew in the room");
            assert_eq!(calls(), []);
        });
    }
}
