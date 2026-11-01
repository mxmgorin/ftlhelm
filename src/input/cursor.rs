//! The virtual cursor: game to window coordinates, stick movement, D-pad focus moves
//! between zones, and the first item a screen opens on.

use crate::core::layer::Layer;
use crate::core::util::{clampf, log, now_s};
use crate::game::actions::mouse_move;
use crate::game::api::{Point, Rect};
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;
use crate::game::zones::{Zone, ZoneKind};
use crate::ui::zoom::Span;

/// Frames a newly opened screen gets to show its items before the focus gives up; a
/// dialog's choices wait for the jump animation.
const FIRST_FOCUS_FRAMES: i32 = 300;
/// Frames a newly opened screen's buttons get to settle: its first hit-tests may hold the
/// screen before, or miss buttons it turns on in its first loop.
const FIRST_FOCUS_SETTLE: i32 = 2;

#[derive(Default)]
pub struct Cursor {
    pub x: f32,
    pub y: f32,
    /// The position last reported to the game, logical and in window pixels.
    pub last_x: i32,
    pub last_y: i32,
    pub last_wx: i32,
    pub last_wy: i32,
    /// The game's own cursor after our last move; a different one means it moved by itself.
    pub last_set: Point,
    pub synced: bool,
    pub l_held: bool,
    pub r_held: bool,
    /// Where a focus move left the cursor, and when: the zone there shows focused while the
    /// cursor stays.
    pub focus: Option<((i32, i32), f64)>,
}

/// Screens that open on their first item.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum FirstScreen {
    #[default]
    None,
    MainMenu,
    GameMenu,
    Choice,
    Map,
    Options,
    OptionsNotice,
}

/// Tracks screens opening, to put the cursor on their first item.
#[derive(Default)]
pub struct FirstFocus {
    prev: FirstScreen,
    prev_text: usize,
    prev_open_time: f32,
    waiting: i32,
    settling: i32,
    open_at: (f32, f32),
}

/// The menu or dialog on screen, if it is one that opens on its first item.
fn first_screen() -> FirstScreen {
    if let Some(options) = state::open_options() {
        let notice = state::options_choices(options).is_some_and(|c| c != options);
        return if notice { FirstScreen::OptionsNotice } else { FirstScreen::Options };
    }
    if state::gui_window_open(GUI_MENU_BOX) {
        FirstScreen::GameMenu
    } else if state::choice_open() {
        FirstScreen::Choice
    } else if state::star_map().is_some_and(|m| m.flag(FOCUS_OPEN) && !m.flag(STARMAP_CHOOSING_SECTOR)) {
        FirstScreen::Map
    } else if state::main_menu_open() {
        FirstScreen::MainMenu
    } else {
        FirstScreen::None
    }
}

/// The game's map from logical positions to window pixels, per axis: window =
/// (logical + offset) / scale + bar.
struct WindowMap {
    scale: [f32; 2],
    offset: [i32; 2],
    bar: [i32; 2],
}

/// The game's current WindowMap; None before the game has a CApp.
fn window_map() -> Option<WindowMap> {
    let app = state::app()?;
    let l = layout();
    let scale = |off| {
        let m = app.f32(off);
        if m <= 0.0 {
            1.0
        } else {
            m
        }
    };
    Some(WindowMap {
        scale: [scale(l.capp_mouse_modifier_x), scale(l.capp_mouse_modifier_y)],
        offset: [app.i32(l.capp_modifier_x), app.i32(l.capp_modifier_y)],
        bar: [app.i32(l.capp_x_bar), app.i32(l.capp_y_bar)],
    })
}

impl Layer {
    /// Maps a logical position to the window pixels the mouse handlers take.
    pub fn to_window(&self, gx: f32, gy: f32) -> (i32, i32) {
        let Some(m) = window_map() else {
            return (gx as i32, gy as i32);
        };
        let axis = |g: f32, k: usize| ((g + m.offset[k] as f32) / m.scale[k]).round() as i32 + m.bar[k];
        (axis(gx, 0), axis(gy, 1))
    }

    /// Logical pixels per window pixel through the game's transform.
    pub fn window_scale(&self) -> f32 {
        window_map().map_or(1.0, |m| m.scale[0])
    }

    /// Maps window pixels to the logical position the game's transform gives them; the
    /// inverse of to_window.
    pub fn logical_at(&self, wx: i32, wy: i32) -> (f32, f32) {
        let Some(m) = window_map() else {
            return (wx as f32, wy as f32);
        };
        let axis = |w: i32, k: usize| (w - m.bar[k]) as f32 * m.scale[k] - m.offset[k] as f32;
        (axis(wx, 0), axis(wy, 1))
    }

    /// Adopts the game's cursor when something else moved it.
    pub fn sync_cursor(&mut self) {
        let p = state::mouse_point();
        if self.cursor.synced && p == self.cursor.last_set {
            return;
        }
        if self.cursor.synced && self.touch_echo() {
            if self.debug {
                log!("touch: mouse echo at {},{} put back", p.x, p.y);
            }
            let c = &mut self.cursor;
            mouse_move(c.last_wx, c.last_wy, 0, 0, c.l_held, c.r_held);
            c.last_set = state::mouse_point();
            return;
        }
        let c = &mut self.cursor;
        let moved_elsewhere = c.synced;
        (c.x, c.y) = (p.x as f32, p.y as f32);
        (c.last_x, c.last_y) = (p.x, p.y);
        c.last_set = p;
        c.synced = true;
        (self.cursor.last_wx, self.cursor.last_wy) = self.to_window(self.cursor.x, self.cursor.y);
        if moved_elsewhere {
            if self.debug && self.touch_in_use() {
                log!("touch: left for the mouse at {},{}", p.x, p.y);
            }
            self.leave_touch();
        }
    }

    /// Reports the cursor to the game when its integer position changed.
    pub fn send_cursor(&mut self) {
        let (x, y) = (self.cursor.x as i32, self.cursor.y as i32);
        if x == self.cursor.last_x && y == self.cursor.last_y {
            return;
        }
        let (wx, wy) = self.to_window(self.cursor.x, self.cursor.y);
        let c = &mut self.cursor;
        mouse_move(wx, wy, wx - c.last_wx, wy - c.last_wy, c.l_held, c.r_held);
        (c.last_x, c.last_y, c.last_wx, c.last_wy) = (x, y, wx, wy);
        c.last_set = state::mouse_point();
    }

    /// Moves the cursor by a stick deflection over dt seconds.
    pub fn move_cursor(&mut self, rx: f32, ry: f32, dt: f32) {
        let mag = rx.hypot(ry);
        let dz = self.cfg.cursor_deadzone;
        if mag <= dz {
            return;
        }
        let k = clampf((mag - dz) / (1.0 - dz), 0.0, 1.0).powf(self.cfg.cursor_curve);
        let step = k * self.cfg.cursor_speed * dt;
        let span = if state::ship_view() { self.zoom.span } else { Span::FRAME };
        self.cursor.x = clampf(self.cursor.x + rx / mag * step, span.left, span.right - 1.0);
        self.cursor.y = clampf(self.cursor.y + ry / mag * step, 0.0, (GAME_H - 1) as f32);
        self.send_cursor();
    }

    /// Moves the cursor to the zone `pick_toward` finds along a direction.
    pub fn focus_move(&mut self, dx: f32, dy: f32) {
        let len = dx.hypot(dy);
        if self.zones.list.is_empty() || len <= 0.0 {
            return;
        }
        let from = self.zone_under_cursor();
        let rects: Vec<Rect> = self.zones.list.iter().map(|t| t.rect).collect();
        let aim = Aim {
            cones: [self.cfg.focus_cone_narrow, self.cfg.focus_cone_wide],
            side_weight: self.cfg.focus_side_weight,
        };
        let best = choice_step(&self.zones.list, from, (dx, dy))
            .or_else(|| pick_toward(&rects, from, (self.cursor.x, self.cursor.y), (dx, dy), aim));
        if self.debug {
            log!("focus {:.0},{:.0}: from {} to {}", dx / len, dy / len, index(from), index(best));
        }
        let Some(best) = best else {
            // Nothing that way: a zoomed view looks there instead.
            if self.zoom_pan(dx, dy) {
                self.buzz();
            }
            return;
        };
        (self.cursor.x, self.cursor.y) = rects[best].center();
        self.send_cursor();
        self.mark_focus();
        self.buzz();
    }

    /// Takes a D-pad step while the cursor holds an armament box: left or right carries the
    /// game's drag to the next box of its bar, up and down do nothing. False when none is held.
    pub fn focus_drag(&mut self, dx: f32, dy: f32) -> bool {
        if !self.cursor.l_held {
            return false;
        }
        let Some(from) = self.zone_under_cursor().filter(|&i| self.zones.list[i].kind == ZoneKind::Armament) else {
            return false;
        };
        if dy == 0.0 {
            if let Some(to) = self.zones.armament_beside(from, dx.signum() as i32) {
                (self.cursor.x, self.cursor.y) = self.zones.list[to].rect.center();
                self.send_cursor();
                self.mark_focus();
                self.buzz();
            }
        }
        true
    }

    /// How many times its size the game's pointer is drawn: as large on screen as on a whole
    /// frame shown at its size, up to `pointer_scale_max`.
    pub fn pointer_scale(&self) -> f32 {
        self.mark_scale().min(self.cfg.pointer_scale_max)
    }

    /// Marks the zone under the cursor as focused, for as long as the cursor stays.
    pub fn mark_focus(&mut self) {
        self.cursor.focus = Some(((self.cursor.last_x, self.cursor.last_y), now_s()));
    }

    /// The zone a focus move left the cursor on while it stays there, and the seconds since.
    pub fn focused_zone(&self, now: f64) -> Option<(usize, f64)> {
        let (at, since) = self.cursor.focus?;
        if at != (self.cursor.last_x, self.cursor.last_y) {
            return None;
        }
        Some((self.zone_under_cursor()?, now - since))
    }

    /// The first item of a screen among the zones: a dialog's first choice, the options' when
    /// listed, the map's beacon under the ship, else the topmost button, leftmost on a tie.
    fn first_item(&self, screen: FirstScreen) -> Option<usize> {
        let zones = &self.zones.list;
        let first_choice = || zones.iter().position(|t| t.kind == ZoneKind::Choice && t.id == 0);
        let top_button = || {
            zones
                .iter()
                .enumerate()
                .filter(|(_, t)| t.kind == ZoneKind::Button)
                .min_by_key(|(_, t)| (t.rect.y, t.rect.x))
                .map(|(i, _)| i)
        };
        match screen {
            FirstScreen::Map => {
                let map = state::star_map()?;
                let current = map.obj(STARMAP_CURRENT_LOC)?;
                let locations: Obj = map.at(STARMAP_LOCATIONS);
                zones
                    .iter()
                    .position(|t| t.kind == ZoneKind::Beacon && locations.vec_obj(t.id as usize) == Some(current))
            }
            FirstScreen::Choice | FirstScreen::OptionsNotice => first_choice(),
            FirstScreen::Options => first_choice().or_else(top_button),
            _ => top_button(),
        }
    }

    /// Puts the cursor on the first item of a menu, dialog or the map as it opens, unless
    /// the cursor moves first. A dialog that follows another counts as new when its text
    /// changes or its open timer restarts.
    pub fn focus_first_on_open(&mut self) {
        let screen = first_screen();
        let f = &mut self.focus;
        let mut opened = screen != f.prev;
        if screen == FirstScreen::Choice {
            if let Some(choice) = state::open_choice() {
                let text = choice.get::<usize>(CHOICE_TEXT);
                let open_time = choice.f32(CHOICE_OPEN_TIME);
                opened |= text != f.prev_text || open_time < f.prev_open_time;
                f.prev_text = text;
                f.prev_open_time = open_time;
            }
        }
        f.prev = screen;
        if opened {
            f.waiting = if screen == FirstScreen::None { 0 } else { FIRST_FOCUS_FRAMES };
            f.settling = FIRST_FOCUS_SETTLE;
            f.open_at = (self.cursor.x, self.cursor.y);
        }
        if f.waiting == 0 || (self.cursor.x, self.cursor.y) != f.open_at {
            f.waiting = 0;
            return;
        }
        f.waiting -= 1;
        if f.settling > 0 {
            f.settling -= 1;
            self.zones.buttons.clear();
            self.retest();
            return;
        }
        let Some(first) = self.first_item(screen) else {
            return;
        };
        (self.cursor.x, self.cursor.y) = self.zones.list[first].rect.center();
        self.send_cursor();
        self.mark_focus();
        self.focus.waiting = 0;
    }

    /// Keeps a focus on screen: when the zone a focus move left the cursor on goes away while
    /// the cursor stays, the focus moves to the nearest zone. Not while aiming or in doors mode.
    pub fn keep_focus(&mut self) {
        let busy = self.focus.waiting > 0 || self.wheel.open.is_some() || self.bind.precision || state::aiming();
        if !self.focus_held() || busy || self.zone_under_cursor().is_some() {
            return;
        }
        let here = (self.cursor.last_x as f32, self.cursor.last_y as f32);
        let Some(i) = nearest_zone(&self.zones.list, here) else {
            return;
        };
        (self.cursor.x, self.cursor.y) = self.zones.list[i].rect.center();
        self.send_cursor();
        self.mark_focus();
    }

    /// Whether the cursor stays where a focus move left it.
    pub fn focus_held(&self) -> bool {
        self.cursor.focus.is_some_and(|(at, _)| at == (self.cursor.last_x, self.cursor.last_y))
    }

    /// Whether the first item of a screen that opened is still awaited.
    pub fn first_focus_pending(&self) -> bool {
        self.focus.waiting > 0
    }

    /// Has the game hit-test the screen under the cursor again.
    fn retest(&mut self) {
        let c = &mut self.cursor;
        if c.synced {
            mouse_move(c.last_wx, c.last_wy, 0, 0, c.l_held, c.r_held);
            c.last_set = state::mouse_point();
        }
    }
}

/// The zone nearest to `at`, measured to its nearest point; None with no zones.
fn nearest_zone(zones: &[Zone], (x, y): (f32, f32)) -> Option<usize> {
    let distance = |r: Rect| {
        let dx = clampf(x, r.x as f32, (r.x + r.w) as f32) - x;
        let dy = clampf(y, r.y as f32, (r.y + r.h) as f32) - y;
        dx.hypot(dy)
    };
    zones.iter().enumerate().min_by(|(_, a), (_, b)| distance(a.rect).total_cmp(&distance(b.rect))).map(|(i, _)| i)
}

/// How a focus move weighs the zones it may land on.
#[derive(Clone, Copy)]
pub struct Aim {
    /// Half-angles in degrees, tried narrow first.
    pub cones: [f32; 2],
    /// How many times the sideways distance counts against the forward one.
    pub side_weight: f32,
}

/// Finds the zone nearest from `at` along `dir`, skipping `from`: inside the narrow cone
/// first, then the wide one, scored by distance grown with the angle off the axis. Both are
/// measured to a zone's nearest point, or its centre when `at` is inside it. Returns its index.
pub fn pick_toward(rects: &[Rect], from: Option<usize>, at: (f32, f32), dir: (f32, f32), aim: Aim) -> Option<usize> {
    let len = dir.0.hypot(dir.1);
    if rects.is_empty() || len <= 0.0 {
        return None;
    }
    let (dx, dy) = (dir.0 / len, dir.1 / len);
    let (cx, cy) = at;
    let mut best: Option<(usize, f32)> = None;
    for cone in aim.cones {
        let cos_cone = cone.to_radians().cos();
        for (i, r) in rects.iter().enumerate() {
            if Some(i) == from {
                continue;
            }
            let mut vx = clampf(cx, r.x as f32, (r.x + r.w) as f32) - cx;
            let mut vy = clampf(cy, r.y as f32, (r.y + r.h) as f32) - cy;
            if vx == 0.0 && vy == 0.0 {
                let (rx, ry) = r.center();
                (vx, vy) = (rx - cx, ry - cy);
            }
            let dist = vx.hypot(vy);
            if dist <= 0.0 {
                continue;
            }
            let cos_a = (vx * dx + vy * dy) / dist;
            if cos_a < cos_cone {
                continue;
            }
            let score = dist * (1.0 + aim.side_weight * (1.0 - cos_a));
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((i, score));
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(i, _)| i)
}

/// The choice after or before choice `from` in its dialog's order for a vertical move; None
/// when `from` is no choice, the move is sideways, or the list ends or turns a column that way.
fn choice_step(zones: &[Zone], from: Option<usize>, (dx, dy): (f32, f32)) -> Option<usize> {
    let from = &zones[from?];
    if from.kind != ZoneKind::Choice || dy.abs() <= dx.abs() {
        return None;
    }
    let down = dy > 0.0;
    let next = from.id + if down { 1 } else { -1 };
    zones.iter().position(|z| z.kind == ZoneKind::Choice && z.id == next && (z.rect.y > from.rect.y) == down)
}

/// A zone index for the log, -1 for none.
fn index(i: Option<usize>) -> i64 {
    i.map_or(-1, |i| i as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::with_game;
    use crate::game::zones::Button;

    const AIM: Aim = Aim { cones: [30.0, 60.0], side_weight: 2.0 };

    fn rect(x: i32, y: i32) -> Rect {
        Rect { x, y, w: 10, h: 10 }
    }

    #[test]
    fn nearest_along_the_axis_wins() {
        let rects = [rect(100, 0), rect(50, 0), rect(-50, 0)];
        assert_eq!(pick_toward(&rects, None, (0.0, 5.0), (1.0, 0.0), AIM), Some(1));
        assert_eq!(pick_toward(&rects, None, (0.0, 5.0), (-1.0, 0.0), AIM), Some(2));
    }

    #[test]
    fn narrow_cone_before_a_nearer_zone_off_axis() {
        // The second zone is nearer but about 45 degrees off, inside only the wide cone.
        let rects = [rect(200, 0), rect(40, 40)];
        assert_eq!(pick_toward(&rects, None, (0.0, 5.0), (1.0, 0.0), AIM), Some(0));
        assert_eq!(pick_toward(&rects[1..], None, (0.0, 5.0), (1.0, 0.0), AIM), Some(0));
    }

    #[test]
    fn the_zone_under_the_cursor_is_skipped() {
        let rects = [rect(0, 0), rect(50, 0)];
        assert_eq!(pick_toward(&rects, Some(0), (5.0, 5.0), (1.0, 0.0), AIM), Some(1));
    }

    #[test]
    fn nothing_that_way_and_no_direction() {
        let rects = [rect(50, 0)];
        assert_eq!(pick_toward(&rects, None, (0.0, 5.0), (0.0, -1.0), AIM), None);
        assert_eq!(pick_toward(&rects, None, (0.0, 5.0), (0.0, 0.0), AIM), None);
    }

    #[test]
    fn a_focus_move_frames_its_zone_while_the_cursor_stays() {
        with_game(|_| {
            let mut l = Layer::default();
            let zone = |x| Zone { rect: Rect { x, y: 100, w: 40, h: 40 }, id: 0, kind: ZoneKind::Button };
            l.zones.list = vec![zone(100), zone(200)];
            (l.cursor.x, l.cursor.y) = (120.0, 120.0);
            l.send_cursor();
            assert_eq!(l.focused_zone(now_s()), None, "no focus move yet");
            l.focus_move(1.0, 0.0);
            assert_eq!(l.focused_zone(now_s()).map(|(i, _)| i), Some(1));
            (l.cursor.x, l.cursor.y) = (300.0, 300.0);
            l.send_cursor();
            assert_eq!(l.focused_zone(now_s()), None, "the cursor moved on");
        });
    }

    #[test]
    fn up_and_down_step_through_event_choices_in_order_whatever_their_widths() {
        with_game(|_| {
            let mut l = Layer::default();
            let choice = |id, y, w| Zone { rect: Rect { x: 352, y, w, h: 32 }, id, kind: ZoneKind::Choice };
            l.zones.list = vec![choice(0, 280, 453), choice(1, 312, 339), choice(2, 344, 71)];
            (l.cursor.x, l.cursor.y) = l.zones.list[1].rect.center();
            l.send_cursor();
            l.focus_move(0.0, 1.0);
            assert_eq!(l.zone_under_cursor(), Some(2), "down to the short last choice");
            l.focus_move(0.0, 1.0);
            assert_eq!(l.zone_under_cursor(), Some(2), "the list ends there");
            l.focus_move(0.0, -1.0);
            l.focus_move(0.0, -1.0);
            assert_eq!(l.zone_under_cursor(), Some(0), "up through the list");
        });
    }

    #[test]
    fn the_options_open_on_their_first_choice_and_their_restart_notice_on_its_own() {
        with_game(|g| {
            let options = g.gui.at(GUI_OPTIONS_BOX);
            options.set(FOCUS_OPEN, 1u8);
            g.arena.vector(options, CHOICE_RECTS, &[rect(300, 200), rect(300, 240)]);
            let mut l = Layer::default();
            frames(&mut l, FIRST_FOCUS_SETTLE + 1);
            assert_eq!((l.cursor.x, l.cursor.y), rect(300, 200).center());
            let notice = options.at(layout().options_restart_dialog);
            notice.set(FOCUS_OPEN, 1u8);
            g.arena.vector(notice, CHOICE_RECTS, &[rect(600, 400)]);
            frames(&mut l, FIRST_FOCUS_SETTLE + 1);
            assert_eq!((l.cursor.x, l.cursor.y), rect(600, 400).center());
        });
    }

    /// Runs the zone build and the first focus as `n` frames do.
    fn frames(l: &mut Layer, n: i32) {
        for _ in 0..n {
            l.build_zones();
            l.focus_first_on_open();
        }
    }

    #[test]
    fn a_menu_opens_on_its_own_first_button_not_one_hit_tested_before() {
        with_game(|g| {
            let mut l = Layer::default();
            let button = |r: Rect| Button { rect: r, object: g.gui.addr() };
            l.zones.buttons = vec![button(rect(700, 100))];
            g.gui.at(GUI_MENU_BOX).set(FOCUS_OPEN, 1u8);
            frames(&mut l, FIRST_FOCUS_SETTLE + 1);
            assert_eq!((l.cursor.x, l.cursor.y), (0.0, 0.0), "the old screen's button left alone");
            l.zones.buttons = vec![button(rect(431, 231))];
            frames(&mut l, 1);
            assert_eq!((l.cursor.x, l.cursor.y), rect(431, 231).center());
        });
    }

    #[test]
    fn a_focus_whose_zone_goes_away_moves_to_the_nearest_zone_unless_aiming() {
        with_game(|g| {
            let mut l = Layer::default();
            let zone = |x, y| Zone { rect: rect(x, y), id: 0, kind: ZoneKind::Button };
            l.zones.list = vec![zone(100, 100)];
            (l.cursor.x, l.cursor.y) = l.zones.list[0].rect.center();
            l.send_cursor();
            l.mark_focus();
            l.zones.list = vec![zone(400, 100), zone(150, 120)];
            g.mouse.set(MOUSE_AIMING, 1i32);
            l.keep_focus();
            assert_eq!(l.zone_under_cursor(), None, "aiming keeps the cursor");
            g.mouse.set(MOUSE_AIMING, 0i32);
            l.keep_focus();
            assert_eq!(l.zone_under_cursor(), Some(1));
            assert!(l.focused_zone(now_s()).is_some(), "framed again");
            (l.cursor.x, l.cursor.y) = (600.0, 600.0);
            l.send_cursor();
            l.zones.list.clear();
            l.zones.list.push(zone(100, 100));
            l.keep_focus();
            assert_eq!((l.cursor.last_x, l.cursor.last_y), (600, 600), "a cursor moved off the focus stays");
        });
    }

    #[test]
    fn up_and_down_keep_to_a_column_of_choices() {
        with_game(|_| {
            let mut l = Layer::default();
            let choice = |id, x, y| Zone { rect: Rect { x, y, w: 200, h: 34 }, id, kind: ZoneKind::Choice };
            let below = Zone { rect: Rect { x: 251, y: 474, w: 131, h: 60 }, id: 0, kind: ZoneKind::Button };
            l.zones.list = vec![choice(0, 262, 217), choice(1, 262, 251), choice(2, 662, 217), below];
            (l.cursor.x, l.cursor.y) = l.zones.list[1].rect.center();
            l.send_cursor();
            l.focus_move(0.0, 1.0);
            assert_eq!(l.zone_under_cursor(), Some(3), "down to the button, not the next column's top");
            (l.cursor.x, l.cursor.y) = l.zones.list[2].rect.center();
            l.send_cursor();
            l.focus_move(0.0, -1.0);
            assert_eq!(l.zone_under_cursor(), Some(2), "nothing above the next column's top");
        });
    }

    #[test]
    fn window_pixels_map_back_to_the_logical_position() {
        with_game(|g| {
            let l = layout();
            g.app.set(l.capp_mouse_modifier_x, 2.0f32);
            g.app.set(l.capp_mouse_modifier_y, 2.0f32);
            g.app.set(l.capp_modifier_y, -40i32);
            g.app.set(l.capp_y_bar, 60i32);
            let layer = Layer::default();
            let (wx, wy) = layer.to_window(640.0, 360.0);
            assert_eq!((wx, wy), (320, 220));
            assert_eq!(layer.logical_at(wx, wy), (640.0, 360.0));
        });
    }

    #[test]
    fn inverted_rect_does_not_panic() {
        let rects = [Rect { x: 50, y: 0, w: -10, h: -10 }];
        let _ = pick_toward(&rects, None, (0.0, 5.0), (1.0, 0.0), AIM);
    }
}
