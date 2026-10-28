//! The HUD's edge panels moved toward the frame's middle on a screen narrower than the frame,
//! as far as the player ship leaves room, so the ship view can keep to the HUD's columns.
//!
//! The left shift moves the status panel, the crew boxes and the systems, whose weapon and
//! drone bars the game places after them; the right shift moves the enemy window and the
//! subsystems. Each is the narrowest free run in the panels' way, measured on the HUD as the
//! game lays it out on its own, less CLEARANCE: the top row and the hull on the left, the
//! hull and the top buttons on the right, the bottom row for both. The flagship's window
//! stays put.
//!
//! Panels draw and hit-test from position fields the layer writes, each the game's own value
//! plus the shift; a field the game sets anew, as for a new ship, gives the new base. The crew
//! boxes take their position when built and the status panel hit-tests fixed spots, so two
//! hooks add the left shift there, and the left panels stay put without them.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::api::api;
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;
use crate::ui::zoom::{self, Span};

/// Frame pixels kept free between a moved panel and what it moves toward.
const CLEARANCE: i32 = 6;
/// The clear columns left and right of each player hull image, by image width: no pixel in
/// them reaches the alpha the hull's glow stays below (tools/hull_margins.py).
const HULL_MARGINS: [(i32, i32, i32); 10] = [
    (440, 28, 26),
    (550, 26, 26),
    (594, 26, 28),
    (595, 21, 27),
    (600, 26, 26),
    (630, 26, 28),
    (650, 25, 28),
    (652, 27, 28),
    (710, 27, 26),
    (721, 25, 26),
];

/// The left shift the crew boxes are built with and the status panel hit-tests with.
static LEFT_SHIFT: AtomicI32 = AtomicI32::new(0);
static LEFT_HOOKED: AtomicBool = AtomicBool::new(false);

/// Records whether the crew box and status panel hooks are in; the left shift needs both.
pub fn set_left_hooked(on: bool) {
    LEFT_HOOKED.store(on, Ordering::Relaxed);
}

/// The left shift as the panels have it.
pub fn left_shift() -> i32 {
    LEFT_SHIFT.load(Ordering::Relaxed)
}

/// Free frame columns in each shift's way, on the HUD as the game lays it out on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Room {
    /// The status panel's top row to the top buttons.
    top_row: i32,
    /// The left column to the hull.
    left_column: i32,
    /// The hull to the enemy window.
    hull: i32,
    /// The top buttons to the enemy window.
    buttons: i32,
    /// The weapon and drone bars to the subsystems.
    bottom_row: i32,
    /// The flagship's window, which stays put.
    boss: bool,
}

/// How far the panels move toward the frame's middle, in frame pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shifts {
    /// The status panel, the crew boxes and the systems, rightward.
    pub left: i32,
    /// The enemy window, leftward.
    pub window: i32,
    /// The subsystems, leftward.
    pub sub: i32,
}

impl Shifts {
    /// The shifts `room` allows; the subsystems take what the left shift leaves of the
    /// bottom row.
    fn fitting(room: Room) -> Shifts {
        let free = |gap: i32| (gap - CLEARANCE).max(0);
        let left = free(room.top_row.min(room.left_column).min(room.bottom_row));
        let right = free(room.hull.min(room.buttons));
        let sub = right.min(free(room.bottom_row) - left).max(0);
        Shifts { left, window: if room.boss { 0 } else { right }, sub }
    }

    /// The frame columns the moved HUD keeps to: each side's edge moves in with its panels,
    /// the right one only as far as both right panels.
    pub fn span(self) -> Span {
        Span { left: self.left as f32, right: (GAME_W - self.window.min(self.sub)) as f32 }
    }
}

/// A position the layer moves: the game's own value and the value the layer last wrote.
#[derive(Clone, Copy, Debug, Default)]
struct Moved {
    base: i32,
    written: Option<i32>,
}

impl Moved {
    /// The game's own value behind `current`: a value other than the last written is new.
    fn base(&self, current: i32) -> i32 {
        if self.written == Some(current) {
            self.base
        } else {
            current
        }
    }

    /// The value to write for `shift` over `current`; None when it is already there.
    fn next(&mut self, current: i32, shift: i32) -> Option<i32> {
        self.base = self.base(current);
        let target = self.base + shift;
        self.written = Some(target);
        (target != current).then_some(target)
    }
}

/// The HUD panels the layer moves.
#[derive(Default)]
pub struct Hud {
    status: Moved,
    window: Moved,
    systems: Moved,
    /// The subsystems' position, relative to the systems'.
    sub: Moved,
    shifts: Shifts,
}

/// The hull's solid columns for an image `w` wide whose left column is at `left`: from the
/// first to past the last; the image's whole width for an image of no known width.
fn solid_columns(left: i32, w: i32) -> (i32, i32) {
    let (clear_left, clear_right) = HULL_MARGINS.iter().find(|m| m.0 == w).map_or((0, 0), |m| (m.1, m.2));
    (left + clear_left, left + w - clear_right)
}

/// The player hull's solid columns in the frame as it stands facing an enemy: where it is
/// while one is in sight, else where an ordinary enemy puts it.
fn hull_columns(gui: Obj, ship: Obj) -> Option<(i32, i32)> {
    // SAFETY: ShipGraph::GetShipInfo with the ship's own id.
    let graph = unsafe { Obj::from_ptr((api().ship_info)(ship.i32(SHIP_ID))) }?;
    let x = match state::enemy_ship() {
        Some(_) => state::player_origin(gui).x,
        None => COMBAT_SHIP_X + ship.i32(SHIP_HORIZONTAL_SHIFT),
    };
    let image = ship.at(SHIP_IMAGE);
    let left = x + graph.i32(SHIPGRAPH_BOX) + image.i32(IMAGE_X);
    Some(solid_columns(left, image.i32(IMAGE_W)))
}

/// Past the right end of the weapon and drone bars' boxes; 0 without any.
fn bars_right() -> i32 {
    [state::weapon_control(), state::drone_control()]
        .into_iter()
        .flatten()
        .flat_map(|bar| {
            let at = bar.point(ARMAMENT_LOCATION).x;
            bar.at(ARMAMENT_BOXES).vec_objs().map(move |(_, b)| at + b.point(ARMAMENTBOX_HIT).x + ARMAMENTBOX_HIT_W)
        })
        .max()
        .unwrap_or(0)
}

impl Hud {
    /// The free columns around the HUD with the moved panels put back; None without a hull.
    fn room(&self, gui: Obj, ship: Obj) -> Option<Room> {
        let (hull_left, hull_right) = hull_columns(gui, ship)?;
        let combat = gui.at(GUI_COMBAT_CONTROL);
        let window_left = self.window.base(combat.i32(COMBAT_POSITION)) + HOSTILE_BOX_X + HOSTILE_FRAME_INSET;
        let systems = gui.at(GUI_SYS_CONTROL);
        let sub_left =
            self.systems.base(systems.i32(SYSCTRL_POSITION)) + self.sub.base(systems.i32(SYSCTRL_SUB_POSITION));
        // The bars follow the systems, by the left shift last written.
        let bars_right = bars_right() - self.shifts.left;
        // The left panels stay put without the hooks that move the crew boxes and status panel.
        let top_row = if LEFT_HOOKED.load(Ordering::Relaxed) { TOP_BUTTONS_LEFT - STATUS_TOP_ROW_RIGHT } else { 0 };
        Some(Room {
            top_row,
            left_column: hull_left - LEFT_COLUMN_RIGHT,
            hull: window_left - hull_right,
            buttons: window_left - TOP_BUTTONS_RIGHT,
            bottom_row: sub_left - bars_right,
            boss: combat.flag(COMBAT_BOSS),
        })
    }

    /// Writes the panels' positions for `shifts` and has the game rebuild what it built from
    /// the old ones.
    fn apply(&mut self, gui: Obj, shifts: Shifts) {
        let status = gui.at(GUI_SHIP_STATUS);
        if let Some(x) = self.status.next(status.i32(STATUS_LOCATION), shifts.left) {
            status.set(STATUS_LOCATION, x);
        }
        let combat = gui.at(GUI_COMBAT_CONTROL);
        if let Some(x) = self.window.next(combat.i32(COMBAT_POSITION), -shifts.window) {
            combat.set(COMBAT_POSITION, x);
        }
        let systems = gui.at(GUI_SYS_CONTROL);
        let mut rebuild = false;
        if let Some(x) = self.systems.next(systems.i32(SYSCTRL_POSITION), shifts.left) {
            systems.set(SYSCTRL_POSITION, x);
            rebuild = true;
        }
        if let Some(x) = self.sub.next(systems.i32(SYSCTRL_SUB_POSITION), -shifts.left - shifts.sub) {
            systems.set(SYSCTRL_SUB_POSITION, x);
            rebuild = true;
        }
        if rebuild {
            // SAFETY: SystemControl::UpdateSubSystemBox on the GUI's own system control.
            unsafe { (api().update_sub_box)(systems.ptr()) };
        }
        if LEFT_SHIFT.swap(shifts.left, Ordering::Relaxed) != shifts.left {
            let crew = gui.at(GUI_CREW_CONTROL);
            // SAFETY: CrewControl::ClearCrewBoxes on the GUI's own crew control; its next
            // OnLoop builds them again.
            unsafe { (api().clear_crew_boxes)(crew.ptr()) };
            crew.set(CREW_STATIONS_LAST_Y, i32::MIN);
        }
        if shifts != self.shifts {
            log!("hud shift: left {}, window {}, subsystems {}", shifts.left, shifts.window, shifts.sub);
            self.shifts = shifts;
        }
    }
}

impl Layer {
    /// Moves the HUD panels as far as the player ship leaves room while the screen is
    /// narrower than the frame and `hud_shift` is on, else back; the ship view keeps to the
    /// columns the HUD covers.
    pub fn update_hud(&mut self) {
        let in_game = (!state::main_menu_open()).then(|| state::gui().zip(state::player_ship())).flatten();
        let Some((gui, ship)) = in_game else {
            self.zoom.span = Span::FRAME;
            return;
        };
        let on = self.cfg.hud_shift && zoom::narrower_than_frame();
        let shifts =
            if on { self.hud.room(gui, ship).map_or_else(Shifts::default, Shifts::fitting) } else { Shifts::default() };
        self.hud.apply(gui, shifts);
        self.zoom.span = shifts.span();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{self, with_game, Call};

    /// The free columns around the Kestrel's HUD as the game lays it out.
    const KESTREL: Room = Room { top_row: 26, left_column: 70, hull: 117, buttons: 132, bottom_row: 358, boss: false };

    #[test]
    fn the_left_shift_keeps_to_the_top_row_and_the_right_to_the_hull() {
        let s = Shifts::fitting(KESTREL);
        assert_eq!(s, Shifts { left: 20, window: 111, sub: 111 });
        assert_eq!(s.span(), Span { left: 20.0, right: 1169.0 });
        // A narrow hull: the top buttons stop the window.
        assert_eq!(Shifts::fitting(Room { hull: 235, ..KESTREL }).window, 126);
        // A hull in the window's way: nothing moves right.
        assert_eq!(Shifts::fitting(Room { hull: -20, ..KESTREL }), Shifts { left: 20, window: 0, sub: 0 });
    }

    #[test]
    fn the_flagship_window_stays_and_the_view_keeps_its_columns() {
        let s = Shifts::fitting(Room { boss: true, ..KESTREL });
        assert_eq!(s, Shifts { left: 20, window: 0, sub: 111 });
        assert_eq!(s.span().right, GAME_W as f32);
    }

    #[test]
    fn a_full_bottom_row_holds_the_subsystems_back_then_the_left_panels() {
        let s = Shifts::fitting(Room { bottom_row: 100, ..KESTREL });
        assert_eq!(s, Shifts { left: 20, window: 111, sub: 74 });
        assert_eq!(s.span().right, (GAME_W - 74) as f32);
        assert_eq!(Shifts::fitting(Room { bottom_row: 10, ..KESTREL }), Shifts { left: 4, window: 111, sub: 0 });
    }

    #[test]
    fn a_field_the_game_sets_anew_is_the_new_base() {
        let mut m = Moved::default();
        assert_eq!(m.next(747, -111), Some(636));
        assert_eq!(m.next(636, -111), None);
        assert_eq!(m.base(636), 747);
        // The game placed the window again.
        assert_eq!(m.next(747, -100), Some(647));
        assert_eq!(m.next(647, 0), Some(747));
        assert_eq!(m.next(747, 0), None);
    }

    #[test]
    fn a_known_hull_image_loses_its_clear_columns() {
        assert_eq!(solid_columns(141, 652), (168, 765));
        assert_eq!(solid_columns(100, 500), (100, 600));
    }

    /// The fake Kestrel between fights, its ship off its place in a fight: its HUD where the
    /// game puts it and its hull image; the weapon bar one box wide.
    fn kestrel(g: &mut fake::Game) {
        let manager = g.player_manager();
        let ship = manager.at(SHIPMANAGER_SHIP);
        ship.at(SHIP_IMAGE).set(IMAGE_W, 652);
        ship.at(SHIP_IMAGE).set(IMAGE_X, -59);
        g.gui.set(GUI_SHIP_POSITION, COMBAT_SHIP_X + 150);
        let combat = g.gui.at(GUI_COMBAT_CONTROL);
        combat.set(COMBAT_POSITION, 747);
        let systems = g.gui.at(GUI_SYS_CONTROL);
        systems.set(SYSCTRL_SUB_POSITION, 1015);
        let bar = combat.at(COMBAT_WEAPON_CONTROL);
        bar.set(ARMAMENT_LOCATION, 234);
        let slot = g.arena.alloc(ARMAMENTBOX_HIT + 8);
        slot.set(ARMAMENTBOX_HIT, 328);
        g.arena.ptr_vector(bar, ARMAMENT_BOXES, &[Some(slot)]);
        fake::set_ship_graph(g.arena.alloc(SHIPGRAPH_BOX + 4));
    }

    #[test]
    fn the_panels_move_and_come_back_when_turned_off() {
        with_game(|g| {
            kestrel(g);
            set_left_hooked(true);
            let mut l = Layer {
                cfg: crate::config::settings::Settings { hud_shift: true, ..Default::default() },
                ..Default::default()
            };
            l.update_hud();
            let (gui, combat, systems) = (g.gui, g.gui.at(GUI_COMBAT_CONTROL), g.gui.at(GUI_SYS_CONTROL));
            assert_eq!(gui.i32(GUI_SHIP_STATUS + STATUS_LOCATION), 20);
            assert_eq!(combat.i32(COMBAT_POSITION), 747 - 111);
            assert_eq!(systems.i32(SYSCTRL_POSITION), 20);
            assert_eq!(systems.i32(SYSCTRL_SUB_POSITION), 1015 - 20 - 111);
            assert_eq!(left_shift(), 20);
            let calls = fake::calls();
            assert!(calls.contains(&Call::UpdateSubBox(systems.addr())));
            assert!(calls.contains(&Call::ClearCrewBoxes(gui.at(GUI_CREW_CONTROL).addr())));
            assert_eq!(l.zoom.span, Span { left: 20.0, right: 1169.0 });
            // A frame later nothing is rebuilt.
            l.update_hud();
            assert!(fake::calls().is_empty());
            l.cfg.hud_shift = false;
            l.update_hud();
            assert_eq!(
                (gui.i32(STATUS_LOCATION), combat.i32(COMBAT_POSITION), systems.i32(SYSCTRL_SUB_POSITION)),
                (0, 747, 1015)
            );
            assert_eq!((left_shift(), l.zoom.span), (0, Span::FRAME));
        });
    }
}
