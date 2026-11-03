//! Drawing over the game's renderer: text through its fonts on its own std::string, plain or
//! in the look of the game's pause banners, frames in the game's style, discs, ring timers,
//! game images by path, and the shared palette.
//!
//! A frame is rings of cut rects: a glow ring per pixel out, the band, a line, then the fill;
//! none overlaps another, so a see-through fill shows nothing of them. Each ring is built once
//! as a game primitive at the origin and drawn moved and tinted.
//!
//! Marks (corner brackets, the down marker) are layers of rects the same way, one primitive
//! per layer with all four corners of a bracket, built at its size on screen: the band, a dark
//! edge around it, then a glow ring per pixel out, none overlapping another.
//!
//! Pixel-art shapes built of many small rects (glyphs, pip rows) are drawn in a batch: the
//! rects are collected, snapped to the pixels the renderer would fill, and each run of one
//! colour is drawn as one cached primitive. gl4es misplaces vertices of long series of the
//! renderer's one-off rect draws.
//!
//! Arcs (discs, round frames, ring timers) are cached too, each pie built around the origin.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::f32::consts::SQRT_2;
use std::sync::Mutex;

use crate::core::util::log;
use crate::game::api::{api, CppVector, GlColor, Ptr, Rect};
use crate::game::ipad_art;
use crate::game::mem::Obj;
use crate::game::offsets::{TEXTURE_H, TEXTURE_W};
use crate::game::string::GameString;

pub const OVERLAY_LINE: f32 = 1.0;
/// The game's outlined header font, as on its own buttons.
pub const WHEEL_FONT: i32 = 62;
/// The height of a WHEEL_FONT line.
pub const WHEEL_FONT_H: f32 = 18.0;
/// How far the outlined capitals of a WHEEL_FONT line centre below the line's middle.
pub const WHEEL_FONT_CAPS_DY: f32 = 1.0;

/// The dark edge of marks over the game.
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
/// Primitives the shape cache holds; past it, those drawn longest ago are freed.
const SHAPES_MAX: usize = 512;
/// Primitives freed at once past SHAPES_MAX.
const SHAPES_EVICT: usize = SHAPES_MAX / 4;
const WHITE: GlColor = GlColor::new(1.0, 1.0, 1.0, 1.0);
/// The dark edge of the game's pause banners' letters.
const BANNER_EDGE_COLOR: GlColor = GlColor::new(0.2, 0.33, 0.37, 0.75);
/// The rings of a banner-look glow, outermost first: offset and alpha.
const GLOW_RINGS: [(f32, f32); 2] = [(3.0, 0.2), (2.0, 0.45)];
/// How far a banner-look glow reaches past its letters.
pub const GLOW_REACH: f32 = GLOW_RINGS[0].0;
/// Unit offsets a glow ring is printed at.
const RING_STEPS: [(f32, f32); 8] =
    [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
/// Unit offsets the dark edge is printed at.
const EDGE_STEPS: [(f32, f32); 4] = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)];

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

/// Corner brackets: `thick` wide arms `arm` long from the corner, which is cut `cut` rows deep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bracket {
    pub arm: i32,
    pub thick: i32,
    pub cut: i32,
}

/// A triangle pointing down, `w` wide and `h` tall.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Marker {
    pub w: i32,
    pub h: i32,
}

/// A band in a mark's colour over a dark edge, in a glow of its colour: a bracket's corner with
/// its point at the origin and its arms to the right and down, or a marker in the box from the
/// origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Mark {
    Corner(Bracket),
    Down(Marker),
}

/// One layer of a mark in whole pixels from its point, `k` times its size as f32 bits: the
/// band, the edge, then each glow ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MarkLayer {
    mark: Mark,
    copies: Copies,
    k: u32,
    layer: usize,
}

/// Where a mark's copies go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Copies {
    /// One at the point, mirrored across the axes as asked.
    One { flip_x: bool, flip_y: bool },
    /// A bracket's four corners on a `w` by `h` box from the point.
    Corners { w: i32, h: i32 },
}

impl Copies {
    /// Each copy's point and its mirroring.
    fn placed(self) -> Vec<(i32, i32, bool, bool)> {
        match self {
            Copies::One { flip_x, flip_y } => vec![(0, 0, flip_x, flip_y)],
            Copies::Corners { w, h } => {
                vec![(0, 0, false, false), (w, 0, true, false), (0, h, false, true), (w, h, true, true)]
            }
        }
    }
}

const MARK_BAND: usize = 0;
const MARK_EDGE: usize = 1;
const MARK_GLOW: usize = 2;
const MARK_LAYERS: usize = MARK_GLOW + FRAME_GLOW.len();
/// How far a mark's layers reach past its band: the edge's line, then the glow.
const MARK_REACH: i32 = OVERLAY_LINE as i32 + FRAME_GLOW.len() as i32;

/// What a cached primitive draws.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Shape {
    Ring(Ring),
    Mark(MarkLayer),
    /// Pixel rects from the origin.
    Rects(Vec<PixelRect>),
    /// Rings placed from the origin.
    Rings(Vec<PlacedRing>),
    Pie(Pie),
}

/// A pie around the origin as the renderer builds it: radius, width, start and end degrees,
/// as f32 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pie {
    r: u32,
    width: u32,
    from: u32,
    end: u32,
}

/// A rect on whole pixels: x, y, w, h.
type PixelRect = [i32; 4];
/// A ring and its top left corner: x, y, ring.
type PlacedRing = (i32, i32, Ring);
/// Pixel rects and their colours, in drawing order.
type Batch = Vec<(PixelRect, GlColor)>;

/// Cached primitives by shape: each one's address and the use count when last drawn.
struct Shapes {
    cached: BTreeMap<Shape, (usize, u64)>,
    uses: u64,
}

static SHAPES: Mutex<Shapes> = Mutex::new(Shapes { cached: BTreeMap::new(), uses: 0 });
thread_local! {
    /// The batch being collected; None while none is. Per thread: the game draws on one, tests on many.
    static BATCH: RefCell<Option<Batch>> = const { RefCell::new(None) };
}

/// A ring timer has as many segments as the game's, each a share of a turn less a gap.
pub const RING_SEGMENTS: usize = 12;
const TURN_DEG: f32 = 360.0;
/// The renderer's pies put a point every PIE_STEP_DEG from the start while short of the end,
/// none at the end itself.
pub const PIE_STEP_DEG: f32 = 10.0;
const RING_GAP_DEG: f32 = 10.0;
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

/// Prints text centred at x with its top at y in the look of the game's pause banners: pale
/// over a dark edge, in a `glow` halo.
pub fn draw_text_glow(font: i32, x: f32, y: f32, text: &str, glow: GlColor) {
    let s = GameString::new(text);
    // SAFETY: the game's centred print of a string alive for the call.
    let print = |x: f32, y: f32| unsafe { (api().print_center)(font, x, y, s.as_ptr()) };
    for (r, a) in GLOW_RINGS {
        set_color(GlColor { a, ..glow });
        for (dx, dy) in RING_STEPS {
            print(x + dx * r, y + dy * r);
        }
    }
    set_color(BANNER_EDGE_COLOR);
    for (dx, dy) in EDGE_STEPS {
        print(x + dx, y + dy);
    }
    set_color(BUTTON_PALE_COLOR);
    print(x, y);
    set_color(TEXT_COLOR);
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

/// Fills `x, y, w, h` with `color`, or adds it to the batch being collected.
pub fn draw_rect(x: f32, y: f32, w: f32, h: f32, color: GlColor) {
    let batched =
        BATCH.with_borrow_mut(|batch| batch.as_mut().map(|b| b.extend(pixel_rect(x, y, w, h).map(|r| (r, color)))));
    if batched.is_some() {
        return;
    }
    // SAFETY: the renderer's rectangle fill.
    unsafe { (api().draw_rect)(x, y, w, h, color) }
}

/// The pixels the renderer fills for `x, y, w, h`; None when it fills none.
fn pixel_rect(x: f32, y: f32, w: f32, h: f32) -> Option<PixelRect> {
    // A pixel is filled when its centre lies inside, the far edges excluded.
    let span = |from: f32, len: f32| {
        let (lo, hi) = (from.min(from + len), from.max(from + len));
        ((lo - 0.5).ceil() as i32, (hi - 0.5).ceil() as i32)
    };
    let ((x0, x1), (y0, y1)) = (span(x, w), span(y, h));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

/// The rects `draw` fills and their colours, in drawing order, drawn nowhere.
#[cfg(test)]
pub fn rects_of(draw: impl FnOnce()) -> Vec<([i32; 4], GlColor)> {
    BATCH.set(Some(Vec::new()));
    draw();
    BATCH.take().unwrap_or_default()
}

/// Runs `draw` collecting the rects it fills, then draws each run of one colour as one cached
/// primitive; the rects land over anything else `draw` draws. Inside a batch, adds to it.
pub fn draw_batched(draw: impl FnOnce()) {
    if BATCH.with_borrow(Option::is_some) {
        draw();
        return;
    }
    BATCH.set(Some(Vec::new()));
    draw();
    let rects = BATCH.take().unwrap_or_default();
    for run in rects.chunk_by(|a, b| a.1 == b.1) {
        let (x, y) = run.iter().fold((i32::MAX, i32::MAX), |(x, y), (r, _)| (x.min(r[0]), y.min(r[1])));
        let local = run.iter().map(|&([rx, ry, w, h], _)| [rx - x, ry - y, w, h]).collect();
        draw_shape(x as f32, y as f32, 1.0, Shape::Rects(local), run[0].1);
    }
    reset_fixed_color();
}

/// The outline of `x, y, w, h`, `thick` wide inside it, out of bars that do not overlap.
pub fn draw_outline(x: f32, y: f32, w: f32, h: f32, thick: f32, color: GlColor) {
    draw_rect(x, y, w, thick, color);
    draw_rect(x, y + h - thick, w, thick, color);
    draw_rect(x, y + thick, thick, h - 2.0 * thick, color);
    draw_rect(x + w - thick, y + thick, thick, h - 2.0 * thick, color);
}

/// A circle in a frame's style, its band's outer edge at radius `r`: the glow past it, a
/// `band` wide pale ring, then the fill.
pub fn draw_round_frame(cx: f32, cy: f32, r: f32, band: f32, fill: GlColor) {
    for (i, &alpha) in FRAME_GLOW.iter().enumerate() {
        draw_circle(cx, cy, r + (i + 1) as f32, OVERLAY_LINE, faded(FRAME_GLOW_COLOR, alpha));
    }
    draw_circle(cx, cy, r, band, BUTTON_PALE_COLOR);
    draw_disc(cx, cy, r - band, fill);
}

/// A ring `width` wide inside radius `r`.
fn draw_circle(cx: f32, cy: f32, r: f32, width: f32, color: GlColor) {
    draw_arc(cx, cy, r, width, (0.0, TURN_DEG), color);
}

/// A filled disc: a pie as wide as its radius.
pub fn draw_disc(cx: f32, cy: f32, r: f32, color: GlColor) {
    draw_arc(cx, cy, r, r, (0.0, TURN_DEG), color);
    reset_fixed_color();
}

/// An arc `width` wide inside radius `r`, from `from` to `to` degrees, centred on whole pixels.
/// The span must be whole PIE_STEP_DEG steps: the renderer's pies draw no part of a step.
pub fn draw_arc(cx: f32, cy: f32, r: f32, width: f32, (from, to): (f32, f32), color: GlColor) {
    // Half a step past the end, so the pie puts its last point on it.
    let end = to + PIE_STEP_DEG / 2.0;
    let pie = Pie { r: r.to_bits(), width: width.to_bits(), from: from.to_bits(), end: end.to_bits() };
    draw_shape(cx.round(), cy.round(), 1.0, Shape::Pie(pie), color);
}

/// Corner brackets in the game's style on the corners of box `x, y, w, h`, `k` times their
/// size: `color` arms with cut corners over a dark edge, in a glow of their colour.
/// Brackets `moving` go corner by corner: a box changing size builds nothing new.
pub fn draw_brackets((x, y, w, h): (f32, f32, f32, f32), bracket: Bracket, k: f32, moving: bool, color: GlColor) {
    let (left, top, right, bottom) = (x.round(), y.round(), (x + w).round(), (y + h).round());
    let mark = Mark::Corner(bracket);
    if moving {
        let corner = |x, y, flip_x, flip_y| (x, y, Copies::One { flip_x, flip_y });
        let corners = [
            corner(left, top, false, false),
            corner(right, top, true, false),
            corner(left, bottom, false, true),
            corner(right, bottom, true, true),
        ];
        draw_mark(&corners, mark, k, color);
    } else {
        let copies = Copies::Corners { w: (right - left) as i32, h: (bottom - top) as i32 };
        draw_mark(&[(left, top, copies)], mark, k, color);
    }
}

/// A triangle pointing down in the game's style, its tip at (`cx`, `tip`), `k` times its size:
/// `color` over a dark edge, in a glow of its colour.
pub fn draw_marker(cx: f32, tip: f32, marker: Marker, k: f32, color: GlColor) {
    let (x, y) = ((cx - marker.w as f32 * k / 2.0).round(), (tip - marker.h as f32 * k).round());
    let copies = Copies::One { flip_x: false, flip_y: false };
    draw_mark(&[(x, y, copies)], Mark::Down(marker), k, color);
}

/// Draws `mark` at each point on whole pixels with its copies, `k` times its size, its band in
/// `color`.
fn draw_mark(at: &[(f32, f32, Copies)], mark: Mark, k: f32, color: GlColor) {
    let colors = [MARK_BAND, MARK_EDGE].into_iter().zip([color, PLATE_BEVEL_COLOR]);
    let glow = FRAME_GLOW.iter().enumerate().map(|(i, &alpha)| (MARK_GLOW + i, faded(color, alpha)));
    for (layer, shade) in colors.chain(glow) {
        for &(x, y, copies) in at {
            draw_shape(x, y, 1.0, Shape::Mark(MarkLayer { mark, copies, k: k.to_bits(), layer }), shade);
        }
    }
    reset_fixed_color();
}

/// Corner brackets `thick` wide and `arm` long on the corners of `x, y, w, h`.
pub fn draw_corners(x: f32, y: f32, w: f32, h: f32, arm: f32, thick: f32, color: GlColor) {
    for (bx, by, bw, bh) in bracket_bars(x, y, w, h, arm, thick) {
        draw_rect(bx, by, bw, bh, color);
    }
}

/// The bars of corner brackets on `x, y, w, h`: two per corner.
fn bracket_bars(x: f32, y: f32, w: f32, h: f32, arm: f32, thick: f32) -> [(f32, f32, f32, f32); 8] {
    let (right, bottom) = (x + w, y + h);
    [
        (x, y, arm, thick),
        (x, y, thick, arm),
        (right - arm, y, arm, thick),
        (right - thick, y, thick, arm),
        (x, bottom - thick, arm, thick),
        (x, bottom - arm, thick, arm),
        (right - arm, bottom - thick, arm, thick),
        (right - thick, bottom - arm, thick, arm),
    ]
}

/// A ring `thick` wide inside radius `r` around (`cx`, `cy`), out of one-line rows like a disc;
/// a centre on whole pixels keeps it symmetric.
pub fn draw_pixel_ring(cx: f32, cy: f32, r: f32, thick: f32, color: GlColor) {
    let hole = r - thick;
    let half_width = |radius: f32, dy: f32| (radius * radius - dy * dy).max(0.0).sqrt().round();
    let mut top = -r;
    while top < r {
        let dy = top + OVERLAY_LINE / 2.0;
        let outer = half_width(r, dy);
        let inner = if dy.abs() < hole { half_width(hole, dy) } else { 0.0 };
        draw_rect(cx - outer, cy + top, outer - inner, OVERLAY_LINE, color);
        draw_rect(cx + inner, cy + top, outer - inner, OVERLAY_LINE, color);
        top += OVERLAY_LINE;
    }
}

/// A triangle pointing sideways, `h` tall, its base at `x` centred on `cy` and its tip `w` to the
/// right, or to the left for a negative `w`; out of one-line rows like a disc.
pub fn draw_side_triangle(x: f32, cy: f32, w: f32, h: f32, color: GlColor) {
    let half = h / 2.0;
    let mut top = -half;
    while top < half {
        let len = triangle_row(w, half, top);
        draw_rect(if w < 0.0 { x - len } else { x }, cy + top, len, OVERLAY_LINE, color);
        top += OVERLAY_LINE;
    }
}

/// The length of the row `top` below the middle of a sideways triangle `w` long and `2 * half`
/// tall; returns 0 outside it.
fn triangle_row(w: f32, half: f32, top: f32) -> f32 {
    let dy = top + OVERLAY_LINE / 2.0;
    (w.abs() * (1.0 - dy.abs() / half)).round().max(0.0)
}

/// A diamond `w` wide and `h` tall centred on (`cx`, `cy`): two sideways triangles base to base.
pub fn draw_diamond(cx: f32, cy: f32, w: f32, h: f32, color: GlColor) {
    draw_side_triangle(cx, cy, w / 2.0, h, color);
    draw_side_triangle(cx, cy, -w / 2.0, h, color);
}

/// The lower edge of the diamond `w` wide and `h` tall centred on (`cx`, `cy`), `thick` lines
/// deep: a chevron pointing down, on the diamond's rows.
pub fn draw_chevron(cx: f32, cy: f32, w: f32, h: f32, thick: f32, color: GlColor) {
    let half = h / 2.0;
    let mut top = -half;
    while top < half {
        // The diamond's row less the same diamond's row `thick` lines higher up.
        let outer = triangle_row(w / 2.0, half, top);
        let inner = if top + thick < half { triangle_row(w / 2.0, half, top + thick) } else { 0.0 };
        if outer > inner {
            draw_rect(cx - outer, cy + top, outer - inner, OVERLAY_LINE, color);
            draw_rect(cx + inner, cy + top, outer - inner, OVERLAY_LINE, color);
        }
        top += OVERLAY_LINE;
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

impl Mark {
    /// The size of the box from the origin that holds the band.
    fn size(self) -> (i32, i32) {
        match self {
            Mark::Corner(b) => (b.arm, b.arm),
            Mark::Down(m) => (m.w, m.h),
        }
    }

    /// Whether the band covers pixel (`x`, `y`).
    fn covers(self, x: i32, y: i32) -> bool {
        let (w, h) = self.size();
        if !(0..w).contains(&x) || !(0..h).contains(&y) {
            return false;
        }
        match self {
            Mark::Corner(Bracket { thick, cut, .. }) => {
                let inner = x >= thick && y >= thick && x + y - 2 * thick >= cut_at(cut, thick).max(0);
                x + y >= cut && !inner
            }
            Mark::Down(_) => {
                let half = w as f32 / 2.0;
                (x as f32 + 0.5 - half).abs() <= half * (1.0 - (y as f32 + 0.5) / h as f32)
            }
        }
    }
}

/// The rects of each layer of a mark, unflipped, row by row.
fn mark_layers(mark: Mark) -> [Vec<Rect>; MARK_LAYERS] {
    let (w, h) = mark.size();
    let band = |x: i32, y: i32| mark.covers(x, y);
    let band_pixels: Vec<(i32, i32)> =
        (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| band(x, y)).collect();
    // The edge takes the pixels next to the band, diagonally too; glow ring `i` the rest past
    // `i + 1` and up to `i + 2` away.
    let layer_of = |x: i32, y: i32| -> Option<usize> {
        if band(x, y) {
            return Some(MARK_BAND);
        }
        let near = |&(bx, by): &(i32, i32)| (bx - x).abs().max((by - y).abs());
        if band_pixels.iter().map(near).min() == Some(1) {
            return Some(MARK_EDGE);
        }
        let dist = |&(bx, by): &(i32, i32)| (((bx - x).pow(2) + (by - y).pow(2)) as f32).sqrt();
        let ring = band_pixels.iter().map(dist).fold(f32::INFINITY, f32::min).ceil() as usize;
        (ring >= 2 && ring - 2 < FRAME_GLOW.len()).then(|| MARK_GLOW + ring - 2)
    };
    let mut layers: [Vec<Rect>; MARK_LAYERS] = Default::default();
    let span = -MARK_REACH..w + MARK_REACH;
    for y in -MARK_REACH..h + MARK_REACH {
        let mut run: Option<(i32, usize)> = None;
        for x in span.start..=span.end {
            let layer = span.contains(&x).then(|| layer_of(x, y)).flatten();
            match run {
                Some((_, l)) if Some(l) == layer => continue,
                Some((from, l)) => layers[l].push(Rect { x: from, y, w: x - from, h: 1 }),
                None => {}
            }
            run = layer.map(|l| (x, l));
        }
    }
    layers
}

/// `rects` mirrored across the axes at the origin as asked.
fn flipped(rects: &[Rect], flip_x: bool, flip_y: bool) -> Vec<Rect> {
    let flip = |at: i32, len: i32, on: bool| if on { -at - len } else { at };
    rects.iter().map(|r| Rect { x: flip(r.x, r.w, flip_x), y: flip(r.y, r.h, flip_y), ..*r }).collect()
}

/// Layer `rects` of a mark placed as `m` asks: each copy mirrored, scaled and moved to its
/// point, on the pixels the renderer fills for the scaled rects.
fn placed(m: &MarkLayer, rects: &[Rect]) -> Vec<Rect> {
    let k = f32::from_bits(m.k);
    let scaled = |r: &Rect| pixel_rect(r.x as f32 * k, r.y as f32 * k, r.w as f32 * k, r.h as f32 * k);
    let mut out = Vec::new();
    for (dx, dy, flip_x, flip_y) in m.copies.placed() {
        let copy = flipped(rects, flip_x, flip_y);
        out.extend(copy.iter().filter_map(scaled).map(|[x, y, w, h]| Rect { x: x + dx, y: y + dy, w, h }));
    }
    out
}

/// A game primitive of `rects`, white; None when the game builds none.
fn build_primitive(rects: &[Rect]) -> Option<usize> {
    // SAFETY: the renderer reads the rects, alive for the call, into a primitive of its own.
    let p = unsafe { (api().multi_rect)(&CppVector::of(rects), WHITE) };
    (!p.is_null()).then_some(p as usize)
}

/// A game pie of `pie`, white; None when the game builds none.
fn build_pie(pie: Pie) -> Option<usize> {
    let f = f32::from_bits;
    // SAFETY: the renderer builds a primitive of its own from plain values.
    let p = unsafe { (api().pie_partial)(0, 0, f(pie.r), f(pie.from), f(pie.end), f(pie.width), WHITE) };
    (!p.is_null()).then_some(p as usize)
}

impl Shapes {
    /// Frees the SHAPES_EVICT primitives drawn longest ago, ties with the last of them
    /// included.
    fn evict(&mut self) {
        let mut uses: Vec<u64> = self.cached.values().map(|&(_, used)| used).collect();
        if uses.len() < SHAPES_EVICT {
            return;
        }
        let (_, &mut last, _) = uses.select_nth_unstable(SHAPES_EVICT - 1);
        self.cached.retain(|_, &mut (p, used)| {
            if used <= last {
                // SAFETY: a primitive built in shape_primitive and drawn no more.
                unsafe { (api().destroy_primitive)(p as Ptr) };
            }
            used > last
        });
    }
}

/// The game primitive of a shape, built white on first use; None when the game builds none.
/// A mark's layers are all built with it.
fn shape_primitive(shape: &Shape) -> Option<Ptr> {
    let mut shapes = SHAPES.lock().unwrap_or_else(|e| e.into_inner());
    shapes.uses += 1;
    let now = shapes.uses;
    if let Some(entry) = shapes.cached.get_mut(shape) {
        entry.1 = now;
        return Some(entry.0 as Ptr);
    }
    if shapes.cached.len() >= SHAPES_MAX {
        shapes.evict();
    }
    let cached = &mut shapes.cached;
    let built = match shape {
        Shape::Ring(r) => build_primitive(&ring_rects(*r)),
        Shape::Mark(m) => {
            for (layer, rects) in mark_layers(m.mark).iter().enumerate() {
                let layer = MarkLayer { layer, ..*m };
                let key = Shape::Mark(layer);
                if cached.contains_key(&key) {
                    continue;
                }
                if let Some(p) = build_primitive(&placed(&layer, rects)) {
                    cached.insert(key, (p, now));
                }
            }
            None
        }
        Shape::Rects(rects) => {
            let rects: Vec<Rect> = rects.iter().map(|&[x, y, w, h]| Rect { x, y, w, h }).collect();
            build_primitive(&rects)
        }
        Shape::Rings(rings) => {
            let at =
                |&(x, y, r): &PlacedRing| ring_rects(r).into_iter().map(move |q| Rect { x: q.x + x, y: q.y + y, ..q });
            build_primitive(&rings.iter().flat_map(at).collect::<Vec<_>>())
        }
        Shape::Pie(pie) => build_pie(*pie),
    };
    if let Some(p) = built {
        cached.insert(shape.clone(), (p, now));
    }
    cached.get(shape).map(|&(p, _)| p as Ptr)
}

/// Draws `shape` with its origin at (`x`, `y`), `k` times its size.
fn draw_shape(x: f32, y: f32, k: f32, shape: Shape, color: GlColor) {
    if color.a <= 0.0 {
        return;
    }
    let Some(p) = shape_primitive(&shape) else {
        return;
    };
    let a = api();
    // SAFETY: the renderer's matrix stack around one of its primitives, popped below.
    unsafe {
        (a.push_matrix)();
        (a.translate)(x, y, 0.0);
        if k != 1.0 {
            (a.scale)(k, k, 1.0);
        }
        (a.render_primitive_color)(p, color);
        (a.pop_matrix)();
    }
}

/// Draws ring `r` with its top left corner at (`x`, `y`).
fn draw_ring(x: i32, y: i32, r: Ring, color: GlColor) {
    if r.w > 0 && r.h > 0 {
        draw_shape(x as f32, y as f32, 1.0, Shape::Ring(r), color);
    }
}

/// A frame as draw_frame takes it: its band's outer edge, its style and its fill.
pub type Frame<'a> = (f32, f32, f32, f32, &'a FrameStyle, GlColor);

/// The layers of `frame` on whole pixels: each ring's top left corner, the ring and its colour.
fn frame_layers((x, y, w, h, style, fill): Frame) -> Vec<(i32, i32, Ring, GlColor)> {
    let [x, y, w, h] = [x, y, w, h].map(|v| v.round() as i32);
    let mut layers = Vec::new();
    // The layer from `inset` pixels in, `width` wide or all the rest.
    let mut layer = |inset: i32, width: Option<i32>, color: GlColor| {
        let r = Ring {
            w: w - 2 * inset,
            h: h - 2 * inset,
            cut: cut_at(style.cut, inset),
            hole: width.map(|t| (t, cut_at(style.cut, inset + t))),
        };
        if r.w > 0 && r.h > 0 {
            layers.push((x + inset, y + inset, r, color));
        }
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
    layers
}

/// Draws a frame of `style` with its band's outer edge at `x, y, w, h`, on whole pixels, filled
/// with `fill`; the glow reaches FRAME_GLOW's length past it.
pub fn draw_frame(x: f32, y: f32, w: f32, h: f32, style: &FrameStyle, fill: GlColor) {
    for (x, y, r, color) in frame_layers((x, y, w, h, style, fill)) {
        draw_ring(x, y, r, color);
    }
    reset_fixed_color();
}

/// Draws frames as draw_frame does, all layers of one colour as one primitive.
/// The frames must not overlap one another.
pub fn draw_frames(frames: &[Frame]) {
    let mut runs: Vec<(GlColor, Vec<PlacedRing>)> = Vec::new();
    for (x, y, r, color) in frames.iter().flat_map(|&f| frame_layers(f)) {
        match runs.iter_mut().find(|(c, _)| *c == color) {
            Some((_, rings)) => rings.push((x, y, r)),
            None => runs.push((color, vec![(x, y, r)])),
        }
    }
    for (color, rings) in runs {
        let (x0, y0, _) = rings[0];
        let local = rings.iter().map(|&(x, y, r)| (x - x0, y - y0, r)).collect();
        draw_shape(x0 as f32, y0 as f32, 1.0, Shape::Rings(local), color);
    }
    reset_fixed_color();
}

/// Sets the engine's fixed colour back to white after tinted primitives.
fn reset_fixed_color() {
    // The game's draws leave the engine's fixed colour white, which untinted text relies on.
    // SAFETY: the engine's colour state, set from a colour alive for the call.
    unsafe { (api().set_fixed_color)(&WHITE) };
}

/// The tint that turns the pale of the game's button art into `fill`.
pub fn art_tint(fill: GlColor) -> GlColor {
    let p = BUTTON_PALE_COLOR;
    GlColor::new((fill.r / p.r).min(1.0), (fill.g / p.g).min(1.0), (fill.b / p.b).min(1.0), 1.0)
}

/// `from` moved toward `to` by the share `t`, opaque.
pub fn mixed(from: GlColor, to: GlColor, t: f32) -> GlColor {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    GlColor::new(lerp(from.r, to.r), lerp(from.g, to.g), lerp(from.b, to.b), 1.0)
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
    reset_fixed_color();
}

/// A game texture's size in pixels.
pub fn texture_size(texture: Ptr) -> (f32, f32) {
    // SAFETY: a texture the game handed out.
    let t = unsafe { Obj::from_ptr(texture) };
    t.map_or((0.0, 0.0), |t| (t.i32(TEXTURE_W) as f32, t.i32(TEXTURE_H) as f32))
}

/// Draws the top `rows` of a game texture at its size, its top left at (x, y).
pub fn draw_image_top(texture: Ptr, x: f32, y: f32, rows: f32) {
    let (w, h) = texture_size(texture);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    // SAFETY: the renderer's blit on a game texture; the region goes in as fractions.
    unsafe { (api().blit_partial)(texture, x, y, w, rows, 0.0, 1.0, 0.0, rows / h, 1.0, WHITE, false) }
}

/// Game images looked up by path under img/, misses included.
#[derive(Default)]
pub struct Images {
    cache: HashMap<String, usize>,
}

impl Images {
    /// The texture of a game image, or None when the game has none.
    pub fn get(&mut self, path: &str) -> Option<Ptr> {
        if let Some(&texture) = self.cache.get(path) {
            return (texture != 0).then_some(texture as Ptr);
        }
        let texture = *self.cache.entry(path.to_string()).or_insert_with(|| {
            let s = GameString::new(path);
            let a = api();
            // SAFETY: ResourceControl::GetImageId on the global resources and a std::string.
            let texture = ipad_art::own_lookup(|| unsafe { (a.image_id)(a.resources as Ptr, s.as_ptr()) }) as usize;
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
    use crate::game::fake::{calls, live_strings, with_game, Call};
    use std::collections::BTreeSet;

    /// The primitives the calls since the last look built, and those they destroyed.
    fn built_and_destroyed() -> (usize, usize) {
        let calls = calls();
        let count = |f: fn(&Call) -> bool| calls.iter().filter(|c| f(c)).count();
        (count(|c| matches!(c, Call::BuildPrimitive(_))), count(|c| matches!(c, Call::DestroyPrimitive(_))))
    }

    fn empty_cache() {
        SHAPES.lock().unwrap_or_else(|e| e.into_inner()).cached.clear();
    }

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

    /// The pixels `rects` cover.
    fn rect_pixels(rects: &[Rect]) -> impl Iterator<Item = (i32, i32)> + '_ {
        rects.iter().flat_map(|r| (r.x..r.x + r.w).flat_map(move |x| (r.y..r.y + r.h).map(move |y| (x, y))))
    }

    /// The pixels of each layer of a mark, each in one layer once.
    fn mark_pixels(mark: Mark) -> Vec<BTreeSet<(i32, i32)>> {
        let mut seen = BTreeSet::new();
        let layers = mark_layers(mark);
        let layer_set = |rects: &Vec<Rect>| {
            let mut layer = BTreeSet::new();
            for p in rect_pixels(rects) {
                assert!(seen.insert(p), "{mark:?} covers {p:?} twice");
                layer.insert(p);
            }
            layer
        };
        layers.iter().map(layer_set).collect()
    }

    #[test]
    fn a_bracket_corner_is_a_cut_band_in_an_edge_and_a_glow() {
        for b in [
            Bracket { arm: 14, thick: 2, cut: 4 },
            Bracket { arm: 9, thick: 2, cut: 4 },
            Bracket { arm: 8, thick: 2, cut: 4 },
        ] {
            let layers = mark_pixels(Mark::Corner(b));
            let band = &layers[MARK_BAND];
            for along in b.cut + b.thick..b.arm {
                assert_eq!(band.iter().filter(|&&(x, _)| x == along).count(), b.thick as usize, "{b:?} column {along}");
                assert_eq!(band.iter().filter(|&&(_, y)| y == along).count(), b.thick as usize, "{b:?} row {along}");
            }
            assert!(!band.contains(&(0, 0)), "{b:?}: the cut");
            assert!(band.contains(&(b.arm - 1, 0)) && !band.contains(&(b.arm, 0)), "{b:?}: the arm");
            for &(x, y) in band {
                for p in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (x + dx, y + dy))) {
                    assert!(band.contains(&p) || layers[MARK_EDGE].contains(&p), "{b:?}: no edge at {p:?}");
                }
            }
            assert!(layers[MARK_GLOW..].iter().all(|ring| !ring.is_empty()), "{b:?}");
        }
    }

    #[test]
    fn a_down_marker_is_a_triangle_mirrored_about_its_middle() {
        let marker = Marker { w: 16, h: 9 };
        let layers = mark_pixels(Mark::Down(marker));
        let band = &layers[MARK_BAND];
        let row = |y: i32| band.iter().filter(|&&(_, by)| by == y).count();
        assert_eq!(row(0), marker.w as usize, "the base");
        assert!((1..marker.h).all(|y| row(y) <= row(y - 1)), "narrowing to the tip");
        let tip = (0..marker.h).rev().find(|&y| row(y) > 0).expect("the base row is full");
        let middle = marker.w / 2;
        assert_eq!(band.iter().filter(|&&(_, y)| y == tip).count(), 2, "a two-pixel tip");
        assert!(band.contains(&(middle - 1, tip)) && band.contains(&(middle, tip)), "the tip in the middle");
        for layer in &layers {
            assert_eq!(layer.iter().map(|&(x, y)| (marker.w - 1 - x, y)).collect::<BTreeSet<_>>(), *layer);
        }
    }

    #[test]
    fn a_placed_bracket_covers_the_pixels_of_its_corners_scaled_by_the_renderer() {
        let b = Bracket { arm: 10, thick: 2, cut: 4 };
        let copies = Copies::Corners { w: 57, h: 31 };
        // A pixel the renderer fills: its centre inside the rect, the far edges excluded.
        let fills = |lo: f32, len: f32, px: i32| {
            let c = px as f32 + 0.5;
            c >= lo.min(lo + len) && c < lo.max(lo + len)
        };
        for k in [1.0, 1.33, 1.5, 2.0] {
            for (layer, rects) in mark_layers(Mark::Corner(b)).iter().enumerate() {
                let m = MarkLayer { mark: Mark::Corner(b), copies, k: f32::to_bits(k), layer };
                let got: BTreeSet<_> = rect_pixels(&placed(&m, rects)).collect();
                let mut want = BTreeSet::new();
                for (dx, dy, flip_x, flip_y) in copies.placed() {
                    for r in flipped(rects, flip_x, flip_y) {
                        let (x0, y0, w, h) = (r.x as f32 * k, r.y as f32 * k, r.w as f32 * k, r.h as f32 * k);
                        let reach = (b.arm as f32 * k) as i32 + MARK_REACH * 2 + 2;
                        for px in -reach..=reach {
                            for py in -reach..=reach {
                                if fills(x0, w, px) && fills(y0, h, py) {
                                    want.insert((px + dx, py + dy));
                                }
                            }
                        }
                    }
                }
                assert_eq!(got, want, "k {k} layer {layer}");
            }
        }
    }

    #[test]
    fn a_flipped_corner_mirrors_across_its_point() {
        let b = Bracket { arm: 10, thick: 2, cut: 4 };
        for rects in mark_layers(Mark::Corner(b)) {
            let plain: BTreeSet<_> = rect_pixels(&rects).collect();
            let both: BTreeSet<_> = rect_pixels(&flipped(&rects, true, true)).collect();
            assert_eq!(both, plain.iter().map(|&(x, y)| (-x - 1, -y - 1)).collect());
        }
    }

    #[test]
    fn ring_segments_span_whole_pie_steps() {
        let span = TURN_DEG / RING_SEGMENTS as f32 - RING_GAP_DEG;
        assert!(span > 0.0 && (span / PIE_STEP_DEG).fract() == 0.0, "{span}");
    }

    #[test]
    fn a_batched_rect_covers_the_pixels_whose_centres_it_holds() {
        assert_eq!(pixel_rect(10.0, 20.0, 3.0, 1.0), Some([10, 20, 3, 1]));
        assert_eq!(pixel_rect(10.5, 20.4, 2.0, 1.0), Some([10, 20, 2, 1]));
        assert_eq!(pixel_rect(10.6, 20.0, 2.6, 1.0), Some([11, 20, 2, 1]));
        assert_eq!(pixel_rect(13.0, 20.0, -3.0, 1.0), Some([10, 20, 3, 1]));
        assert_eq!(pixel_rect(10.0, 20.0, 0.0, 1.0), None);
        assert_eq!(pixel_rect(10.6, 20.0, 0.3, 1.0), None);
    }

    #[test]
    fn a_full_cache_frees_the_shapes_drawn_longest_ago() {
        with_game(|_| {
            empty_cache();
            let hot = Shape::Rects(vec![[0, 0, 1, 1]]);
            let cold = |i: usize| Shape::Rects(vec![[0, 0, 2, i as i32]]);
            let hot_at = shape_primitive(&hot);
            for i in 0..=SHAPES_MAX {
                shape_primitive(&cold(i));
                shape_primitive(&hot);
            }
            assert_eq!(shape_primitive(&hot), hot_at, "drawn all along");
            let shapes = SHAPES.lock().unwrap_or_else(|e| e.into_inner());
            assert!(!shapes.cached.contains_key(&cold(0)));
            assert!(shapes.cached.contains_key(&cold(SHAPES_MAX)));
            assert!(shapes.cached.len() <= SHAPES_MAX);
            drop(shapes);
            let (built, destroyed) = built_and_destroyed();
            assert_eq!((built, destroyed), (SHAPES_MAX + 2, SHAPES_EVICT));
        });
    }

    #[test]
    fn frames_drawn_together_take_a_primitive_per_colour_wherever_they_are() {
        with_game(|_| {
            empty_cache();
            let pair = |x: f32, fill| {
                [
                    (x, 50.0, 60.0, 60.0, &BUTTON_FRAME, BUTTON_PALE_COLOR),
                    (x + 80.0, 50.0, 60.0, 60.0, &BUTTON_FRAME, fill),
                ]
            };
            let mut colors: Vec<GlColor> = Vec::new();
            for (.., color) in frame_layers(pair(0.0, BUTTON_PALE_COLOR)[0]) {
                if !colors.contains(&color) {
                    colors.push(color);
                }
            }
            draw_frames(&pair(10.0, BUTTON_PALE_COLOR));
            assert_eq!(built_and_destroyed(), (colors.len(), 0), "one per colour for both");
            draw_frames(&pair(300.4, BUTTON_PALE_COLOR));
            assert_eq!(built_and_destroyed(), (0, 0), "moved");
            draw_frames(&pair(10.0, BUTTON_SELECTED_COLOR));
            assert_eq!(built_and_destroyed(), (2, 0), "the pale run without one fill, and that fill");
        });
    }

    #[test]
    fn arcs_are_built_once_and_drawn_from_the_cache() {
        with_game(|_| {
            empty_cache();
            draw_ring_timer(100.3, 50.0, 20.0, 4.0, 5, RING_LIT_COLOR, 1.0);
            draw_round_frame(300.0, 200.0, 50.0, 3.0, INK_COLOR);
            assert_eq!(built_and_destroyed(), (1 + RING_SEGMENTS + FRAME_GLOW.len() + 2, 0));
            draw_ring_timer(400.0, 70.6, 20.0, 4.0, 9, RING_LIT_COLOR, 0.5);
            draw_round_frame(10.4, 20.0, 50.0, 3.0, TEXT_COLOR);
            assert_eq!(built_and_destroyed(), (0, 0), "moved, lit and tinted otherwise");
        });
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
