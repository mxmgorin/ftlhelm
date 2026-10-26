//! Drawing over the game's renderer: text through its fonts on its own std::string, panels,
//! discs, ring timers, game images by path, and the shared palette.

use std::collections::HashMap;
use std::ffi::c_int;

use crate::core::util::log;
use crate::game::api::{api, GlColor, Ptr};
use crate::game::string::GameString;

const DISC_SCANLINE: f32 = 4.0;
const BEVEL: f32 = 2.0;
pub const OVERLAY_LINE: f32 = 1.0;
/// The game's outlined header font, as on its own buttons.
pub const WHEEL_FONT: i32 = 62;
/// The height of a WHEEL_FONT line.
pub const WHEEL_FONT_H: f32 = 18.0;

/// The game's palette: dark teal panels, pale green frames, white text.
pub const PLATE_COLOR: GlColor = GlColor::new(0.05, 0.11, 0.1, 0.9);
pub const PLATE_EDGE_COLOR: GlColor = GlColor::new(0.63, 0.9, 0.76, 0.95);
const PLATE_BEVEL_COLOR: GlColor = GlColor::new(0.0, 0.05, 0.05, 0.9);
pub const TEXT_COLOR: GlColor = GlColor::new(1.0, 1.0, 1.0, 1.0);
pub const TEXT_DISABLED_COLOR: GlColor = GlColor::new(0.42, 0.46, 0.45, 1.0);
/// The game's lock timer around a system icon, faded: cyan segments on a dark teal disc.
pub const RING_LIT_COLOR: GlColor = GlColor::new(0.52, 0.91, 0.93, 0.7);
pub const RING_UNLIT_COLOR: GlColor = GlColor::new(0.52, 0.91, 0.93, 0.2);
pub const RING_TRACK_COLOR: GlColor = GlColor::new(0.05, 0.16, 0.13, 0.6);
pub const RING_READY_COLOR: GlColor = GlColor::new(1.0, 1.0, 1.0, 0.85);

/// A ring timer has as many segments as the game's, each a share of a turn less a gap.
pub const RING_SEGMENTS: usize = 12;
const TURN_DEG: f32 = 360.0;
const RING_GAP_DEG: f32 = 5.0;
/// The first segment starts straight up; angles run clockwise from the right.
const RING_START_DEG: f32 = -90.0;
/// The track shows this far past each side of the segments.
const RING_TRACK_PAD: f32 = 2.0;

pub fn draw_text_left(font: i32, x: f32, y: f32, text: &str) {
    let s = GameString::new(text);
    // SAFETY: the game's font code on a std::string it built.
    unsafe { (api().print_left)(font, x, y, s.as_ptr()) };
}

/// Prints text centred at x with its top at y.
pub fn draw_text_center(font: i32, x: f32, y: f32, text: &str) {
    let s = GameString::new(text);
    // SAFETY: as above.
    unsafe { (api().print_center)(font, x, y, s.as_ptr()) };
}

/// Prints a WHEEL_FONT_H line centred on (cx, cy), `k` times its size.
pub fn draw_text_scaled(font: i32, cx: f32, cy: f32, k: f32, text: &str) {
    with_scale(cx, cy, k, || draw_text_center(font, 0.0, -WHEEL_FONT_H / 2.0, text));
}

/// Runs `draw` with (cx, cy) as the origin and everything drawn `k` times its size.
pub fn with_scale(cx: f32, cy: f32, k: f32, draw: impl FnOnce()) {
    let a = api();
    // SAFETY: the renderer's matrix stack, popped below.
    unsafe {
        (a.push_matrix)();
        (a.translate)(cx, cy, 0.0);
        (a.scale)(k, k, 1.0);
    }
    draw();
    // SAFETY: pops the matrix pushed above.
    unsafe { (a.pop_matrix)() };
}

pub fn text_width(font: i32, text: &str) -> f32 {
    let s = GameString::new(text);
    // SAFETY: as above.
    unsafe { (api().measure_width)(font, s.as_ptr()) as f32 }
}

pub fn set_color(color: GlColor) {
    // SAFETY: the renderer's colour state.
    unsafe { (api().set_color)(color) }
}

pub fn draw_rect(x: f32, y: f32, w: f32, h: f32, color: GlColor) {
    // SAFETY: the renderer's rectangle fill.
    unsafe { (api().draw_rect)(x, y, w, h, color) }
}

pub fn draw_rect_outline(x: f32, y: f32, w: f32, h: f32, color: GlColor) {
    // SAFETY: the renderer's rectangle outline.
    unsafe { (api().draw_rect_outline)(x as i32, y as i32, w as i32, h as i32, color, OVERLAY_LINE) }
}

/// A panel in the game's style: a dark bevel around a framed fill.
pub fn draw_panel(x: f32, y: f32, w: f32, h: f32, fill: GlColor, edge: GlColor) {
    draw_rect(x - BEVEL, y - BEVEL, w + 2.0 * BEVEL, h + 2.0 * BEVEL, PLATE_BEVEL_COLOR);
    draw_rect(x, y, w, h, fill);
    draw_rect_outline(x, y, w, h, edge);
}

/// A filled disc out of scanlines: the game draws no curves.
pub fn draw_disc(cx: f32, cy: f32, r: f32, color: GlColor) {
    let mut y = -r;
    while y < r {
        let t = (y + DISC_SCANLINE / 2.0) / r;
        let half = r * (1.0 - t * t).max(0.0).sqrt();
        draw_rect(cx - half, cy + y, 2.0 * half, DISC_SCANLINE, color);
        y += DISC_SCANLINE;
    }
}

/// An arc `width` wide inside radius `r`, from `from` to `to` degrees.
fn draw_arc(cx: f32, cy: f32, r: f32, width: f32, (from, to): (f32, f32), color: GlColor) {
    let a = api();
    // SAFETY: the renderer's pie primitive, rendered once and freed.
    unsafe {
        let pie = (a.pie_partial)(cx.round() as c_int, cy.round() as c_int, r, from, to, width, color);
        if !pie.is_null() {
            (a.render_primitive)(pie);
            (a.destroy_primitive)(pie);
        }
    }
}

/// `color` with its alpha scaled by `opacity`.
fn faded(color: GlColor, opacity: f32) -> GlColor {
    GlColor { a: color.a * opacity, ..color }
}

/// A ring timer in the game's style, `width` wide inside radius `r`: RING_SEGMENTS segments
/// clockwise from the top over a dark track, the first `lit` in `color`; all of it at
/// `opacity`.
pub fn draw_ring_timer(cx: f32, cy: f32, r: f32, width: f32, lit: usize, color: GlColor, opacity: f32) {
    let whole = (RING_START_DEG, RING_START_DEG + TURN_DEG);
    let track = faded(RING_TRACK_COLOR, opacity);
    draw_arc(cx, cy, r + RING_TRACK_PAD, width + 2.0 * RING_TRACK_PAD, whole, track);
    let step = TURN_DEG / RING_SEGMENTS as f32;
    for i in 0..RING_SEGMENTS {
        let from = RING_START_DEG + i as f32 * step + RING_GAP_DEG / 2.0;
        let shade = faded(if i < lit { color } else { RING_UNLIT_COLOR }, opacity);
        draw_arc(cx, cy, r, width, (from, from + step - RING_GAP_DEG), shade);
    }
}

/// Game images looked up by path under img/, misses included.
#[derive(Default)]
pub struct Images {
    cache: HashMap<String, usize>,
}

impl Images {
    /// The texture of a game image, or None when the game has none.
    pub fn get(&mut self, path: &str) -> Option<Ptr> {
        let texture = *self.cache.entry(path.to_string()).or_insert_with(|| {
            let s = GameString::new(path);
            let a = api();
            // SAFETY: ResourceControl::GetImageId on the global resources and a std::string.
            let texture = unsafe { (a.image_id)(a.resources as Ptr, s.as_ptr()) } as usize;
            if texture == 0 {
                log!("image missing: {path}");
            }
            texture
        });
        (texture != 0).then_some(texture as Ptr)
    }

    /// Caches `texture` for `path` as a lookup would; 0 caches a miss.
    #[cfg(test)]
    pub fn insert(&mut self, path: &str, texture: usize) {
        self.cache.insert(path.to_string(), texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{live_strings, with_game};

    #[test]
    fn text_reaches_the_game_as_its_own_string_released_after() {
        with_game(|_| {
            assert_eq!(text_width(WHEEL_FONT, "Shields"), "Shields".len() as f32);
            assert_eq!(text_width(WHEEL_FONT, "a\0b"), "ab".len() as f32);
            assert_eq!(live_strings(), 0);
        });
    }
}
