//! Beam aiming from the pad. Once a pad click places a beam's start on a room, the D-pad
//! and the sticks turn the beam about that room's centre (left, right) and slide its start
//! along its own line up or down the screen (a level beam's forward on up) in place of
//! stepping between zones; A fires it and B takes the start back.
//!
//! The game keeps the start in CombatControl::aimingPoints, in the enemy ship's coordinates,
//! and draws and fires the beam from it towards the mouse at the weapon's length. A slide
//! rewrites that start; every step moves the cursor onto the beam's end. The start stays
//! inside a room and the beam over the chosen room, so the line is one a mouse could aim; a
//! slide that would break that stops at the farthest point that keeps it. A new start begins
//! on the line over the room's centre through the most rooms.

use std::f32::consts::TAU;

use super::api::Rect;
use super::mem::Obj;
use super::offsets::*;
use super::state;
use crate::core::layer::Layer;
use crate::core::util::now_s;

/// A full turn of the beam in this many steps.
const TURN_STEPS: i32 = 24;
/// A slide moves the start by this share of a room tile.
const SLIDE_STEP: f32 = ROOM_TILE as f32 / 4.0;
/// The finest a slide stops short, or a turn pulls the start in, to keep the line fitting.
const FIT_STEP: f32 = 1.0;
/// A direction component this small is float error off an axis.
const AXIS_EPS: f32 = 1e-6;
/// A held direction repeats its step after REPEAT_DELAY_S, then every REPEAT_S.
const REPEAT_DELAY_S: f64 = 0.3;
const REPEAT_S: f64 = 0.08;

/// One step of the beam, by its sign: a turn (clockwise on screen when positive) or a slide
/// (the start moving down the screen when positive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Turn(i32),
    Slide(i32),
}

impl Step {
    /// The step a direction takes: sideways turns, up and down slide.
    fn of(dx: f32, dy: f32) -> Step {
        if dx.abs() > dy.abs() {
            Step::Turn(dx.signum() as i32)
        } else {
            Step::Slide(dy.signum() as i32)
        }
    }
}

/// A beam line being set: its weapon, the chosen room and the centre it turns about, its turn,
/// how far its start sits behind the centre (ahead when negative), and its length.
#[derive(Clone, Copy, Debug)]
pub struct BeamLine {
    weapon: usize,
    room: Rect,
    center: (f32, f32),
    turn: i32,
    shift: f32,
    length: f32,
    /// The step a held direction repeats, and when it next does.
    repeat: Option<(Step, f64)>,
}

/// The unit vector of a turn, exact on the axes so a beam along a room's edge stays on it.
fn direction(turn: i32) -> (f32, f32) {
    let a = turn as f32 * TAU / TURN_STEPS as f32;
    let snap = |v: f32| if v.abs() < AXIS_EPS { 0.0 } else { v };
    (snap(a.cos()), snap(a.sin()))
}

/// The start `shift` behind `center` on the line of `turn`.
fn start_of(center: (f32, f32), turn: i32, shift: f32) -> (f32, f32) {
    let (ux, uy) = direction(turn);
    (center.0 - ux * shift, center.1 - uy * shift)
}

fn inside(rooms: &[Rect], (x, y): (f32, f32)) -> bool {
    rooms.iter().any(|r| x >= r.x as f32 && x < (r.x + r.w) as f32 && y >= r.y as f32 && y < (r.y + r.h) as f32)
}

/// How many rooms the segment of `length` from `s` along `u` crosses, and its length inside
/// them; one running along a room's edge leaves it out, as the game does.
fn crossing(rooms: &[Rect], s: (f32, f32), u: (f32, f32), length: f32) -> (usize, f32) {
    let (dx, dy) = (u.0 * length, u.1 * length);
    let mut count = 0;
    let mut within = 0.0;
    for r in rooms {
        let (x0, y0, x1, y1) = (r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32);
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        let mut clear = true;
        for (p, q) in [(-dx, s.0 - x0), (dx, x1 - s.0), (-dy, s.1 - y0), (dy, y1 - s.1)] {
            if p == 0.0 {
                clear &= q > 0.0;
            } else if p < 0.0 {
                t0 = t0.max(q / p);
            } else {
                t1 = t1.min(q / p);
            }
        }
        if clear && t1 > t0 {
            count += 1;
            within += (t1 - t0) * length;
        }
    }
    (count, within)
}

/// The line a beam may take: its start inside a room and the beam over the chosen room.
#[derive(Clone, Copy)]
struct Fit<'a> {
    rooms: &'a [Rect],
    room: Rect,
    center: (f32, f32),
    length: f32,
}

impl Fit<'_> {
    fn fits(&self, turn: i32, shift: f32) -> bool {
        let s = start_of(self.center, turn, shift);
        inside(self.rooms, s) && crossing(&[self.room], s, direction(turn), self.length).0 > 0
    }

    /// The shift a slide from `shift` by `by` steps lands on: a full step where the line
    /// fits, else the farthest fitting point short of it; None when no FIT_STEP fits.
    fn slid(&self, turn: i32, shift: f32, by: i32) -> Option<f32> {
        let mut d = SLIDE_STEP;
        while d >= FIT_STEP {
            let to = shift + by as f32 * d;
            if self.fits(turn, to) {
                return Some(to);
            }
            d -= FIT_STEP;
        }
        None
    }

    /// `shift` pulled towards the centre until the line of `turn` fits; the centre always does.
    fn pulled_in(&self, turn: i32, mut shift: f32) -> f32 {
        while shift != 0.0 && !self.fits(turn, shift) {
            shift = if shift > 0.0 { (shift - FIT_STEP).max(0.0) } else { (shift + FIT_STEP).min(0.0) };
        }
        shift
    }

    /// The turn and shift, in whole slide steps, of a line over the chosen room's centre
    /// through the most rooms, then the most length inside them; the nearer start, then the
    /// first turn, on a tie.
    fn best_line(&self) -> (i32, f32) {
        let mut best: Option<((i32, f32), (usize, f32))> = None;
        for turn in 0..TURN_STEPS {
            for k in 0..=(self.length / SLIDE_STEP) as i32 {
                let shift = k as f32 * SLIDE_STEP;
                if !self.fits(turn, shift) {
                    continue;
                }
                let score = crossing(self.rooms, start_of(self.center, turn, shift), direction(turn), self.length);
                if best.is_none_or(|(_, b)| score.0 > b.0 || (score.0 == b.0 && score.1 > b.1)) {
                    best = Some(((turn, shift), score));
                }
            }
        }
        best.map_or((0, 0.0), |(line, _)| line)
    }
}

/// The armed beam with its start placed: the aiming points' vector and the weapon.
fn placed_beam() -> Option<(Obj, Obj)> {
    let combat = state::gui()?.at(GUI_COMBAT_CONTROL);
    let weapon = combat.at(COMBAT_WEAPON_CONTROL).obj(WEAPONCTRL_ARMED).filter(|&w| state::weapon_is_beam(w))?;
    let points = combat.at(COMBAT_AIMING_POINTS);
    (points.vec_len(POINTF_SIZE) == 1).then_some((points, weapon))
}

/// The enemy ship's room rectangles in its own coordinates.
fn enemy_rooms() -> Option<Vec<Rect>> {
    Some(state::enemy_ship()?.at(SHIP_ROOMS).vec_objs().map(|(_, r)| r.rect(ROOM_RECT)).collect())
}

/// How far a ray from `p` along `u` runs before it leaves the game's frame.
fn frame_reach((x, y): (f32, f32), (ux, uy): (f32, f32)) -> f32 {
    let along = |p: f32, u: f32, hi: f32| {
        if u > 0.0 {
            (hi - p) / u
        } else if u < 0.0 {
            -p / u
        } else {
            f32::INFINITY
        }
    };
    along(x, ux, (GAME_W - 1) as f32).min(along(y, uy, (GAME_H - 1) as f32))
}

impl Layer {
    /// Takes over a beam whose start a pad click placed, putting it on the line over its room's
    /// centre through the most rooms; lets go once the beam fires or drops.
    pub fn update_beam(&mut self) {
        let Some((points, weapon)) = placed_beam() else {
            self.actions.beam = None;
            return;
        };
        if self.actions.beam.is_some_and(|b| b.weapon == weapon.addr()) {
            return;
        }
        // A start the mouse or a touch placed is left as it is.
        if !self.focus_held() {
            return;
        }
        let (Some(rooms), Some(first)) = (enemy_rooms(), points.obj(VECTOR_BEGIN)) else {
            return;
        };
        let length = weapon.obj(WEAPON_BLUEPRINT).map_or(0, |bp| bp.i32(WEAPONBP_LENGTH)) as f32;
        if length <= 0.0 {
            return;
        }
        let [cx, cy] = first.get::<[f32; 2]>(0);
        let Some(room) = rooms.iter().copied().find(|&r| inside(&[r], (cx, cy))) else {
            return;
        };
        let (turn, shift) = Fit { rooms: &rooms, room, center: (cx, cy), length }.best_line();
        self.actions.beam =
            Some(BeamLine { weapon: weapon.addr(), room, center: (cx, cy), turn, shift, length, repeat: None });
        self.place_beam(points);
    }

    /// Writes the beam's start into the game and puts the cursor on its end, short of the
    /// frame's edge.
    fn place_beam(&mut self, points: Obj) {
        let (Some(b), Some(gui), Some(first)) = (self.actions.beam, state::gui(), points.obj(VECTOR_BEGIN)) else {
            return;
        };
        let s = start_of(b.center, b.turn, b.shift);
        first.set(0, [s.0, s.1]);
        let o = state::enemy_origin(gui);
        let (ux, uy) = direction(b.turn);
        let at = (o.x as f32 + s.0, o.y as f32 + s.1);
        let reach = frame_reach(at, (ux, uy)).min(b.length);
        (self.cursor.x, self.cursor.y) = (at.0 + ux * reach, at.1 + uy * reach);
        self.send_cursor();
    }

    /// Turns or slides the beam one step, its line kept fitting: a turn pulls a start that no
    /// longer fits in towards the centre, a slide stops short or is not taken.
    fn take_step(&mut self, step: Step, points: Obj) {
        let (Some(rooms), Some(b)) = (enemy_rooms(), self.actions.beam.as_mut()) else {
            return;
        };
        let fit = Fit { rooms: &rooms, room: b.room, center: b.center, length: b.length };
        match step {
            Step::Turn(by) => {
                b.turn = (b.turn + by).rem_euclid(TURN_STEPS);
                b.shift = fit.pulled_in(b.turn, b.shift);
            }
            Step::Slide(by) => {
                // A shift moves the start against the beam; a level beam's goes back on down.
                let uy = direction(b.turn).1;
                let back = if uy == 0.0 { by } else { -by * uy.signum() as i32 };
                let Some(shift) = fit.slid(b.turn, b.shift, back) else {
                    return;
                };
                b.shift = shift;
            }
        }
        self.place_beam(points);
        self.buzz();
    }

    /// Takes a D-pad or stick step on a beam being set; false when none is.
    pub fn beam_step(&mut self, dx: f32, dy: f32) -> bool {
        let Some((points, weapon)) = placed_beam() else {
            return false;
        };
        if !self.actions.beam.is_some_and(|b| b.weapon == weapon.addr()) {
            return false;
        }
        let step = Step::of(dx, dy);
        self.take_step(step, points);
        if let Some(b) = self.actions.beam.as_mut() {
            b.repeat = Some((step, now_s() + REPEAT_DELAY_S));
        }
        true
    }

    /// Repeats the step of the direction `held` while it stays held on a beam being set.
    pub fn repeat_beam(&mut self, held: Option<(f32, f32)>) {
        let Some((step, at)) = self.actions.beam.and_then(|b| b.repeat) else {
            return;
        };
        if held.map(|(x, y)| Step::of(x, y)) != Some(step) {
            if let Some(b) = self.actions.beam.as_mut() {
                b.repeat = None;
            }
            return;
        }
        let now = now_s();
        let Some((points, _)) = placed_beam().filter(|_| now >= at) else {
            return;
        };
        self.take_step(step, points);
        if let Some(b) = self.actions.beam.as_mut() {
            b.repeat = Some((step, now + REPEAT_S));
        }
    }

    /// Takes back the start of a beam being set, the cursor on its room again; false when no
    /// beam is being set.
    pub fn take_beam_start(&mut self) -> bool {
        let Some(b) = self.actions.beam.take() else {
            return false;
        };
        let (Some((points, _)), Some(gui)) = (placed_beam(), state::gui()) else {
            return false;
        };
        points.set(VECTOR_END, points.get::<usize>(VECTOR_BEGIN));
        let o = state::enemy_origin(gui);
        (self.cursor.x, self.cursor.y) = (o.x as f32 + b.center.0, o.y as f32 + b.center.1);
        self.send_cursor();
        self.mark_focus();
        true
    }

    pub fn beam_active(&self) -> bool {
        self.actions.beam.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::api::Point;
    use crate::game::fake::{with_game, Game, WEAPON_SIZE};
    use std::mem::size_of;

    const TILE: i32 = ROOM_TILE;

    fn room(x: i32, y: i32) -> Rect {
        Rect { x: x * TILE, y: y * TILE, w: TILE, h: TILE }
    }

    fn center(r: Rect) -> (f32, f32) {
        r.center()
    }

    #[test]
    fn the_best_line_crosses_the_most_rooms_and_starts_inside_one() {
        // A row of three rooms with one hanging below the middle.
        let rooms = [room(0, 0), room(1, 0), room(2, 0), room(1, 1)];
        let fit = Fit { rooms: &rooms, room: rooms[0], center: center(rooms[0]), length: 80.0 };
        let (turn, shift) = fit.best_line();
        let s = start_of(fit.center, turn, shift);
        assert!(inside(&rooms, s));
        assert_eq!(crossing(&rooms, s, direction(turn), 80.0).0, 3, "along the row");
        assert_eq!((turn, shift), (0, 0.0), "from the centre on a tie");
    }

    #[test]
    fn a_beam_along_a_rooms_edge_leaves_it_out() {
        // A wide room over a narrow one, the beam down the narrow one's right edge.
        let rooms = [Rect { w: 2 * TILE, ..room(0, 0) }, room(0, 1)];
        let down = direction(TURN_STEPS / 4);
        assert_eq!(down, (0.0, 1.0));
        assert_eq!(crossing(&rooms, center(rooms[0]), down, 45.0).0, 1, "the wide room alone");
    }

    #[test]
    fn the_best_line_keeps_the_chosen_rooms_centre_on_the_beam() {
        // A corner room's beam could graze it to cross two more; the line stays over its centre.
        let rooms = [room(0, 0), room(1, 0), room(0, 1), room(1, 1)];
        let fit = Fit { rooms: &rooms, room: rooms[3], center: center(rooms[3]), length: 45.0 };
        let (turn, shift) = fit.best_line();
        assert!((0.0..=fit.length).contains(&shift), "the centre on the beam: {shift}");
        assert!(fit.fits(turn, shift));
    }

    #[test]
    fn a_slid_start_lets_a_short_beam_cross_both_neighbours() {
        const PIKE: f32 = 45.0;
        let rooms = [room(0, 0), room(1, 0), room(2, 0)];
        let fit = Fit { rooms: &rooms, room: rooms[1], center: center(rooms[1]), length: PIKE };
        assert_eq!(crossing(&rooms, fit.center, direction(0), PIKE).0, 2, "from the centre one neighbour");
        let (turn, shift) = fit.best_line();
        assert_eq!(crossing(&rooms, start_of(fit.center, turn, shift), direction(turn), PIKE).0, 3);
        assert!(shift != 0.0);
    }

    #[test]
    fn a_slide_forward_stops_at_the_chosen_rooms_far_edge() {
        let rooms = [room(0, 0), room(1, 0)];
        let fit = Fit { rooms: &rooms, room: rooms[0], center: center(rooms[0]), length: 45.0 };
        let once = fit.slid(0, 0.0, -1).expect("a full step fits");
        assert_eq!(once, -SLIDE_STEP);
        let edge = fit.slid(0, once, -1).expect("a step short of the edge fits");
        let s = start_of(fit.center, 0, edge);
        assert!(s.0 < TILE as f32 && s.0 >= TILE as f32 - FIT_STEP, "at the room's edge: {s:?}");
        assert_eq!(fit.slid(0, edge, -1), None, "no farther");
    }

    /// An enemy with a row of rooms, an armed beam of `length` and its start on the first
    /// room's centre, the cursor focused there; returns the aiming points' vector.
    fn placed(g: &mut Game, l: &mut Layer, length: i32) -> Obj {
        let combat = g.gui.at(GUI_COMBAT_CONTROL);
        combat.set(COMBAT_POSITION, Point { x: 800, y: 100 });
        let enemy = g.enemy_manager().at(SHIPMANAGER_SHIP);
        let rooms: Vec<_> = (0..3).map(|i| Some(g.room(i, room(i, 0)))).collect();
        g.arena.ptr_vector(enemy, SHIP_ROOMS, &rooms);
        let bp = g.arena.alloc(WEAPONBP_LENGTH + size_of::<i32>());
        g.string(bp, WEAPONBP_TYPE_NAME, BEAM_TYPE_NAME);
        bp.set(WEAPONBP_LENGTH, length);
        let weapon = g.arena.alloc(WEAPON_SIZE);
        weapon.set(WEAPON_BLUEPRINT, bp.addr());
        combat.at(COMBAT_WEAPON_CONTROL).set(WEAPONCTRL_ARMED, weapon.addr());
        let c = center(room(0, 0));
        g.arena.vector(combat, COMBAT_AIMING_POINTS, &[[c.0, c.1]]);
        (l.cursor.x, l.cursor.y) = (800.0 + c.0, 100.0 + c.1);
        l.send_cursor();
        l.mark_focus();
        combat.at(COMBAT_AIMING_POINTS)
    }

    fn start(points: Obj) -> [f32; 2] {
        points.obj(VECTOR_BEGIN).expect("one point placed").get(0)
    }

    #[test]
    fn a_pad_placed_start_slides_and_turns_and_goes_back_on_cancel() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            let c = center(room(0, 0));
            l.update_beam();
            assert!(l.beam_active());
            assert_eq!(start(points), [c.0, c.1], "the row is crossed from the centre");
            assert_eq!((l.cursor.x, l.cursor.y), (800.0 + c.0 + 80.0, 100.0 + c.1), "the cursor on the end");
            assert!(l.beam_step(0.0, -1.0));
            assert_eq!(start(points), [c.0 + SLIDE_STEP, c.1], "a step forward along the line");
            assert!(l.beam_step(0.0, 1.0) && l.beam_step(0.0, 1.0));
            assert_eq!(start(points), [c.0 - SLIDE_STEP, c.1], "and back past the centre");
            assert!(l.beam_step(1.0, 0.0));
            assert!(l.cursor.y > 100.0 + c.1, "turned clockwise, the end going down");
            assert!(l.take_beam_start());
            assert_eq!(points.vec_len(POINTF_SIZE), 0);
            assert_eq!((l.cursor.x, l.cursor.y), (800.0 + c.0, 100.0 + c.1));
            l.update_beam();
            assert!(!l.beam_active() && !l.beam_step(1.0, 0.0), "no start, no beam to set");
        });
    }

    #[test]
    fn up_and_down_move_the_start_up_and_down_the_screen() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            l.update_beam();
            for _ in 0..TURN_STEPS / 4 {
                l.beam_step(1.0, 0.0);
            }
            let y = start(points)[1];
            assert!(l.beam_step(0.0, -1.0));
            assert!(start(points)[1] < y, "up on a beam pointing down");
            for _ in 0..TURN_STEPS / 2 {
                l.beam_step(1.0, 0.0);
            }
            let y = start(points)[1];
            assert!(l.beam_step(0.0, -1.0));
            assert!(start(points)[1] < y, "up on a beam pointing up");
        });
    }

    #[test]
    fn a_start_placed_off_the_focus_is_left_alone() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            (l.cursor.x, l.cursor.y) = (l.cursor.x + 3.0, l.cursor.y);
            l.send_cursor();
            l.update_beam();
            assert!(!l.beam_active());
            let c = center(room(0, 0));
            assert_eq!(start(points), [c.0, c.1]);
        });
    }
}
