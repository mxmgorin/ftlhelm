//! The zoomed view: the ship view shown magnified around the cursor.
//!
//! The game draws each frame into a GAME_W x GAME_H frame buffer and blits it to the
//! screen; the zoom changes only that blit, through the viewport and the model matrix.
//! The game's layout and hit-tests and the layer's cursor stay in frame pixels; a real
//! mouse is not mapped through the zoom.
//!
//! A level is a magnification of the whole frame as the game fits it to the screen. Each
//! frame the layer eases the shown level and the camera toward their goals and publishes
//! the view for the blit. The ship view keeps to the frame columns the HUD covers, its span:
//! its lowest level shows the span whole. The camera goal keeps the cursor, or a wheel open
//! around it, inside the view's inner part, resting on an edge of the span where that does. A
//! pan moves the goal by hand and holds it until the focus moves; a peek shows the span whole
//! while it lasts, as does an event dialog. While zoomed on a screen that shrinks the frame,
//! a window over the ship view is shown whole, as large as the screen allows; otherwise the
//! span is shown whole. An open wheel holds the level down to one whose view shows it whole.
//! A frame shown smaller than it is drawn is area-averaged.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::core::layer::Layer;
use crate::core::util::{clampf, log, now_s};
use crate::game::api::{api, Rect};
use crate::game::offsets::{GAME_H, GAME_W};
use crate::game::state;
use crate::ui::shrink;
use crate::ui::wheel::{wheel_area, WHEEL_SIZE};

/// The `zoom` setting that zooms one level in where the screen shrinks the frame.
pub const ZOOM_AUTO: f32 = 0.0;
pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 3.0;
/// Pixel-exact levels show a frame pixel on a multiple of this many screen pixels.
const EXACT_STEP: f32 = 0.5;
/// Levels within this ratio of each other count as one.
const LEVEL_MERGE: f32 = 1.04;
/// The share of the view at each edge the camera keeps its focus out of.
const EDGE: f32 = 0.2;
/// Frame pixels kept clear around a window fitted to the screen.
const WINDOW_MARGIN: f32 = 8.0;
/// A pan moves the camera by this share of the view.
const PAN_STEP: f32 = 0.5;
/// How fast the shown level and the camera close on their goals, per second.
const EASE_RATE: f32 = 12.0;
/// The gaps below which the shown level and the camera snap to their goals.
const SNAP_LEVEL: f32 = 0.002;
const SNAP_PX: f32 = 0.25;

/// Blits of the frame buffer so far; a frame without one leaves the zoom off.
static BLITS: AtomicU32 = AtomicU32::new(0);
/// What the next frame blit shows; None shows the whole frame as the game fits it.
static SHOWN: Mutex<Option<Blit>> = Mutex::new(None);

/// The frame columns the ship view keeps to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub left: f32,
    pub right: f32,
}

impl Span {
    pub const FRAME: Span = Span { left: 0.0, right: GAME_W as f32 };

    pub fn width(self) -> f32 {
        self.right - self.left
    }
}

impl Default for Span {
    fn default() -> Self {
        Span::FRAME
    }
}

/// A part of the frame as the screen shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The part shown, in frame pixels.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Screen pixels per frame pixel.
    pub k: f32,
}

/// Screen pixels per frame pixel when the whole frame fits the screen.
fn fit_scale((sw, sh): (f32, f32)) -> f32 {
    (sw / GAME_W as f32).min(sh / GAME_H as f32)
}

impl View {
    /// The view at `level` on a screen of `screen` pixels, centred as near `center` as the
    /// frame's rows and `span`'s columns allow; its origin on whole screen pixels.
    pub fn at(screen: (f32, f32), level: f32, (cx, cy): (f32, f32), span: Span) -> View {
        let k = fit_scale(screen) * level;
        let w = (screen.0 / k).min(span.width());
        let h = (screen.1 / k).min(GAME_H as f32);
        let x = (clampf(cx - w / 2.0, span.left, span.right - w) * k).floor() / k;
        let y = (clampf(cy - h / 2.0, 0.0, GAME_H as f32 - h) * k).floor() / k;
        View { x, y, w, h, k }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// What a frame blit shows: a part of the frame and the screen rectangle it fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blit {
    pub view: View,
    /// x, y, w, h in screen pixels, centred on the screen.
    pub viewport: [i32; 4],
}

impl Blit {
    fn new(view: View, (sw, sh): (f32, f32)) -> Blit {
        let (w, h) = ((view.w * view.k).round(), (view.h * view.k).round());
        let (x, y) = (((sw - w) / 2.0).round(), ((sh - h) / 2.0).round());
        Blit { view, viewport: [x as i32, y as i32, w as i32, h as i32] }
    }

    /// The frame point shown at screen pixel (`sx`, `sy`).
    fn to_frame(self, sx: f32, sy: f32) -> (f32, f32) {
        let [x, y, w, h] = self.viewport.map(|v| v as f32);
        let v = self.view;
        (v.x + (sx - x) * v.w / w, v.y + (sy - y) * v.h / h)
    }
}

fn same_level(a: f32, b: f32) -> bool {
    a.max(b) / a.min(b) < LEVEL_MERGE
}

/// The level that shows `span` whole on a screen of `screen` pixels; ZOOM_MIN for the
/// whole frame's columns.
fn span_level(screen: (f32, f32), span: Span) -> f32 {
    ((screen.0 / span.width()).min(screen.1 / GAME_H as f32) / fit_scale(screen)).max(ZOOM_MIN)
}

/// The levels zoom steps through on a screen of `screen` pixels over `span`, rising: the one
/// that shows the span whole, the pixel-exact levels past it, and the level that fills the
/// screen unless an exact one is as good as it.
fn levels(screen: (f32, f32), span: Span) -> Vec<f32> {
    let fit = fit_scale(screen);
    let lowest = span_level(screen, span);
    let mut out = vec![lowest];
    let mut k = (fit * lowest / EXACT_STEP).floor() * EXACT_STEP + EXACT_STEP;
    while k / fit <= ZOOM_MAX {
        out.push(k / fit);
        k += EXACT_STEP;
    }
    let fill = (screen.0 / span.width()).max(screen.1 / GAME_H as f32) / fit;
    if fill <= ZOOM_MAX && !out.iter().any(|&l| same_level(l, fill)) {
        out.push(fill);
    }
    out.sort_by(f32::total_cmp);
    out.dedup_by(|a, b| same_level(*a, *b));
    out
}

/// The level next to `level` among `levels`, up or down; the top stays, the lowest is
/// ZOOM_MIN.
fn step(levels: &[f32], level: f32, up: bool) -> f32 {
    if up {
        levels.iter().copied().find(|&l| l > level && !same_level(l, level)).unwrap_or(level)
    } else {
        levels.iter().skip(1).rev().copied().find(|&l| l < level && !same_level(l, level)).unwrap_or(ZOOM_MIN)
    }
}

/// The level that fits `rect`, with WINDOW_MARGIN around it, to the screen; within
/// ZOOM_MIN..=ZOOM_MAX.
fn fit_level(screen: (f32, f32), rect: Rect) -> f32 {
    let (w, h) = (rect.w as f32 + 2.0 * WINDOW_MARGIN, rect.h as f32 + 2.0 * WINDOW_MARGIN);
    clampf((screen.0 / w).min(screen.1 / h) / fit_scale(screen), ZOOM_MIN, ZOOM_MAX)
}

/// The highest level whose view shows an open wheel whole on a screen of `screen` pixels.
fn wheel_level(screen: (f32, f32)) -> f32 {
    let side = WHEEL_SIZE.ceil() as i32;
    fit_level(screen, Rect { x: 0, y: 0, w: side, h: side })
}

/// The smallest rectangle holding `rect` and the area `[left, top, right, bottom]`.
fn hull(rect: Rect, [left, top, right, bottom]: [f32; 4]) -> Rect {
    let x = rect.x.min(left.floor() as i32);
    let y = rect.y.min(top.floor() as i32);
    let right = (rect.x + rect.w).max(right.ceil() as i32);
    let bottom = (rect.y + rect.h).max(bottom.ceil() as i32);
    Rect { x, y, w: right - x, h: bottom - y }
}

/// Whether a screen of `screen` pixels shows the frame shrunk at the lowest level over `span`.
fn shrinks(screen: (f32, f32), span: Span) -> bool {
    fit_scale(screen) * span_level(screen, span) < 1.0
}

/// The level ZOOM_AUTO takes on a screen of `screen` pixels over `span`: the first past the
/// lowest where the screen shows the frame shrunk there, else the lowest.
fn auto_level(screen: (f32, f32), span: Span) -> f32 {
    let all = levels(screen, span);
    if !shrinks(screen, span) {
        return all[0];
    }
    step(&all, all[0], true)
}

/// What the view shows for a window covering `rect` on a screen of `screen` pixels: on a
/// screen that shrinks the frame and with the ship view zoomed, the window as large as it
/// allows; else the span whole, or the whole frame for a window reaching past the span.
fn window_scene(screen: (f32, f32), rect: Rect, level: f32, span: Span) -> Scene {
    if shrinks(screen, span) && resolve(level, screen, span) > ZOOM_MIN {
        Scene::Window(rect)
    } else if rect.x as f32 >= span.left && (rect.x + rect.w) as f32 <= span.right {
        Scene::Span
    } else {
        Scene::Whole
    }
}

/// The level the `zoom` setting `level` takes on a screen of `screen` pixels over `span`.
fn resolve(level: f32, screen: (f32, f32), span: Span) -> f32 {
    if level == ZOOM_AUTO {
        auto_level(screen, span)
    } else {
        clampf(level, span_level(screen, span), ZOOM_MAX)
    }
}

/// The level the `zoom` setting `level` takes on the current screen over `span`; ZOOM_MIN
/// before the game has a screen.
pub fn resolved_level(level: f32, span: Span) -> f32 {
    screen_size().map_or(ZOOM_MIN, |screen| resolve(level, screen, span))
}

/// The `zoom` setting a step up or down from `setting` among `levels`, from `shown`, the
/// level it takes; a step below the lowest is auto.
fn step_setting(levels: &[f32], setting: f32, shown: f32, up: bool) -> f32 {
    if !up && setting != ZOOM_AUTO && same_level(shown, levels[0]) {
        ZOOM_AUTO
    } else {
        step(levels, shown, up)
    }
}

/// The `zoom` setting after `setting` in the cycle, from `shown`, the level it takes: auto,
/// the whole frame, each of `levels` up, auto.
fn cycle_setting(levels: &[f32], setting: f32, shown: f32) -> f32 {
    if setting == ZOOM_AUTO {
        return ZOOM_MIN;
    }
    let next = step(levels, shown, true);
    if same_level(next, shown) {
        ZOOM_AUTO
    } else {
        next
    }
}

/// The zoom plate's pips for the `zoom` setting on the current screen over `span`: one per
/// level, the lowest included, and how many the setting reaches; none for auto.
pub fn setting_pips(setting: f32, span: Span) -> (i32, i32) {
    let Some(screen) = screen_size() else {
        return (0, 0);
    };
    let all = levels(screen, span);
    let level = resolve(setting, screen, span);
    let reached = all.iter().filter(|&&l| l <= level || same_level(l, level)).count();
    (all.len() as i32, if setting == ZOOM_AUTO { 0 } else { reached as i32 })
}

/// The centre along one axis for a view of `size` within `from..to` that holds `lo..hi`
/// inside its inner part: on an edge where that does, the nearer to `center` of two, else
/// the nearest to `center`; the middle of `lo..hi` when the inner part is too small. A view
/// on an edge keeps no margin on that side.
fn follow_axis(center: f32, size: f32, lo: f32, hi: f32, (from, to): (f32, f32)) -> f32 {
    let half_inner = size * (0.5 - EDGE);
    if hi - lo > 2.0 * half_inner {
        return (lo + hi) / 2.0;
    }
    let (low_edge, high_edge) = (from + size / 2.0, to - size / 2.0);
    [(low_edge, hi <= low_edge + half_inner), (high_edge, lo >= high_edge - half_inner)]
        .into_iter()
        .filter(|&(_, holds)| holds)
        .map(|(edge, _)| edge)
        .min_by(|a, b| (a - center).abs().total_cmp(&(b - center).abs()))
        .unwrap_or_else(|| clampf(center, hi - half_inner, lo + half_inner))
}

/// Moves `from` toward `to` over `dt` seconds; snaps when within `snap`.
fn ease(from: f32, to: f32, dt: f32, snap: f32) -> f32 {
    let next = from + (to - from) * (1.0 - (-EASE_RATE * dt).exp());
    if (to - next).abs() < snap {
        to
    } else {
        next
    }
}

/// Whether the screen is narrower than the frame, so a view that fills its height crops the
/// frame's sides.
pub fn narrower_than_frame() -> bool {
    screen_size().is_some_and(|(w, h)| w * (GAME_H as f32) < h * GAME_W as f32)
}

/// The screen's size in pixels; None before the game has a display.
fn screen_size() -> Option<(f32, f32)> {
    let a = api();
    // SAFETY: the engine's display size getters.
    let (w, h) = unsafe { ((a.display_width)(), (a.display_height)()) };
    (w > 0 && h > 0).then_some((w as f32, h as f32))
}

fn publish(blit: Option<Blit>) {
    *SHOWN.lock().unwrap_or_else(|e| e.into_inner()) = blit;
}

/// Runs the game's blit of a `frame_w` x `frame_h` frame through the published view;
/// `blit` takes whether to filter the frame smoothly.
pub fn blit_frame(frame_w: f32, frame_h: f32, smooth: bool, blit: impl FnOnce(bool)) {
    BLITS.fetch_add(1, Ordering::Relaxed);
    let shown = *SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    let a = api();
    let Some(Blit { view, viewport: [x, y, w, h] }) = shown else {
        // SAFETY: the engine's viewport getters; the game has set the frame's viewport.
        let (vw, vh) = unsafe { ((a.viewport_width)() as f32, (a.viewport_height)() as f32) };
        return blit_at(frame_w, frame_h, (frame_w / vw, frame_h / vh), smooth, blit);
    };
    let (sx, sy) = (frame_w / view.w, frame_h / view.h);
    // SAFETY: the renderer's viewport and matrix stack inside the game's frame blit; the
    // game sets its viewport anew each frame.
    unsafe {
        (a.set_viewport)(x, y, w, h);
        (a.push_matrix)();
        (a.translate)(-view.x * sx, -view.y * sy, 0.0);
        (a.scale)(sx, sy, 1.0);
    }
    // A whole number of screen pixels per frame pixel stays sharp unfiltered.
    blit_at(frame_w, frame_h, (view.w / w as f32, view.h / h as f32), view.k.fract() != 0.0, blit);
    // SAFETY: pops the matrix pushed above.
    unsafe { (a.pop_matrix)() };
}

/// Blits the frame at `span` frame pixels per screen pixel per axis: area-averaged where
/// the screen shrinks it, else filtered as `smooth` asks.
fn blit_at(frame_w: f32, frame_h: f32, span: (f32, f32), smooth: bool, blit: impl FnOnce(bool)) {
    if span.0 > 1.0 || span.1 > 1.0 {
        shrink::blit(frame_w, frame_h, span, || blit(true));
    } else {
        blit(smooth);
    }
}

/// What the view shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Scene {
    /// The ship view at the zoom level, under the camera.
    Ship,
    /// A window, whole and as large as the screen allows.
    Window(Rect),
    /// The ship view at its lowest level: the span whole.
    Span,
    Whole,
}

/// The zoom as the layer keeps it between frames.
pub struct Zoom {
    /// The ship view's level as the `zoom` setting gives it, ZOOM_AUTO included.
    pub level: f32,
    /// The frame columns the ship view keeps to.
    pub span: Span,
    /// The level the current screen takes: `level` on the ship view, else ZOOM_MIN.
    pub target: f32,
    /// The point the camera aims to centre.
    goal: (f32, f32),
    /// The focus when a pan last moved the goal; the goal stays while the focus does.
    pin: Option<[f32; 4]>,
    /// When the peek's button went down; the peek starts once it is held past tap_hold.
    pub peek_from: Option<f64>,
    /// The level and centre shown, easing toward `target` and `goal`.
    shown_level: f32,
    shown_center: (f32, f32),
    /// The view shown this frame; None while it is the whole frame.
    pub view: Option<View>,
    /// BLITS as the last update saw it.
    blits_seen: u32,
}

impl Default for Zoom {
    fn default() -> Self {
        let middle = (GAME_W as f32 / 2.0, GAME_H as f32 / 2.0);
        Zoom {
            level: ZOOM_MIN,
            span: Span::FRAME,
            target: ZOOM_MIN,
            goal: middle,
            pin: None,
            peek_from: None,
            shown_level: ZOOM_MIN,
            shown_center: middle,
            view: None,
            blits_seen: 0,
        }
    }
}

impl Zoom {
    /// Advances the shown level and camera by `dt` seconds toward what `scene` shows, the
    /// ship view and a window at most at level `top`; on the ship view the goal keeps `focus`
    /// (left, top, right, bottom) in view. Returns the view to show, None for the whole frame.
    fn advance(&mut self, screen: (f32, f32), scene: Scene, focus: [f32; 4], top: f32, dt: f32) -> Option<View> {
        let middle = (GAME_W as f32 / 2.0, GAME_H as f32 / 2.0);
        let (toward, span) = match scene {
            Scene::Ship => {
                self.target = resolve(self.level, screen, self.span).min(top);
                self.follow(screen, focus);
                (self.goal, self.span)
            }
            Scene::Window(rect) => {
                self.target = fit_level(screen, rect).min(top);
                (rect.center(), Span::FRAME)
            }
            Scene::Span => {
                self.target = span_level(screen, self.span);
                (((self.span.left + self.span.right) / 2.0, middle.1), self.span)
            }
            Scene::Whole => {
                self.target = ZOOM_MIN;
                (middle, Span::FRAME)
            }
        };
        let (tx, ty) = View::at(screen, self.target, toward, span).center();
        self.shown_level = ease(self.shown_level, self.target, dt, SNAP_LEVEL);
        let (cx, cy) = self.shown_center;
        self.shown_center = (ease(cx, tx, dt, SNAP_PX), ease(cy, ty, dt, SNAP_PX));
        (self.shown_level > ZOOM_MIN).then(|| View::at(screen, self.shown_level, self.shown_center, span))
    }

    /// Moves the ship view's camera goal to keep `focus` in view, unless a pan holds it.
    /// The goal stays off the ship view, so the zoom comes back where it was.
    fn follow(&mut self, screen: (f32, f32), focus: [f32; 4]) {
        if self.target <= ZOOM_MIN {
            return;
        }
        let goal = View::at(screen, self.target, self.goal, self.span);
        let (gx, gy) = goal.center();
        let [left, top, right, bottom] = focus;
        let aim = if self.pin == Some(focus) {
            (gx, gy)
        } else {
            self.pin = None;
            let (columns, rows) = ((self.span.left, self.span.right), (0.0, GAME_H as f32));
            (follow_axis(gx, goal.w, left, right, columns), follow_axis(gy, goal.h, top, bottom, rows))
        };
        self.goal = View::at(screen, self.target, aim, self.span).center();
    }

    /// Drops to the whole frame at once.
    fn reset_view(&mut self) {
        self.target = ZOOM_MIN;
        self.shown_level = ZOOM_MIN;
        self.view = None;
    }
}

impl Layer {
    /// Steps the ship view's level up or down through the screen's levels from the level
    /// shown; a step below the whole frame is auto.
    pub fn zoom_step(&mut self, up: bool) {
        let Some(screen) = screen_size() else {
            return;
        };
        let (setting, span) = (self.zoom.level, self.zoom.span);
        self.zoom.level = step_setting(&levels(screen, span), setting, resolve(setting, screen, span), up);
        log!("zoom {:.2}", self.zoom.level);
    }

    /// Moves the ship view to the next in the cycle: auto, the whole frame, each of the
    /// screen's levels up, auto.
    pub fn zoom_cycle(&mut self) {
        let Some(screen) = screen_size() else {
            return;
        };
        let (setting, span) = (self.zoom.level, self.zoom.span);
        self.zoom.level = cycle_setting(&levels(screen, span), setting, resolve(setting, screen, span));
        log!("zoom {:.2}", self.zoom.level);
    }

    /// Moves the zoomed camera a step toward (`dx`, `dy`) along the stronger axis and holds
    /// it there until the focus moves; returns whether the view is zoomed.
    pub fn zoom_pan(&mut self, dx: f32, dy: f32) -> bool {
        let (Some(view), Some(screen)) = (self.zoom.view, screen_size()) else {
            return false;
        };
        let (mut x, mut y) = self.zoom.goal;
        if dx.abs() >= dy.abs() {
            x += dx.signum() * view.w * PAN_STEP;
        } else {
            y += dy.signum() * view.h * PAN_STEP;
        }
        self.zoom.goal = View::at(screen, self.zoom.target, (x, y), self.zoom.span).center();
        self.zoom.pin = Some(self.zoom_focus());
        true
    }

    /// The frame point the screen shows at window pixel (`wx`, `wy`): through the zoomed
    /// view, else through the game's own transform; on the frame's edge for a point past it.
    pub fn frame_at(&self, wx: i32, wy: i32) -> (f32, f32) {
        let (x, y) = match (self.zoom.view, screen_size()) {
            (Some(view), Some(screen)) => Blit::new(view, screen).to_frame(wx as f32, wy as f32),
            _ => self.logical_at(wx, wy),
        };
        (clampf(x, 0.0, (GAME_W - 1) as f32), clampf(y, 0.0, (GAME_H - 1) as f32))
    }

    /// Frame pixels per window pixel as the screen shows the frame, zoomed or not.
    pub fn frame_per_window(&self) -> f32 {
        match (self.zoom.view, screen_size()) {
            (Some(view), Some(_)) => 1.0 / view.k,
            _ => self.window_scale(),
        }
    }

    /// Frame pixels per screen pixel, at least 1: drawn this many times its size, a mark keeps
    /// at least its size on a screen that shows the frame smaller.
    pub fn mark_scale(&self) -> f32 {
        self.frame_per_window().max(1.0)
    }

    /// Starts the peek's hold when its button goes down, ends it when it comes up.
    pub fn zoom_peek(&mut self, down: bool) {
        self.zoom.peek_from = down.then(now_s);
    }

    /// The frame area the camera keeps in view: that of a wheel open around the cursor, else
    /// the cursor.
    fn zoom_focus(&self) -> [f32; 4] {
        let (x, y) = (self.cursor.x, self.cursor.y);
        if self.wheel_at_cursor() {
            wheel_area(x, y)
        } else {
            [x, y, x, y]
        }
    }

    /// Eases the view toward the screen's level and a camera goal that keeps the focus in
    /// view, and publishes it for the next frame blit. Without frame blits, the game shows
    /// the whole frame.
    pub fn update_zoom(&mut self, dt: f32) {
        let blits = BLITS.load(Ordering::Relaxed);
        let blitting = blits != self.zoom.blits_seen;
        self.zoom.blits_seen = blits;
        let Some(screen) = screen_size().filter(|_| blitting) else {
            self.zoom.reset_view();
            publish(None);
            return;
        };
        let peeking = self.zoom.peek_from.is_some_and(|t| now_s() - t >= self.cfg.tap_hold as f64);
        let focus = self.zoom_focus();
        let scene = if self.help_shown {
            Scene::Whole
        } else if peeking {
            Scene::Span
        } else if state::ship_view() {
            Scene::Ship
        } else if let Some(rect) = state::window_rect() {
            // An open wheel shows whole beside the window.
            let rect = if self.wheel_at_cursor() { hull(rect, focus) } else { rect };
            window_scene(screen, rect, self.zoom.level, self.zoom.span)
        } else if state::choice_open() {
            Scene::Span
        } else {
            Scene::Whole
        };
        let top = if self.wheel.open.is_some() { wheel_level(screen) } else { ZOOM_MAX };
        let view = self.zoom.advance(screen, scene, focus, top, dt);
        self.zoom.view = view;
        publish(view.map(|v| Blit::new(v, screen)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: (f32, f32) = (640.0, 480.0);
    const SQUARE: (f32, f32) = (720.0, 720.0);
    const WIDE: (f32, f32) = (1280.0, 720.0);
    /// A frame step at the game's frame rate.
    const DT: f32 = 1.0 / 60.0;

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    #[test]
    fn levels_are_the_whole_frame_the_fill_and_the_pixel_exact_ones() {
        assert!(close(&levels(SMALL, Span::FRAME), &[1.0, 1.333, 2.0, 3.0]));
        assert!(close(&levels(SQUARE, Span::FRAME), &[1.0, 1.778, 2.667]));
        assert!(close(&levels(WIDE, Span::FRAME), &[1.0, 1.5, 2.0, 2.5, 3.0]));
    }

    #[test]
    fn steps_stop_at_the_ends() {
        let l = levels(SMALL, Span::FRAME);
        assert_eq!(step(&l, ZOOM_MIN, true), l[1]);
        assert_eq!(step(&l, l[1], false), ZOOM_MIN);
        assert_eq!(step(&l, ZOOM_MIN, false), ZOOM_MIN);
        assert_eq!(step(&l, ZOOM_MAX, true), ZOOM_MAX);
        assert_eq!(step(&l, 1.7, true), 2.0);
    }

    #[test]
    fn auto_zooms_one_level_in_where_the_screen_shrinks_the_frame() {
        assert!(close(
            &[auto_level(SMALL, Span::FRAME), auto_level(SQUARE, Span::FRAME), auto_level((854.0, 480.0), Span::FRAME)],
            &[1.333, 1.778, 1.5]
        ));
        assert_eq!(auto_level(WIDE, Span::FRAME), ZOOM_MIN);
        assert_eq!(resolve(ZOOM_AUTO, WIDE, Span::FRAME), ZOOM_MIN);
        assert_eq!(resolve(2.0, SMALL, Span::FRAME), 2.0);
    }

    #[test]
    fn settings_step_from_what_auto_shows_and_back_to_auto_below_the_whole_frame() {
        let (l, auto) = (levels(SMALL, Span::FRAME), auto_level(SMALL, Span::FRAME));
        assert_eq!(step_setting(&l, ZOOM_AUTO, auto, true), step(&l, auto, true));
        assert_eq!(step_setting(&l, ZOOM_AUTO, auto, false), ZOOM_MIN);
        assert_eq!(step_setting(&l, ZOOM_MIN, ZOOM_MIN, false), ZOOM_AUTO);
        assert_eq!(step_setting(&l, ZOOM_AUTO, ZOOM_MIN, false), ZOOM_MIN);
        assert_eq!(step_setting(&l, ZOOM_MAX, ZOOM_MAX, true), ZOOM_MAX);
    }

    #[test]
    fn a_cycle_runs_from_auto_through_the_levels_back_to_auto() {
        let l = levels(SMALL, Span::FRAME);
        assert_eq!(cycle_setting(&l, ZOOM_AUTO, auto_level(SMALL, Span::FRAME)), ZOOM_MIN);
        assert_eq!(cycle_setting(&l, ZOOM_MIN, ZOOM_MIN), l[1]);
        assert_eq!(cycle_setting(&l, l[2], l[2]), l[3]);
        assert_eq!(cycle_setting(&l, ZOOM_MAX, ZOOM_MAX), ZOOM_AUTO);
    }

    #[test]
    fn a_view_fills_the_screen_inside_the_frame() {
        let v = View::at(SMALL, 2.0, (0.0, 0.0), Span::FRAME);
        assert_eq!(v, View { x: 0.0, y: 0.0, w: 640.0, h: 480.0, k: 1.0 });
        let v = View::at(SMALL, 2.0, (GAME_W as f32, GAME_H as f32), Span::FRAME);
        assert_eq!((v.right(), v.y + v.h), (GAME_W as f32, GAME_H as f32));
        let whole = View::at(SMALL, ZOOM_MIN, (100.0, 100.0), Span::FRAME);
        assert_eq!((whole.x, whole.y, whole.w, whole.h), (0.0, 0.0, GAME_W as f32, GAME_H as f32));
        let b = Blit::new(View::at(SQUARE, 1.2, (640.0, 360.0), Span::FRAME), SQUARE);
        assert_eq!(b.viewport[2], SQUARE.0 as i32);
        assert!(b.viewport[1] > 0 && b.viewport[1] * 2 + b.viewport[3] == SQUARE.1 as i32);
    }

    #[test]
    fn the_camera_rests_on_a_frame_edge_that_holds_the_focus() {
        let frame = GAME_W as f32;
        let at = |center, size, x| follow_axis(center, size, x, x, (0.0, frame));
        // A square screen's view: the left half of the frame, the right, or between them.
        assert_eq!(at(640.0, 720.0, 232.0), 360.0);
        assert_eq!(at(360.0, 720.0, 1000.0), frame - 360.0);
        assert_eq!(at(360.0, 720.0, 640.0), 640.0 - 720.0 * (0.5 - EDGE));
        // Both edges hold it: the nearer one.
        assert_eq!(at(700.0, 960.0, 600.0), frame - 480.0);
        assert_eq!(follow_axis(500.0, 100.0, 0.0, 400.0, (0.0, frame)), 200.0);
    }

    #[test]
    fn a_screen_pixel_maps_back_to_the_frame_point_the_view_shows_there() {
        for (screen, level) in [(SMALL, 2.0), (SQUARE, 1.2), (WIDE, 1.5)] {
            let b = Blit::new(View::at(screen, level, (900.0, 500.0), Span::FRAME), screen);
            let [x, y, w, h] = b.viewport.map(|v| v as f32);
            assert_eq!(b.to_frame(x, y), (b.view.x, b.view.y));
            let (rx, by) = b.to_frame(x + w, y + h);
            assert!((rx - b.view.right()).abs() < 0.01 && (by - b.view.y - b.view.h).abs() < 0.01);
        }
    }

    #[test]
    fn a_window_shows_whole_and_centred() {
        let map = Rect { x: 340, y: 83, w: 752, h: 580 };
        let mut z = Zoom { level: 2.0, ..Zoom::default() };
        let mut view = None;
        for _ in 0..120 {
            view = z.advance(SQUARE, Scene::Window(map), [0.0; 4], ZOOM_MAX, DT);
        }
        let v = view.expect("a window larger than the whole frame shows");
        assert!(v.x <= map.x as f32 && v.right() >= (map.x + map.w) as f32);
        assert!(v.y <= map.y as f32 && v.y + v.h >= (map.y + map.h) as f32);
        assert_eq!(z.target, fit_level(SQUARE, map));
        // A window the size of the frame stays at the whole frame.
        assert_eq!(fit_level(WIDE, Rect { x: 0, y: 0, w: GAME_W, h: GAME_H }), ZOOM_MIN);
        assert_eq!(hull(map, [100.0, 300.0, 400.0, 700.0]), Rect { x: 100, y: 83, w: 992, h: 617 });
    }

    #[test]
    fn an_open_wheel_holds_the_level_down_to_show_it_whole() {
        let mut z = Zoom { level: ZOOM_MAX, ..Zoom::default() };
        let cursor = [640.0, 360.0, 640.0, 360.0];
        let mut view = None;
        for _ in 0..120 {
            view = z.advance(SMALL, Scene::Ship, cursor, wheel_level(SMALL), DT);
        }
        let v = view.expect("zoomed on the ship view");
        assert!(z.target < ZOOM_MAX && v.w >= WHEEL_SIZE && v.h >= WHEEL_SIZE);
        // A level that shows the wheel whole stays.
        let mut z = Zoom { level: 1.5, ..Zoom::default() };
        z.advance(WIDE, Scene::Ship, cursor, wheel_level(WIDE), DT);
        assert_eq!(z.target, 1.5);
    }

    #[test]
    fn a_pan_holds_until_the_focus_moves() {
        let mut z = Zoom { level: 2.0, ..Zoom::default() };
        let cursor = [300.0, 300.0, 300.0, 300.0];
        for _ in 0..120 {
            z.view = z.advance(SMALL, Scene::Ship, cursor, ZOOM_MAX, DT);
        }
        assert_eq!(z.goal.0, 320.0);
        z.goal.0 += 400.0;
        z.pin = Some(cursor);
        z.advance(SMALL, Scene::Ship, cursor, ZOOM_MAX, DT);
        assert_eq!(z.goal.0, 720.0);
        z.advance(SMALL, Scene::Ship, [310.0, 300.0, 310.0, 300.0], ZOOM_MAX, DT);
        assert_eq!((z.goal.0, z.pin), (320.0, None));
    }

    #[test]
    fn the_view_eases_to_the_cursor_and_back_out_off_the_ship() {
        let mut z = Zoom { level: 2.0, ..Zoom::default() };
        let cursor = [100.0, 600.0, 100.0, 600.0];
        let mut view = None;
        for _ in 0..120 {
            view = z.advance(SMALL, Scene::Ship, cursor, ZOOM_MAX, DT);
        }
        let v = view.expect("zoomed on the ship view");
        assert_eq!(v.k, 1.0);
        // The view rests on the frame edges nearest the cursor.
        assert_eq!((v.x, v.y), (0.0, GAME_H as f32 - v.h));
        for _ in 0..120 {
            view = z.advance(SMALL, Scene::Whole, cursor, ZOOM_MAX, DT);
        }
        assert_eq!((view, z.target), (None, ZOOM_MIN));
        assert_eq!(z.goal, v.center());
    }

    /// The Kestrel's HUD moved in on a narrow screen.
    const HUD: Span = Span { left: 20.0, right: 1169.0 };

    #[test]
    fn the_lowest_level_shows_the_hud_whole_and_auto_magnifies_as_before() {
        assert!(close(&levels(SMALL, HUD), &[1.114, 1.333, 2.0, 3.0]));
        assert!(close(&levels(SQUARE, HUD), &[1.114, 1.778, 2.667]));
        assert!(close(&[auto_level(SMALL, HUD), auto_level(SQUARE, HUD)], &[1.333, 1.778]));
        assert!(close(&[resolve(ZOOM_MIN, SMALL, HUD), resolve(2.0, SMALL, HUD)], &[1.114, 2.0]));
        let v = View::at(SMALL, resolve(ZOOM_MIN, SMALL, HUD), (0.0, 0.0), HUD);
        assert!((v.x - HUD.left).abs() < 1.0 && (v.right() - HUD.right).abs() < 1.0);
    }

    #[test]
    fn the_lowest_level_is_stored_as_the_whole_frame() {
        let l = levels(SMALL, HUD);
        assert_eq!(step(&l, l[1], false), ZOOM_MIN);
        assert_eq!(step_setting(&l, ZOOM_MIN, l[0], false), ZOOM_AUTO);
        assert_eq!(cycle_setting(&l, ZOOM_MIN, l[0]), l[1]);
    }

    #[test]
    fn a_peek_shows_the_hud_whole_or_the_whole_frame() {
        let mut z = Zoom { level: 2.0, span: HUD, ..Zoom::default() };
        let mut view = None;
        for _ in 0..120 {
            view = z.advance(SMALL, Scene::Span, [300.0; 4], ZOOM_MAX, DT);
        }
        let v = view.expect("the HUD's columns are narrower than the frame");
        assert!((v.x - HUD.left).abs() < 1.0 && (v.right() - HUD.right).abs() < 1.0);
        z.span = Span::FRAME;
        for _ in 0..120 {
            view = z.advance(SMALL, Scene::Span, [300.0; 4], ZOOM_MAX, DT);
        }
        assert_eq!(view, None);
    }

    #[test]
    fn a_window_is_enlarged_only_on_a_screen_that_shrinks_the_frame() {
        const DECK: (f32, f32) = (1280.0, 800.0);
        let map = Rect { x: 340, y: 83, w: 752, h: 580 };
        assert_eq!(window_scene(SMALL, map, ZOOM_AUTO, HUD), Scene::Window(map));
        assert_eq!(window_scene(SMALL, map, ZOOM_MIN, Span::FRAME), Scene::Span);
        for level in [ZOOM_AUTO, ZOOM_MIN, 2.0] {
            assert_eq!(window_scene(DECK, map, level, HUD), Scene::Span);
        }
        let wide = Rect { x: 0, w: GAME_W, ..map };
        assert_eq!(window_scene(DECK, wide, ZOOM_AUTO, HUD), Scene::Whole);
    }

    #[test]
    fn the_camera_rests_on_the_hud_edges() {
        let mut z = Zoom { level: 2.0, span: HUD, ..Zoom::default() };
        let mut view = None;
        for cursor in [[30.0, 300.0, 30.0, 300.0], [1160.0, 300.0, 1160.0, 300.0]] {
            for _ in 0..120 {
                view = z.advance(SMALL, Scene::Ship, cursor, ZOOM_MAX, DT);
            }
            let v = view.expect("zoomed on the ship view");
            assert!(v.x >= HUD.left - 1.0 && v.right() <= HUD.right + 1.0);
            assert!((v.x - HUD.left).abs() < 1.0 || (v.right() - HUD.right).abs() < 1.0);
        }
    }
}
