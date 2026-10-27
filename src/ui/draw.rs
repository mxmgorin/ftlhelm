//! Drawing over the game's renderer: text through its fonts on its own std::string, frames in
//! the game's style, discs, ring timers, game images by path, and the shared palette.
//!
//! A frame is rings of cut rects: a glow ring per pixel out, the band, a line, then the fill;
//! none overlaps another, so a see-through fill shows nothing of them. Each ring is built once
//! as a game primitive at the origin and drawn moved and tinted.

use std::collections::{BTreeMap, HashMap};
use std::f32::consts::SQRT_2;
use std::ffi::c_int;
use std::sync::Mutex;

use crate::core::util::log;
use crate::game::api::{api, CppVector, GlColor, Ptr, Rect};
use crate::game::string::GameString;

pub const OVERLAY_LINE: f32 = 1.0;
/// The game's outlined header font, as on its own buttons.
pub const WHEEL_FONT: i32 = 62;
/// The height of a WHEEL_FONT line.
pub const WHEEL_FONT_H: f32 = 18.0;

/// Marks over the game: pale green on a dark edge.
pub const PLATE_EDGE_COLOR: GlColor = GlColor::new(0.63, 0.9, 0.76, 0.95);
const PLATE_BEVEL_COLOR: GlColor = GlColor::new(0.0, 0.05, 0.05, 0.9);
pub const TEXT_COLOR: GlColor = GlColor::new(1.0, 1.0, 1.0, 1.0);
pub const TEXT_DISABLED_COLOR: GlColor = GlColor::new(0.42, 0.46, 0.45, 1.0);
/// The game's lock timer around a system icon, faded: cyan segments on a dark teal disc.
pub const RING_LIT_COLOR: GlColor = GlColor::new(0.52, 0.91, 0.93, 0.7);
pub const RING_UNLIT_COLOR: GlColor = GlColor::new(0.52, 0.91, 0.93, 0.2);
pub const RING_TRACK_COLOR: GlColor = GlColor::new(0.05, 0.16, 0.13, 0.6);
pub const RING_READY_COLOR: GlColor = GlColor::new(1.0, 1.0, 1.0, 0.85);
/// The green of the game's frame around a selected crew member's box.
pub const CREW_SELECTED_COLOR: GlColor = GlColor::new(0.47, 1.0, 0.47, 1.0);
/// The game's buttons, labels and panels: pale, yellow while selected, grey while off, with
/// slate lines and text; a cyan glow fading out past a pale band; all with cut corners.
pub const BUTTON_PALE_COLOR: GlColor = GlColor::new(0.92, 0.96, 0.9, 1.0);
pub const INK_COLOR: GlColor = GlColor::new(0.21, 0.29, 0.35, 1.0);
pub const BUTTON_SELECTED_COLOR: GlColor = GlColor::new(1.0, 0.9, 0.37, 1.0);
pub const BUTTON_OFF_COLOR: GlColor = GlColor::new(0.64, 0.67, 0.63, 1.0);
pub const FRAME_GLOW_COLOR: GlColor = GlColor::new(0.47, 0.95, 0.99, 1.0);
/// The header strip of the game's info cards, under white text.
pub const HEADER_COLOR: GlColor = GlColor::new(0.49, 0.41, 0.4, 1.0);
/// The glow's alpha per pixel out from the band.
const FRAME_GLOW: [f32; 7] = [0.345, 0.259, 0.184, 0.122, 0.075, 0.039, 0.012];
/// Rows of cut a frame's layer loses per pixel inward: a band stays as thick across its cut
/// as along its sides.
const CUT_PER_INSET: f32 = 2.0 - SQRT_2;
/// Ring primitives kept; past it they are all freed and built anew.
const RINGS_MAX: usize = 256;
const WHITE: GlColor = GlColor::new(1.0, 1.0, 1.0, 1.0);

/// A frame's look: its corners cut `cut` rows deep, a `band` wide band, a `line` wide slate
/// line inside it, none when 0, and a glow when given.
pub struct FrameStyle {
    pub cut: i32,
    pub band: i32,
    pub band_color: GlColor,
    pub line: i32,
    pub glow: Option<GlColor>,
}

/// The game's top-bar button.
pub const BUTTON_FRAME: FrameStyle =
    FrameStyle { cut: 6, band: 3, band_color: BUTTON_PALE_COLOR, line: 2, glow: Some(FRAME_GLOW_COLOR) };
/// The game's resource boxes in the top bar.
pub const BOX_FRAME: FrameStyle = FrameStyle { line: 0, ..BUTTON_FRAME };
/// The game's event window.
pub const WINDOW_FRAME: FrameStyle = FrameStyle { cut: 12, band: 4, ..BOX_FRAME };
/// The header strip of an info card.
pub const HEADER_FRAME: FrameStyle = FrameStyle { cut: 4, band: 0, band_color: HEADER_COLOR, line: 0, glow: None };

/// A cut rect at the origin, less the cut rect `hole` = (inset, cut) inside it when given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Ring {
    w: i32,
    h: i32,
    cut: i32,
    hole: Option<(i32, i32)>,
}

/// Ring primitives by shape, as addresses.
static RINGS: Mutex<BTreeMap<Ring, usize>> = Mutex::new(BTreeMap::new());

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

/// A filled disc: a pie as wide as its radius.
pub fn draw_disc(cx: f32, cy: f32, r: f32, color: GlColor) {
    draw_arc(cx, cy, r, r, (0.0, TURN_DEG), color);
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

/// Corner brackets `thick` wide and `arm` long on the corners of `x, y, w, h`, each over a dark
/// edge one line wider.
pub fn draw_brackets(x: f32, y: f32, w: f32, h: f32, arm: f32, thick: f32, color: GlColor) {
    let (right, bottom) = (x + w, y + h);
    let bars = [
        (x, y, arm, thick),
        (x, y, thick, arm),
        (right - arm, y, arm, thick),
        (right - thick, y, thick, arm),
        (x, bottom - thick, arm, thick),
        (x, bottom - arm, thick, arm),
        (right - arm, bottom - thick, arm, thick),
        (right - thick, bottom - arm, thick, arm),
    ];
    let e = OVERLAY_LINE;
    for (bx, by, bw, bh) in bars {
        draw_rect(bx - e, by - e, bw + 2.0 * e, bh + 2.0 * e, PLATE_BEVEL_COLOR);
    }
    for (bx, by, bw, bh) in bars {
        draw_rect(bx, by, bw, bh, color);
    }
}

/// A triangle pointing down, `w` wide and `h` tall, its tip at (`cx`, `tip`), over a dark edge
/// one line wider; out of one-line rows like a disc.
pub fn draw_down_marker(cx: f32, tip: f32, w: f32, h: f32, color: GlColor) {
    for (grow, shade) in [(OVERLAY_LINE, PLATE_BEVEL_COLOR), (0.0, color)] {
        let (w, h) = (w + 2.0 * grow, h + 2.0 * grow);
        let top = tip + grow - h;
        let mut row = 0.0;
        while row < h {
            let half = w / 2.0 * (1.0 - row / h);
            draw_rect(cx - half, top + row, 2.0 * half, OVERLAY_LINE, shade);
            row += OVERLAY_LINE;
        }
    }
}

/// The cut of a frame's layer `inset` pixels inside its band's outer edge, outside when
/// negative, for a band cut `cut` rows deep.
fn cut_at(cut: i32, inset: i32) -> i32 {
    (cut as f32 - inset as f32 * CUT_PER_INSET).round() as i32
}

/// How far row `y` of an `h` row rect with corners cut `cut` rows deep starts in from its sides.
fn row_inset(y: i32, h: i32, cut: i32) -> i32 {
    (cut - y.min(h - 1 - y)).max(0)
}

/// The rects that cover a ring, row by row, each run of rows with the same spans in one rect.
fn ring_rects(r: Ring) -> Vec<Rect> {
    let spans = |y: i32| {
        let lo = row_inset(y, r.h, r.cut);
        match r.hole {
            Some((t, cut)) if y >= t && y < r.h - t => {
                let hi = t + row_inset(y - t, r.h - 2 * t, cut);
                [(lo, hi), (r.w - hi, r.w - lo)]
            }
            _ => [(lo, r.w - lo), (0, 0)],
        }
    };
    let mut rects = Vec::new();
    let (mut start, mut run) = (0, spans(0));
    for y in 1..=r.h {
        let next = (y < r.h).then(|| spans(y));
        if next == Some(run) {
            continue;
        }
        for (a, b) in run.into_iter().filter(|(a, b)| b > a) {
            rects.push(Rect { x: a, y: start, w: b - a, h: y - start });
        }
        if let Some(next) = next {
            (start, run) = (y, next);
        }
    }
    rects
}

/// The game primitive of a ring, built white on first use; None when the game builds none.
fn ring_primitive(r: Ring) -> Option<Ptr> {
    let mut rings = RINGS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&p) = rings.get(&r) {
        return Some(p as Ptr);
    }
    let a = api();
    if rings.len() >= RINGS_MAX {
        for (_, p) in std::mem::take(&mut *rings) {
            // SAFETY: a primitive built below and drawn no more.
            unsafe { (a.destroy_primitive)(p as Ptr) };
        }
    }
    let rects = ring_rects(r);
    // SAFETY: the renderer reads the rects, alive for the call, into a primitive of its own.
    let p = unsafe { (a.multi_rect)(&CppVector::of(&rects), WHITE) };
    if p.is_null() {
        return None;
    }
    rings.insert(r, p as usize);
    Some(p)
}

/// Draws ring `r` with its top left corner at (`x`, `y`).
fn draw_ring(x: i32, y: i32, r: Ring, color: GlColor) {
    if r.w <= 0 || r.h <= 0 || color.a <= 0.0 {
        return;
    }
    let Some(p) = ring_primitive(r) else {
        return;
    };
    let a = api();
    // SAFETY: the renderer's matrix stack around one of its primitives, popped below.
    unsafe {
        (a.push_matrix)();
        (a.translate)(x as f32, y as f32, 0.0);
        (a.render_primitive_color)(p, color);
        (a.pop_matrix)();
    }
}

/// Draws a frame of `style` with its band's outer edge at `x, y, w, h`, on whole pixels, filled
/// with `fill`; the glow reaches FRAME_GLOW's length past it.
pub fn draw_frame(x: f32, y: f32, w: f32, h: f32, style: &FrameStyle, fill: GlColor) {
    let [x, y, w, h] = [x, y, w, h].map(|v| v.round() as i32);
    // The layer from `inset` pixels in, `width` wide or all the rest.
    let layer = |inset: i32, width: Option<i32>, color: GlColor| {
        let r = Ring {
            w: w - 2 * inset,
            h: h - 2 * inset,
            cut: cut_at(style.cut, inset),
            hole: width.map(|t| (t, cut_at(style.cut, inset + t))),
        };
        draw_ring(x + inset, y + inset, r, color);
    };
    if let Some(glow) = style.glow {
        for (i, &alpha) in FRAME_GLOW.iter().enumerate() {
            layer(-(i as i32) - 1, Some(1), GlColor { a: glow.a * alpha, ..glow });
        }
    }
    if style.band > 0 {
        layer(0, Some(style.band), style.band_color);
    }
    if style.line > 0 {
        layer(style.band, Some(style.line), INK_COLOR);
    }
    layer(style.band + style.line, None, fill);
    // The game's draws leave the engine's fixed colour white, which untinted text relies on.
    // SAFETY: the engine's colour state, set from a colour alive for the call.
    unsafe { (api().set_fixed_color)(&WHITE) };
}

/// The tint that turns the pale of the game's button art into `fill`.
pub fn art_tint(fill: GlColor) -> GlColor {
    let p = BUTTON_PALE_COLOR;
    GlColor::new((fill.r / p.r).min(1.0), (fill.g / p.g).min(1.0), (fill.b / p.b).min(1.0), 1.0)
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
    use std::collections::BTreeSet;

    /// The pixels a ring's rects cover, each once.
    fn pixels(r: Ring) -> BTreeSet<(i32, i32)> {
        let mut covered = BTreeSet::new();
        for rect in ring_rects(r) {
            for (x, y) in (rect.x..rect.x + rect.w).flat_map(|x| (rect.y..rect.y + rect.h).map(move |y| (x, y))) {
                assert!(covered.insert((x, y)), "{r:?} covers {x},{y} twice");
            }
        }
        covered
    }

    #[test]
    fn a_frame_takes_the_cuts_of_the_games_button_art() {
        assert_eq!(cut_at(6, 3), 4, "the slate line");
        assert_eq!(cut_at(6, 5), 3, "the fill");
        assert_eq!(cut_at(6, -7), 10, "the glow's outer ring");
    }

    #[test]
    fn a_ring_and_its_hole_tile_the_whole_and_mirror_both_ways() {
        for (w, h, cut, t) in [(60, 60, 6, 2), (68, 30, 6, 3), (120, 40, 12, 4), (20, 16, 7, 1)] {
            let hole_cut = cut_at(cut, t);
            let ring = pixels(Ring { w, h, cut, hole: Some((t, hole_cut)) });
            let hole: BTreeSet<_> = pixels(Ring { w: w - 2 * t, h: h - 2 * t, cut: hole_cut, hole: None })
                .into_iter()
                .map(|(x, y)| (x + t, y + t))
                .collect();
            assert!(ring.is_disjoint(&hole));
            assert_eq!(&ring | &hole, pixels(Ring { w, h, cut, hole: None }));
            assert_eq!(ring.iter().map(|&(x, y)| (w - 1 - x, y)).collect::<BTreeSet<_>>(), ring);
            assert_eq!(ring.iter().map(|&(x, y)| (x, h - 1 - y)).collect::<BTreeSet<_>>(), ring);
        }
    }

    #[test]
    fn a_band_is_as_thick_across_its_cut_as_along_its_sides() {
        for (cut, t) in [(6, 2), (6, 3), (12, 4)] {
            let side = 60;
            let hole_cut = cut_at(cut, t);
            let band = pixels(Ring { w: side, h: side, cut, hole: Some((t, hole_cut)) });
            let run = |y: i32| band.iter().filter(|&&(x, row)| row == y && x < side / 2).count() as f32;
            assert_eq!(run(side / 2), t as f32, "along a side");
            // Rows where both of the band's edges run down the cut.
            for y in t..cut.min(hole_cut + t) {
                assert!((run(y) - t as f32 * SQRT_2).abs() < 1.0, "cut {cut}, band {t}, row {y}: {}", run(y));
            }
        }
    }

    #[test]
    fn text_reaches_the_game_as_its_own_string_released_after() {
        with_game(|_| {
            assert_eq!(text_width(WHEEL_FONT, "Shields"), "Shields".len() as f32);
            assert_eq!(text_width(WHEEL_FONT, "a\0b"), "ab".len() as f32);
            assert_eq!(live_strings(), 0);
        });
    }
}
