//! The virtual cursor: game to window coordinates, stick movement, D-pad focus moves
//! between zones, and the first item a screen opens on.

use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::game::actions::mouse_move;
use crate::game::api::{Point, Rect};
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;
use crate::game::zones::ZoneKind;

/// Frames a newly opened screen gets to show its items before the focus gives up; a
/// dialog's choices wait for the jump animation.
const FIRST_FOCUS_FRAMES: i32 = 300;

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
}

/// Tracks screens opening, to put the cursor on their first item.
#[derive(Default)]
pub struct FirstFocus {
    prev: FirstScreen,
    prev_text: usize,
    prev_open_time: f32,
    waiting: i32,
    open_at: (f32, f32),
}

/// The menu or dialog on screen, if it is one that opens on its first item.
fn first_screen() -> FirstScreen {
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

impl Layer {
    /// Maps a logical position to the window pixels the mouse handlers take.
    pub fn to_window(&self, gx: f32, gy: f32) -> (i32, i32) {
        let Some(app) = state::app() else {
            return (gx as i32, gy as i32);
        };
        let l = layout();
        let mx = app.f32(l.capp_mouse_modifier_x);
        let my = app.f32(l.capp_mouse_modifier_y);
        let mx = if mx <= 0.0 { 1.0 } else { mx };
        let my = if my <= 0.0 { 1.0 } else { my };
        let wx = ((gx + app.i32(l.capp_modifier_x) as f32) / mx).round() as i32 + app.i32(l.capp_x_bar);
        let wy = ((gy + app.i32(l.capp_modifier_y) as f32) / my).round() as i32 + app.i32(l.capp_y_bar);
        (wx, wy)
    }

    /// Adopts the game's cursor when something else moved it.
    pub fn sync_cursor(&mut self) {
        let p = state::mouse_point();
        let c = &mut self.cursor;
        if c.synced && p == c.last_set {
            return;
        }
        (c.x, c.y) = (p.x as f32, p.y as f32);
        (c.last_x, c.last_y) = (p.x, p.y);
        c.last_set = p;
        c.synced = true;
        (self.cursor.last_wx, self.cursor.last_wy) = self.to_window(self.cursor.x, self.cursor.y);
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
        self.cursor.x = clampf(self.cursor.x + rx / mag * step, 0.0, (GAME_W - 1) as f32);
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
        let best = pick_toward(&rects, from, (self.cursor.x, self.cursor.y), (dx, dy), aim);
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
        self.buzz();
    }

    /// The first item of a screen among the zones: a dialog's first choice, the map's beacon
    /// under the ship, else the topmost button, leftmost on a tie.
    fn first_item(&self, screen: FirstScreen) -> Option<usize> {
        let zones = &self.zones.list;
        match screen {
            FirstScreen::Map => {
                let map = state::star_map()?;
                let current = map.obj(STARMAP_CURRENT_LOC)?;
                let locations: Obj = map.at(STARMAP_LOCATIONS);
                zones
                    .iter()
                    .position(|t| t.kind == ZoneKind::Beacon && locations.vec_obj(t.id as usize) == Some(current))
            }
            FirstScreen::Choice => zones.iter().position(|t| t.kind == ZoneKind::Choice && t.id == 0),
            _ => zones
                .iter()
                .enumerate()
                .filter(|(_, t)| t.kind == ZoneKind::Button)
                .min_by_key(|(_, t)| (t.rect.y, t.rect.x))
                .map(|(i, _)| i),
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
            f.open_at = (self.cursor.x, self.cursor.y);
        }
        if f.waiting == 0 || (self.cursor.x, self.cursor.y) != f.open_at {
            f.waiting = 0;
            return;
        }
        f.waiting -= 1;
        let Some(first) = self.first_item(screen) else {
            return;
        };
        (self.cursor.x, self.cursor.y) = self.zones.list[first].rect.center();
        self.send_cursor();
        self.focus.waiting = 0;
    }
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

/// A zone index for the log, -1 for none.
fn index(i: Option<usize>) -> i64 {
    i.map_or(-1, |i| i as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn inverted_rect_does_not_panic() {
        let rects = [Rect { x: 50, y: 0, w: -10, h: -10 }];
        let _ = pick_toward(&rects, None, (0.0, 5.0), (1.0, 0.0), AIM);
    }
}
