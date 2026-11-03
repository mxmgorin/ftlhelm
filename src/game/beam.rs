//! Beam aiming from the pad. Once a pad click places a beam's start on a room, the D-pad
//! and the sticks turn the beam about a pivot (left, right) and slide its start along its
//! own line in place of stepping between zones: down towards the beam's end, where the
//! cursor is, up away from it. With A held they move the pivot, and the line with it, across
//! the screen. A fires the beam on its release unless it moved the line; B takes the start
//! back.
//!
//! The game keeps the start in CombatControl::aimingPoints, in the enemy ship's coordinates,
//! and draws and fires the beam from it towards the mouse at the weapon's length; a click
//! places a start only on a room. It fires on the button's press, so A over a line being set
//! holds its click until the release. A step rewrites that start and moves the cursor past the
//! beam's end. The pivot and the start stay inside rooms, so the line is one a mouse could
//! aim. A slide or move that would break that stops at the farthest point that keeps it, the
//! next one crossing empty space to the next room along its way, and a turn or move pulls the
//! start in towards the pivot; a step that cannot be taken rumbles longer. A new start becomes
//! the pivot, the line laid over it through the most rooms.

use std::f32::consts::TAU;

use super::api::Rect;
use super::mem::Obj;
use super::offsets::*;
use super::state;
use crate::core::layer::Layer;
use crate::core::util::now_s;
use crate::game::actions::MouseButton;

/// A full turn of the beam in this many steps.
const TURN_STEPS: i32 = 24;
/// A slide or a move shifts the start by this share of a room tile.
const SLIDE_STEP: f32 = ROOM_TILE as f32 / 4.0;
/// The finest a slide or move stops short, or the start is pulled in, to keep the line fitting.
const FIT_STEP: f32 = 1.0;
/// A direction component this small is float error off an axis.
const AXIS_EPS: f32 = 1e-6;
/// A held direction repeats its step after REPEAT_DELAY_S, then every REPEAT_S.
const REPEAT_DELAY_S: f64 = 0.3;
const REPEAT_S: f64 = 0.08;

/// One step of the beam, by its sign: a turn (clockwise on screen when positive), a slide
/// (towards the beam's end when positive) or a move of the pivot along the screen's axes
/// (right and down when positive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Turn(i32),
    Slide(i32),
    Move(i32, i32),
}

impl Step {
    /// The step a direction takes: with a click held a move along its main axis, else
    /// sideways turns, up and down slide.
    fn of(dx: f32, dy: f32, click_held: bool) -> Step {
        match (click_held, dx.abs() > dy.abs()) {
            (true, true) => Step::Move(dx.signum() as i32, 0),
            (true, false) => Step::Move(0, dy.signum() as i32),
            (false, true) => Step::Turn(dx.signum() as i32),
            (false, false) => Step::Slide(dy.signum() as i32),
        }
    }
}

/// A beam line being set: its weapon, where its start was placed, the pivot it turns about,
/// its turn, how far its start sits behind the pivot (ahead when negative), and its length.
#[derive(Clone, Copy, Debug)]
pub struct BeamLine {
    weapon: usize,
    placed: (f32, f32),
    pivot: (f32, f32),
    turn: i32,
    shift: f32,
    length: f32,
    /// The step a held direction repeats, and when it next does.
    repeat: Option<(Step, f64)>,
    /// Whether a click is held over the line, and if so whether its release fires the beam.
    press: Option<bool>,
}

impl BeamLine {
    fn fit<'a>(&self, rooms: &'a [Rect]) -> Fit<'a> {
        Fit { rooms, pivot: self.pivot, length: self.length }
    }
}

/// The unit vector of a turn, exact on the axes so a beam along a room's edge stays on it.
fn direction(turn: i32) -> (f32, f32) {
    let a = turn as f32 * TAU / TURN_STEPS as f32;
    let snap = |v: f32| if v.abs() < AXIS_EPS { 0.0 } else { v };
    (snap(a.cos()), snap(a.sin()))
}

/// The start `shift` behind `pivot` on the line of `turn`.
fn start_of(pivot: (f32, f32), turn: i32, shift: f32) -> (f32, f32) {
    let (ux, uy) = direction(turn);
    (pivot.0 - ux * shift, pivot.1 - uy * shift)
}

fn beyond(p: (f32, f32), u: (f32, f32), by: f32) -> (f32, f32) {
    (p.0 + u.0 * by, p.1 + u.1 * by)
}

fn inside(rooms: &[Rect], (x, y): (f32, f32)) -> bool {
    rooms.iter().any(|r| x >= r.x as f32 && x < (r.x + r.w) as f32 && y >= r.y as f32 && y < (r.y + r.h) as f32)
}

/// Whether `p` lies strictly inside a room, so a line through `p` crosses that room
/// whichever way it turns.
fn strictly_inside(rooms: &[Rect], (x, y): (f32, f32)) -> bool {
    rooms.iter().any(|r| x > r.x as f32 && x < (r.x + r.w) as f32 && y > r.y as f32 && y < (r.y + r.h) as f32)
}

/// The farthest a step may cross empty space: the width and height of the rooms' bounds
/// together, longer than any straight path within them.
fn span(rooms: &[Rect]) -> f32 {
    let extent = |lo: fn(&Rect) -> i32, hi: fn(&Rect) -> i32| {
        rooms.iter().map(hi).max().unwrap_or(0) - rooms.iter().map(lo).min().unwrap_or(0)
    };
    (extent(|r| r.x, |r| r.x + r.w) + extent(|r| r.y, |r| r.y + r.h)) as f32
}

/// The first distance `lands` takes: a full SLIDE_STEP, else ever shorter ones FIT_STEP apart,
/// else ever longer ones out to `reach`.
fn step_along<T>(reach: f32, lands: impl FnMut(f32) -> Option<T>) -> Option<T> {
    let shorter = (0..).map(|k| SLIDE_STEP - k as f32 * FIT_STEP).take_while(|&d| d >= FIT_STEP);
    let longer = (1..).map(|k| SLIDE_STEP + k as f32 * FIT_STEP).take_while(move |&d| d <= reach);
    shorter.chain(longer).find_map(lands)
}

/// The pivot a move from `pivot` by `by` lands on: a full step where it stays strictly
/// inside a room, else the farthest point short of it that does, else the nearest past it.
fn moved(rooms: &[Rect], pivot: (f32, f32), by: (i32, i32)) -> Option<(f32, f32)> {
    step_along(span(rooms), |d| {
        Some((pivot.0 + by.0 as f32 * d, pivot.1 + by.1 as f32 * d)).filter(|&to| strictly_inside(rooms, to))
    })
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

/// The lines a beam may take about a pivot: those with the start inside a room.
#[derive(Clone, Copy)]
struct Fit<'a> {
    rooms: &'a [Rect],
    pivot: (f32, f32),
    length: f32,
}

impl Fit<'_> {
    fn fits(&self, turn: i32, shift: f32) -> bool {
        let s = start_of(self.pivot, turn, shift);
        inside(self.rooms, s)
    }

    /// The shift a slide from `shift` by `by` steps lands on: a full step where the line
    /// fits, else the farthest fitting point short of it, else the nearest past it.
    fn slid(&self, turn: i32, shift: f32, by: i32) -> Option<f32> {
        step_along(span(self.rooms), |d| Some(shift + by as f32 * d).filter(|&to| self.fits(turn, to)))
    }

    /// `shift` pulled towards the pivot until the line of `turn` fits; the pivot always does.
    fn pulled_in(&self, turn: i32, mut shift: f32) -> f32 {
        while shift != 0.0 && !self.fits(turn, shift) {
            shift = if shift > 0.0 { (shift - FIT_STEP).max(0.0) } else { (shift + FIT_STEP).min(0.0) };
        }
        shift
    }

    /// The turn and shift, in whole slide steps, of a line over the pivot through the most
    /// rooms, then the most length inside them; the nearer start, then the
    /// first turn, on a tie.
    fn best_line(&self) -> (i32, f32) {
        let mut best: Option<((i32, f32), (usize, f32))> = None;
        for turn in 0..TURN_STEPS {
            for k in 0..=(self.length / SLIDE_STEP) as i32 {
                let shift = k as f32 * SLIDE_STEP;
                if !self.fits(turn, shift) {
                    continue;
                }
                let score = crossing(self.rooms, start_of(self.pivot, turn, shift), direction(turn), self.length);
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
        if !strictly_inside(&rooms, (cx, cy)) {
            return;
        }
        let (turn, shift) = Fit { rooms: &rooms, pivot: (cx, cy), length }.best_line();
        self.actions.beam = Some(BeamLine {
            weapon: weapon.addr(),
            placed: (cx, cy),
            pivot: (cx, cy),
            turn,
            shift,
            length,
            repeat: None,
            // The click that placed the start, still held, lets go without firing.
            press: self.cursor.l_held.then_some(false),
        });
        self.place_beam(points);
    }

    /// Writes the beam's start into the game and puts the cursor past its end, short of the
    /// frame's edge.
    fn place_beam(&mut self, points: Obj) {
        let (Some(b), Some(gui), Some(first)) = (self.actions.beam, state::gui(), points.obj(VECTOR_BEGIN)) else {
            return;
        };
        let (ux, uy) = direction(b.turn);
        let s = start_of(b.pivot, b.turn, b.shift);
        first.set(0, [s.0, s.1]);
        let o = state::enemy_origin(gui);
        let at = (o.x as f32 + s.0, o.y as f32 + s.1);
        let reach = frame_reach(at, (ux, uy)).min(b.length);
        (self.cursor.x, self.cursor.y) = beyond(at, (ux, uy), reach);
        self.send_cursor();
    }

    /// Takes one step of the beam, its line kept fitting: a slide or move stops short or is
    /// not taken, and a start that no longer fits is pulled in towards the pivot. Returns
    /// whether the step was taken.
    fn take_step(&mut self, step: Step, points: Obj) -> bool {
        let (Some(rooms), Some(b)) = (enemy_rooms(), self.actions.beam.as_mut()) else {
            return false;
        };
        match step {
            Step::Turn(by) => b.turn = (b.turn + by).rem_euclid(TURN_STEPS),
            Step::Slide(by) => {
                // A shift moves the start against the beam.
                let Some(shift) = b.fit(&rooms).slid(b.turn, b.shift, -by) else {
                    return false;
                };
                b.shift = shift;
            }
            Step::Move(x, y) => {
                let Some(pivot) = moved(&rooms, b.pivot, (x, y)) else {
                    return false;
                };
                b.pivot = pivot;
            }
        }
        b.shift = b.fit(&rooms).pulled_in(b.turn, b.shift);
        self.place_beam(points);
        self.buzz();
        true
    }

    /// The aiming points and the line of the beam being set, if one is.
    fn beam_being_set(&mut self) -> Option<(Obj, &mut BeamLine)> {
        let (points, weapon) = placed_beam()?;
        let b = self.actions.beam.as_mut().filter(|b| b.weapon == weapon.addr())?;
        Some((points, b))
    }

    /// Takes a D-pad or stick step on a beam being set; false when none is.
    pub fn beam_step(&mut self, dx: f32, dy: f32) -> bool {
        let Some((points, b)) = self.beam_being_set() else {
            return false;
        };
        let step = Step::of(dx, dy, b.press.is_some());
        b.press = b.press.map(|_| false);
        // A held direction's repeats stay quiet at a limit.
        if !self.take_step(step, points) {
            self.bump();
        }
        if let Some(b) = self.actions.beam.as_mut() {
            b.repeat = Some((step, now_s() + REPEAT_DELAY_S));
        }
        true
    }

    /// Repeats the step of the direction `held` while it stays held on a beam being set.
    pub fn repeat_beam(&mut self, held: Option<(f32, f32)>) {
        let Some(b) = self.actions.beam else {
            return;
        };
        let Some((step, at)) = b.repeat else {
            return;
        };
        if held.map(|(x, y)| Step::of(x, y, b.press.is_some())) != Some(step) {
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

    /// Holds a click over a beam being set until its release; false when no beam is.
    pub fn press_beam(&mut self) -> bool {
        let Some((_, b)) = self.beam_being_set() else {
            return false;
        };
        b.press = Some(true);
        true
    }

    /// Lets go of a click held over a beam being set, firing the beam unless the line moved.
    pub fn release_beam(&mut self) {
        if self.actions.beam.as_mut().and_then(|b| b.press.take()) == Some(true) {
            self.click_tap(MouseButton::Left);
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
        (self.cursor.x, self.cursor.y) = (o.x as f32 + b.placed.0, o.y as f32 + b.placed.1);
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
    use crate::config::layout::{Builtin, Input};
    use crate::game::api::Point;
    use crate::game::fake::{calls, with_game, Call, Game, WEAPON_SIZE};
    use crate::input::bindings::Source;
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
        let fit = Fit { rooms: &rooms, pivot: center(rooms[0]), length: 80.0 };
        let (turn, shift) = fit.best_line();
        let s = start_of(fit.pivot, turn, shift);
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
        let fit = Fit { rooms: &rooms, pivot: center(rooms[3]), length: 45.0 };
        let (turn, shift) = fit.best_line();
        assert!((0.0..=fit.length).contains(&shift), "the centre on the beam: {shift}");
        assert!(fit.fits(turn, shift));
    }

    #[test]
    fn a_slid_start_lets_a_short_beam_cross_both_neighbours() {
        const PIKE: f32 = 45.0;
        let rooms = [room(0, 0), room(1, 0), room(2, 0)];
        let fit = Fit { rooms: &rooms, pivot: center(rooms[1]), length: PIKE };
        assert_eq!(crossing(&rooms, fit.pivot, direction(0), PIKE).0, 2, "from the centre one neighbour");
        let (turn, shift) = fit.best_line();
        assert_eq!(crossing(&rooms, start_of(fit.pivot, turn, shift), direction(turn), PIKE).0, 3);
        assert!(shift != 0.0);
    }

    #[test]
    fn a_slide_forward_crosses_rooms_and_stops_at_the_ships_edge() {
        let rooms = [room(0, 0), room(1, 0)];
        let fit = Fit { rooms: &rooms, pivot: center(rooms[0]), length: 45.0 };
        let once = fit.slid(0, 0.0, -1).expect("a full step fits");
        assert_eq!(once, -SLIDE_STEP);
        let mut shift = once;
        while let Some(to) = fit.slid(0, shift, -1) {
            shift = to;
        }
        let s = start_of(fit.pivot, 0, shift);
        let edge = (2 * TILE) as f32;
        assert!(s.0 < edge && s.0 >= edge - FIT_STEP, "past the next room's wall to the ship's edge: {s:?}");
    }

    #[test]
    fn a_slide_stops_at_a_gap_then_crosses_it_to_the_next_room() {
        let rooms = [room(0, 0), room(2, 0)];
        let fit = Fit { rooms: &rooms, pivot: center(rooms[0]), length: 45.0 };
        let x = |shift| start_of(fit.pivot, 0, shift).0;
        let mut shift = fit.slid(0, 0.0, -1).expect("a full step fits");
        shift = fit.slid(0, shift, -1).expect("a step short of the gap fits");
        assert_eq!(x(shift), TILE as f32 - FIT_STEP, "at the room's edge");
        shift = fit.slid(0, shift, -1).expect("the next room lies that way");
        let wall = (2 * TILE) as f32;
        assert!(x(shift) >= wall && x(shift) < wall + FIT_STEP, "just past the gap: {}", x(shift));
    }

    #[test]
    fn a_move_crosses_a_gap_and_stops_at_the_ships_edge() {
        let rooms = [room(0, 0), room(2, 0)];
        let y = center(rooms[0]).1;
        let (x, to_y) = moved(&rooms, (TILE as f32 - FIT_STEP, y), (1, 0)).expect("the next room lies that way");
        let wall = (2 * TILE) as f32;
        assert!(x > wall && x <= wall + FIT_STEP && to_y == y, "just past the gap: {x}");
        assert_eq!(moved(&rooms, ((3 * TILE) as f32 - FIT_STEP, y), (1, 0)), None, "nothing past the ship");
    }

    /// An enemy with a row of rooms, an armed beam of `length` and its start on the first
    /// room's centre, the cursor focused there; returns the aiming points' vector.
    fn placed(g: &mut Game, l: &mut Layer, length: i32) -> Obj {
        placed_in(g, l, length, &[room(0, 0), room(1, 0), room(2, 0)])
    }

    /// As `placed`, with the enemy's rooms `rooms`.
    fn placed_in(g: &mut Game, l: &mut Layer, length: i32, rooms: &[Rect]) -> Obj {
        let combat = g.gui.at(GUI_COMBAT_CONTROL);
        combat.set(COMBAT_POSITION, Point { x: 800, y: 100 });
        let enemy = g.enemy_manager().at(SHIPMANAGER_SHIP);
        let rooms: Vec<_> = rooms.iter().zip(0..).map(|(&r, id)| Some(g.room(id, r))).collect();
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
            assert!(l.beam_step(0.0, 1.0));
            assert_eq!(start(points), [c.0 + SLIDE_STEP, c.1], "down: a step towards the end");
            assert!(l.beam_step(0.0, -1.0) && l.beam_step(0.0, -1.0));
            assert_eq!(start(points), [c.0 - SLIDE_STEP, c.1], "up: back past the centre");
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
    fn down_slides_the_start_towards_the_end_and_up_away_whatever_the_turn() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            l.update_beam();
            for turn in 1..=TURN_STEPS {
                l.beam_step(1.0, 0.0);
                let (ux, uy) = direction(turn);
                let along = |from: [f32; 2], to: [f32; 2]| (to[0] - from[0]) * ux + (to[1] - from[1]) * uy;
                let at = start(points);
                assert!(l.beam_step(0.0, 1.0));
                assert!(along(at, start(points)) > 0.0, "down towards the end at turn {turn}");
                let at = start(points);
                assert!(l.beam_step(0.0, -1.0));
                assert!(along(at, start(points)) < 0.0, "up away from it at turn {turn}");
            }
        });
    }

    #[test]
    fn a_held_a_moves_the_line_across_rooms_short_of_their_walls() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            let c = center(room(0, 0));
            l.update_beam();
            assert!(l.press_beam());
            assert!(l.beam_step(0.0, 1.0));
            assert_eq!(start(points), [c.0, c.1 + SLIDE_STEP], "down the room, the turn kept");
            assert_eq!(l.cursor.y, 100.0 + c.1 + SLIDE_STEP);
            l.beam_step(0.0, 1.0);
            assert_eq!(start(points)[1], TILE as f32 - FIT_STEP, "short of the bottom wall");
            l.beam_step(0.0, 1.0);
            assert_eq!(start(points)[1], TILE as f32 - FIT_STEP, "no farther off the ship");
            l.beam_step(1.0, 0.0);
            l.beam_step(1.0, 0.0);
            assert_eq!(start(points)[0], TILE as f32 - FIT_STEP, "short of the next room's wall");
            l.beam_step(1.0, 0.0);
            assert!(start(points)[0] > TILE as f32, "into the next room");
        });
    }

    #[test]
    fn a_beam_fires_on_as_release_unless_a_moved_its_line() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            l.update_beam();
            let a = |l: &mut Layer, down| l.builtin(Builtin::ContextClick, Source::Input(Input::FaceDown), down);
            calls();
            a(&mut l, true);
            assert!(calls().is_empty(), "no click on the press");
            a(&mut l, false);
            let clicks: Vec<_> = calls().into_iter().filter(|c| matches!(c, Call::Button(..))).collect();
            assert!(matches!(
                clicks[..],
                [Call::Button(MouseButton::Left, true, ..), Call::Button(MouseButton::Left, false, ..)]
            ));
            let before = start(points);
            a(&mut l, true);
            l.beam_step(0.0, 1.0);
            a(&mut l, false);
            assert_ne!(start(points), before);
            assert!(!calls().iter().any(|c| matches!(c, Call::Button(..))), "a move does not fire");
        });
    }

    #[test]
    fn the_a_that_placed_the_start_moves_the_line_and_does_not_fire() {
        with_game(|g| {
            let mut l = Layer::default();
            let points = placed(g, &mut l, 80);
            let c = center(room(0, 0));
            l.cursor.l_held = true;
            l.update_beam();
            l.beam_step(0.0, -1.0);
            assert_eq!(start(points), [c.0, c.1 - SLIDE_STEP]);
            calls();
            l.release_beam();
            assert!(calls().is_empty());
        });
    }

    #[test]
    fn a_slide_stops_where_the_start_reaches_the_ships_edge() {
        with_game(|g| {
            let mut l = Layer::default();
            let column = [room(0, 0), room(0, 1), room(0, 2)];
            let points = placed_in(g, &mut l, 80, &column);
            let c = center(column[0]);
            l.update_beam();
            if let Some(b) = l.actions.beam.as_mut() {
                (b.turn, b.shift) = (TURN_STEPS / 4, 0.0);
            }
            for _ in 0..3 {
                assert!(l.beam_step(0.0, -1.0));
            }
            assert_eq!(start(points), [c.0, 0.0], "the start at the top wall, the beam still down");
            assert_eq!((l.cursor.x, l.cursor.y), (800.0 + c.0, 100.0 + 80.0), "the cursor past the end");
        });
    }

    #[test]
    fn a_step_at_a_limit_rumbles_longer_and_its_repeats_do_not() {
        with_game(|g| {
            let mut l = Layer::default();
            placed(g, &mut l, 80);
            l.update_beam();
            l.pad.index = Some(0);
            assert!(l.press_beam());
            calls();
            for _ in 0..3 {
                l.beam_step(0.0, 1.0);
            }
            let rumbles = |calls: Vec<Call>| -> Vec<f32> {
                calls.into_iter().filter_map(|c| if let Call::Rumble(s) = c { Some(s) } else { None }).collect()
            };
            let r = rumbles(calls());
            assert_eq!(r.len(), 3, "two moves down the room, then its edge: {r:?}");
            assert!(r[2] > r[0], "the edge rumbles longer: {r:?}");
            if let Some(b) = l.actions.beam.as_mut() {
                b.repeat = Some((Step::Move(0, 1), 0.0));
            }
            l.repeat_beam(Some((0.0, 1.0)));
            assert!(rumbles(calls()).is_empty(), "a repeat at the edge stays quiet");
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
