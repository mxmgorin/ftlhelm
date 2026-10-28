//! What the layer does to the game: keys, Shift, clicks and queued clicks, room doors,
//! crew and weapon cycling, tabs, and the fast speed.

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
    /// selected elsewhere; returns whether it did.
    pub fn pick_room_crew(&mut self) -> bool {
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

/// Arms the next powered weapon after the armed one, wrapping.
pub fn arm_next() {
    let Some(control) = state::weapon_control() else {
        return;
    };
    let slots = WEAPON_SLOTS as usize;
    let armed = control.obj(WEAPONCTRL_ARMED);
    let start = armed.and_then(|a| (0..slots).rev().find(|&i| state::armament_item(control, i) == Some(a)));
    for n in 1..=slots {
        let i = (start.unwrap_or(slots - 1) + n) % slots;
        if state::armament_item(control, i).is_some_and(|w| w.flag(WEAPON_POWERED)) {
            tap_key(KEY_FIRST_WEAPON + i as i32, false);
            return;
        }
    }
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
