//! Plate glyphs: built-ins and settings the game has no art for, as large text or as pixel
//! art of slate rects drawn through the batch.

use std::f32::consts::FRAC_1_SQRT_2;

use super::draw::*;
use crate::game::api::GlColor;

/// Glyphs of plates without game art: text as wide as the icon box, up to GLYPH_TEXT_MAX
/// times its size; drawn ones in GLYPH_STROKE lines, corner brackets GLYPH_W by GLYPH_H.
const GLYPH_TEXT_MAX: f32 = 2.0;
pub(super) const TEXT_GLYPH: TextGlyph = TextGlyph { text: "Aa", ink: (-0.5, 0.5) };
pub(super) const HELP_GLYPH: TextGlyph = TextGlyph { text: "?", ink: (1.0, 1.0) };
const GLYPH_W: f32 = 32.0;
const GLYPH_H: f32 = 26.0;
const GLYPH_ARM: f32 = 9.0;
const GLYPH_STROKE: f32 = 3.0;
/// The zoom glyph: a lens ring round a plus, its handle running to the bottom right corner of a
/// LENS_BOX square.
const LENS_BOX: f32 = 28.0;
const LENS_R: f32 = 10.0;
const LENS_PLUS: f32 = 8.0;
const LENS_PLUS_STROKE: f32 = 2.0;
const LENS_HANDLE: f32 = 4.0;
/// The fast forward glyph: two triangles pointing right, tip to base.
const FORWARD_GLYPH_W: f32 = 13.0;
const FORWARD_GLYPH_H: f32 = 20.0;
/// The other ship glyph: two arrows passing each other, the upper one pointing right.
const SWAP_GLYPH_W: f32 = 28.0;
const SWAP_SHAFT: f32 = 4.0;
const SWAP_HEAD_W: f32 = 8.0;
const SWAP_HEAD_H: f32 = 12.0;
/// The distance between the arrows' middle lines.
const SWAP_SPACING: f32 = 10.0;
/// The autopause glyph: the two bars of a pause sign.
const PAUSE_GLYPH_BAR_W: f32 = 6.0;
const PAUSE_GLYPH_BAR_H: f32 = 20.0;
const PAUSE_GLYPH_GAP: f32 = 6.0;
/// The badge glyph: a row of title letters over a row of smaller caption letters, each row
/// BADGE_LETTERS blocks a line apart.
const BADGE_LETTERS: usize = 5;
const BADGE_TITLE: (f32, f32) = (5.0, 8.0);
const BADGE_CAPTION: (f32, f32) = (3.0, 4.0);
const BADGE_ROW_GAP: f32 = 4.0;
const BADGE_H: f32 = BADGE_TITLE.1 + BADGE_ROW_GAP + BADGE_CAPTION.1;
/// The rumble glyph: a pad body, and each side of it waves that shorten outward.
const RUMBLE_BODY_W: f32 = 8.0;
const RUMBLE_BODY_H: f32 = 20.0;
const RUMBLE_WAVE_GAP: f32 = 3.0;
const RUMBLE_WAVES_H: [f32; 2] = [14.0, 8.0];
/// The crew marker glyph, top down: the marker, a gap, a crew member's head and shoulders.
const FIGURE_MARKER_W: f32 = 16.0;
const FIGURE_MARKER_H: f32 = 9.0;
const FIGURE_GAP: f32 = 4.0;
const FIGURE_HEAD_R: f32 = 5.0;
const FIGURE_BODY_W: f32 = 18.0;
const FIGURE_BODY_H: f32 = 6.0;
const FIGURE_H: f32 = FIGURE_MARKER_H + FIGURE_GAP + 2.0 * FIGURE_HEAD_R + OVERLAY_LINE + FIGURE_BODY_H;
/// The HUD shift glyph: a screen, and inside it a panel each side with a triangle pushing it
/// toward the middle.
const SHIFT_SCREEN_W: f32 = 34.0;
const SHIFT_SCREEN_H: f32 = 24.0;
const SHIFT_SCREEN_LINE: f32 = 2.0;
const SHIFT_HEAD_W: f32 = 5.0;
const SHIFT_HEAD_H: f32 = 10.0;
const SHIFT_PANEL_W: f32 = 4.0;
const SHIFT_PANEL_H: f32 = 14.0;
/// How far each panel's outer side sits from the screen's edge.
const SHIFT_PANEL_IN: f32 = 9.0;
/// The frame filter glyph: a checkerboard of FILTER_CELLS by FILTER_CELLS squares, an arrow,
/// and one square of their average.
const FILTER_CELLS: i32 = 2;
const FILTER_CELL: f32 = 7.0;
const FILTER_SQUARE: f32 = FILTER_CELLS as f32 * FILTER_CELL;
const FILTER_HEAD_W: f32 = 5.0;
const FILTER_HEAD_H: f32 = 8.0;
const FILTER_GAP: f32 = 2.0;
const FILTER_W: f32 = 2.0 * FILTER_SQUARE + FILTER_HEAD_W + 2.0 * FILTER_GAP;
/// Ink over the plate at half strength: the checkerboard's average.
const FILTER_AVERAGE_COLOR: GlColor = GlColor { a: 0.5, ..INK_COLOR };
/// The beacon glyph: a diamond as tall as it is wide.
const BEACON_GLYPH: f32 = 22.0;
/// The display glyph: a screen outline over a neck and a base.
const DISPLAY_W: f32 = 32.0;
const DISPLAY_SCREEN_H: f32 = 22.0;
const DISPLAY_NECK_W: f32 = 6.0;
const DISPLAY_BASE_W: f32 = 18.0;
const DISPLAY_H: f32 = DISPLAY_SCREEN_H + 2.0 * GLYPH_STROKE;
/// The pointer size glyph: a pointer in one-line cells over a dimension line SIZE_LINE_W wide,
/// SIZE_LINE_GAP under it, with end ticks SIZE_TICK_H tall centred on the line.
const POINTER_ROWS: [&str; 20] = [
    "#............",
    "##...........",
    "###..........",
    "####.........",
    "#####........",
    "######.......",
    "########.....",
    "#########....",
    "##########...",
    "###########..",
    "############.",
    "#############",
    "########.....",
    "########.....",
    "###..####....",
    "##...####....",
    "#.....####...",
    "......####...",
    ".......####..",
    ".......#.....",
];
const SIZE_LINE_W: f32 = 28.0;
const SIZE_LINE_GAP: f32 = 3.0;
const SIZE_TICK_H: f32 = 8.0;
const SIZE_STROKE: f32 = 2.0;
/// A six-row A in LETTER_CELL squares.
const TALL_A: [&str; 6] = [".###.", "#...#", "#...#", "#####", "#...#", "#...#"];
const LETTER_CELL: f32 = 2.0;
/// The autofire glyph: a crosshair, a ring of radius AIM_R crossed by ticks from AIM_TICK_IN to
/// AIM_TICK_OUT, and in its empty top left corner a TALL_A, its bottom right AIM_A_GAP up and
/// left of the ring's centre.
const AIM_R: f32 = 10.0;
const AIM_TICK_IN: f32 = 4.0;
const AIM_TICK_OUT: f32 = 12.0;
const AIM_TICK_STROKE: f32 = 2.0;
const AIM_A_GAP: f32 = 8.0;
/// The repeat glyph: a loop REPEAT_W by REPEAT_H between its shafts' middle lines, each shaft
/// ending in a head; the sides stop REPEAT_GAP short of the heads.
const REPEAT_W: f32 = 30.0;
const REPEAT_H: f32 = 20.0;
const REPEAT_SHAFT: f32 = 4.0;
const REPEAT_HEAD_W: f32 = 7.0;
const REPEAT_HEAD_H: f32 = 12.0;
const REPEAT_GAP: f32 = 2.0;
/// The focus repeat glyph: HOP_ZONES squares HOP_ZONE wide, HOP_STEP apart, the last filled, and
/// half rings of HOP_STEP across arcing from each zone to the next, the last ending in a head.
const HOP_ZONES: usize = 3;
const HOP_ZONE: f32 = 9.0;
const HOP_STEP: f32 = 12.0;
const HOP_LINE: f32 = 2.0;
const HOP_HEAD_W: f32 = 8.0;
const HOP_HEAD_H: f32 = 6.0;
/// The pad glyph: a pill's outline PAD_W by PAD_H, a D-pad's cross PAD_CROSS long in its left
/// end and two PAD_BUTTON squares in its right one, the second up and right of the first.
const PAD_W: f32 = 32.0;
const PAD_H: f32 = 16.0;
const PAD_LINE: f32 = 2.0;
const PAD_CROSS: f32 = 6.0;
const PAD_CROSS_LINE: f32 = 2.0;
/// The cross's and the buttons' middles off the pad's middle.
const PAD_CROSS_AT: f32 = 8.0;
const PAD_BUTTONS_AT: f32 = 7.0;
const PAD_BUTTON: f32 = 2.0;
const PAD_BUTTON_STEP: f32 = 3.0;
/// The touch glyph: a hand pointing up with a ring round its fingertip, in TOUCH_CELL squares,
/// raised TOUCH_RAISE above centred: the ring is lighter than the hand.
const TOUCH_HAND: [&str; 29] = [
    ".......#####..........",
    ".....#########........",
    "....###.....###.......",
    "...###.......###......",
    "...##.........##......",
    "..##...........##.....",
    "..##...........##.....",
    "..##....###....##.....",
    "..##...#####...##.....",
    "..##...#####...##.....",
    "...##..#####..##......",
    "...###.#####.###......",
    "....##.#####.##.......",
    ".....#.#####.#........",
    ".......#####..........",
    ".......#####.##.......",
    ".......####.###.##....",
    "...#...####.##.####...",
    "######.####.##.######.",
    "###########.##.###.###",
    "###########.##.###.###",
    ".#############.###.###",
    "..################.###",
    "...###############.###",
    "....##################",
    ".....#################",
    "......################",
    ".......##############.",
    "........###########...",
];
const TOUCH_CELL: f32 = 1.0;
const TOUCH_RAISE: f32 = 2.5;
/// The room crew glyph: a room ROOM_W by ROOM_H holding a crew member's head and shoulders, and
/// ROOM_A_GAP right of it a TALL_A.
const ROOM_W: f32 = 20.0;
const ROOM_H: f32 = 24.0;
const ROOM_LINE: f32 = 2.0;
const ROOM_A_GAP: f32 = 3.0;
const ROOM_HEAD_R: f32 = 3.0;
/// The head's middle below the room's top, and the gap from the head down to the shoulders.
const ROOM_HEAD_AT: f32 = 8.0;
const ROOM_BODY_GAP: f32 = 2.0;
const ROOM_BODY_W: f32 = 10.0;
const ROOM_BODY_H: f32 = 5.0;
/// The hold time glyph: a button's disc in a timer's ring that runs clockwise from the top to
/// the left, and a tick over its start.
const HOLD_RING_R: f32 = 13.0;
const HOLD_RING_LINE: f32 = 3.0;
const HOLD_RING_SPAN: (f32, f32) = (-90.0, 180.0);
const HOLD_BUTTON_R: f32 = 6.0;
const HOLD_TICK_W: f32 = 2.0;
const HOLD_TICK_H: f32 = 4.0;
/// The stick glyph, from the side: a base, a shaft from the base's middle to a knob tilted up
/// and right, and an arc of its swing up and left.
const STICK_BASE_W: f32 = 26.0;
const STICK_BASE_H: f32 = 5.0;
/// The base's top and the knob's middle off the glyph's middle.
const STICK_BASE_AT: f32 = 7.0;
const STICK_KNOB_AT: (f32, f32) = (8.0, -8.0);
const STICK_KNOB_R: f32 = 5.0;
const STICK_SHAFT_W: f32 = 3.0;
const STICK_SWING_R: f32 = 14.0;
const STICK_SWING_LINE: f32 = 2.0;
const STICK_SWING_SPAN: (f32, f32) = (200.0, 260.0);
/// The swap glyph: two face buttons, rings of radius BUTTON_R round letters of LETTER_CELL
/// squares, the second BUTTON_AT up and right of the glyph's middle and the first as far down
/// and left; quarter rings of radius SWAP_ARC_R round the other two corners carry the arrows.
const BUTTON_R: f32 = 10.0;
const BUTTON_LINE: f32 = 2.0;
const BUTTON_AT: (f32, f32) = (8.0, 7.0);
const SWAP_ARC_R: f32 = 8.0;
const SWAP_ARC_LINE: f32 = 2.0;
const SWAP_ARROW_W: f32 = 4.0;
const SWAP_ARROW_H: f32 = 6.0;
pub(super) const LETTER_A: Letter = [".###.", "#...#", "#####", "#...#", "#...#"];
pub(super) const LETTER_B: Letter = ["####.", "#...#", "####.", "#...#", "####."];
pub(super) const LETTER_X: Letter = ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"];
pub(super) const LETTER_Y: Letter = ["#...#", ".#.#.", "..#..", "..#..", "..#.."];
/// The B pause glyph: a pause button, two bars in a ring, and a B PAUSE_B_AT from the ring's
/// middle; the ring is left out over PAUSE_RING_GAP, degrees clockwise from the right, round the B.
const PAUSE_RING_R: f32 = 12.0;
const PAUSE_RING_LINE: f32 = 2.0;
const PAUSE_RING_BAR_W: f32 = 4.0;
const PAUSE_RING_BAR_H: f32 = 12.0;
const PAUSE_RING_BAR_GAP: f32 = 4.0;
const PAUSE_B_AT: (f32, f32) = (14.0, 10.0);
const PAUSE_RING_GAP: (f32, f32) = (15.0, 50.0);

/// A face button's letter in rows of LETTER_CELL squares.
pub(super) type Letter = [&'static str; 5];

/// Text as a glyph: the text, and how far the font's ink sits off the middle of its advance
/// and line, in font pixels, as measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TextGlyph {
    text: &'static str,
    ink: (f32, f32),
}

/// A plate's own glyph, for a built-in or a setting the game has no art for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Glyph {
    /// Text drawn large, its ink centred.
    Text(TextGlyph),
    /// A magnifying glass with a plus.
    Zoom,
    /// Two triangles pointing right.
    FastForward,
    /// Two arrows passing each other.
    Swap,
    /// The bars of a pause sign.
    Pause,
    /// A line of title letters over a smaller caption line.
    Badge,
    /// A pad body between waves.
    Rumble,
    /// Corner brackets alone.
    Brackets,
    /// A crew marker over a crew member.
    Marker,
    /// A screen with its side panels pushed in.
    HudShift,
    /// A checkerboard, an arrow, and a square of their average.
    FrameFilter,
    /// A map beacon's diamond.
    Beacon,
    /// A display on its stand.
    Display,
    /// The game's autofire target mark: an A beside a crosshair.
    Autofire,
    /// Two arrows chasing each other round a loop.
    Repeat,
    /// The focus hopping from zone to zone.
    FocusRepeat,
    /// A pad's outline with a D-pad and two buttons.
    Pad,
    /// A hand pointing up with a ring round its fingertip.
    Touch,
    /// A crew member in a room beside an A.
    RoomCrew,
    /// A button in a timer's ring.
    HoldTime,
    /// A stick tilted on its base.
    Stick,
    /// Two face buttons, the second up and right of the first, and arrows trading them.
    Buttons(Letter, Letter),
    /// A pause button with a B over its lower right.
    CancelPause,
    /// A pointer over a dimension line.
    PointerSize,
}

/// Draws `rows` of `#` cells `cell` square from (`left`, `top`), a row's run as one rect.
fn draw_cells(rows: &[&str], left: f32, top: f32, cell: f32) {
    for (row, line) in rows.iter().enumerate() {
        let mut col = 0;
        for run in line.as_bytes().chunk_by(|a, b| a == b) {
            if run[0] == b'#' {
                draw_rect(left + col as f32 * cell, top + row as f32 * cell, run.len() as f32 * cell, cell, INK_COLOR);
            }
            col += run.len();
        }
    }
}

/// The width and height of `rows` in cells `cell` square.
fn cells_size(rows: &[&str], cell: f32) -> (f32, f32) {
    (rows[0].len() as f32 * cell, rows.len() as f32 * cell)
}

/// Draws `glyph` in slate centred at cx,cy, text fitted to `size` wide.
pub(super) fn draw_glyph(glyph: Glyph, font: i32, cx: f32, cy: f32, size: f32) {
    match glyph {
        Glyph::Text(g) => {
            let k = (size / text_width(font, g.text)).min(GLYPH_TEXT_MAX);
            set_color(INK_COLOR);
            draw_text_scaled(font, cx - g.ink.0 * k, cy - g.ink.1 * k, k, g.text);
            set_color(TEXT_COLOR);
        }
        Glyph::Zoom => {
            // On whole pixels: the ring and the plus stay symmetric.
            let (left, top) = ((cx - LENS_BOX / 2.0).round(), (cy - LENS_BOX / 2.0).round());
            let (lx, ly) = (left + LENS_R, top + LENS_R);
            draw_pixel_ring(lx, ly, LENS_R, GLYPH_STROKE, INK_COLOR);
            let (p, t) = (LENS_PLUS, LENS_PLUS_STROKE);
            draw_rect(lx - p / 2.0, ly - t / 2.0, p, t, INK_COLOR);
            draw_rect(lx - t / 2.0, ly - p / 2.0, t, p, INK_COLOR);
            // Squares a pixel apart down the diagonal, from the ring's middle to the corner.
            let mut d = (LENS_R + (LENS_R - GLYPH_STROKE / 2.0) * FRAC_1_SQRT_2 - LENS_HANDLE / 2.0).round();
            while d <= LENS_BOX - LENS_HANDLE {
                draw_rect(left + d, top + d, LENS_HANDLE, LENS_HANDLE, INK_COLOR);
                d += OVERLAY_LINE;
            }
        }
        Glyph::FastForward => {
            // On whole pixels: each row stays one line.
            let (left, cy) = ((cx - FORWARD_GLYPH_W).round(), cy.round());
            draw_side_triangle(left, cy, FORWARD_GLYPH_W, FORWARD_GLYPH_H, INK_COLOR);
            draw_side_triangle(left + FORWARD_GLYPH_W, cy, FORWARD_GLYPH_W, FORWARD_GLYPH_H, INK_COLOR);
        }
        Glyph::Swap => {
            // On whole pixels: each row stays one line.
            let (left, cy) = ((cx - SWAP_GLYPH_W / 2.0).round(), cy.round());
            let (upper, lower) = (cy - SWAP_SPACING / 2.0, cy + SWAP_SPACING / 2.0);
            let (shaft_w, head_at) = (SWAP_GLYPH_W - SWAP_HEAD_W, left + SWAP_GLYPH_W - SWAP_HEAD_W);
            draw_rect(left, upper - SWAP_SHAFT / 2.0, shaft_w, SWAP_SHAFT, INK_COLOR);
            draw_side_triangle(head_at, upper, SWAP_HEAD_W, SWAP_HEAD_H, INK_COLOR);
            draw_rect(left + SWAP_HEAD_W, lower - SWAP_SHAFT / 2.0, shaft_w, SWAP_SHAFT, INK_COLOR);
            draw_side_triangle(left + SWAP_HEAD_W, lower, -SWAP_HEAD_W, SWAP_HEAD_H, INK_COLOR);
        }
        Glyph::Pause => {
            let (w, h, gap) = (PAUSE_GLYPH_BAR_W, PAUSE_GLYPH_BAR_H, PAUSE_GLYPH_GAP);
            // On whole pixels: both bars keep one width.
            let (cx, top) = (cx.round(), (cy - h / 2.0).round());
            draw_rect(cx - gap / 2.0 - w, top, w, h, INK_COLOR);
            draw_rect(cx + gap / 2.0, top, w, h, INK_COLOR);
        }
        Glyph::Badge => {
            // On whole pixels: the letters keep one size.
            let mut top = (cy - BADGE_H / 2.0).round();
            for (w, h) in [BADGE_TITLE, BADGE_CAPTION] {
                let step = w + OVERLAY_LINE;
                let left = (cx - (BADGE_LETTERS as f32 * step - OVERLAY_LINE) / 2.0).round();
                for i in 0..BADGE_LETTERS {
                    draw_rect(left + i as f32 * step, top, w, h, INK_COLOR);
                }
                top += h + BADGE_ROW_GAP;
            }
        }
        Glyph::Rumble => {
            let t = GLYPH_STROKE;
            draw_rect(cx - RUMBLE_BODY_W / 2.0, cy - RUMBLE_BODY_H / 2.0, RUMBLE_BODY_W, RUMBLE_BODY_H, INK_COLOR);
            let mut off = RUMBLE_BODY_W / 2.0 + RUMBLE_WAVE_GAP;
            for h in RUMBLE_WAVES_H {
                draw_rect(cx + off, cy - h / 2.0, t, h, INK_COLOR);
                draw_rect(cx - off - t, cy - h / 2.0, t, h, INK_COLOR);
                off += t + RUMBLE_WAVE_GAP;
            }
        }
        Glyph::Brackets => {
            let t = GLYPH_STROKE;
            draw_corners(cx - GLYPH_W / 2.0, cy - GLYPH_H / 2.0, GLYPH_W, GLYPH_H, GLYPH_ARM, t, INK_COLOR);
        }
        Glyph::Marker => {
            // On whole pixels: the head stays round.
            let (cx, top) = (cx.round(), (cy - FIGURE_H / 2.0).round());
            let tip = top + FIGURE_MARKER_H;
            draw_down_marker(cx, tip, FIGURE_MARKER_W, FIGURE_MARKER_H, INK_COLOR);
            let head = tip + FIGURE_GAP + FIGURE_HEAD_R;
            draw_pixel_ring(cx, head, FIGURE_HEAD_R, FIGURE_HEAD_R, INK_COLOR);
            let body = head + FIGURE_HEAD_R + OVERLAY_LINE;
            draw_rect(cx - FIGURE_BODY_W / 2.0, body, FIGURE_BODY_W, FIGURE_BODY_H, INK_COLOR);
        }
        Glyph::HudShift => {
            // On whole pixels: each row stays one line.
            let (left, top, cy) =
                ((cx - SHIFT_SCREEN_W / 2.0).round(), (cy - SHIFT_SCREEN_H / 2.0).round(), cy.round());
            let (line, right) = (SHIFT_SCREEN_LINE, left + SHIFT_SCREEN_W);
            draw_outline(left, top, SHIFT_SCREEN_W, SHIFT_SCREEN_H, line, INK_COLOR);
            draw_side_triangle(left + line + OVERLAY_LINE, cy, SHIFT_HEAD_W, SHIFT_HEAD_H, INK_COLOR);
            draw_side_triangle(right - line - OVERLAY_LINE, cy, -SHIFT_HEAD_W, SHIFT_HEAD_H, INK_COLOR);
            let panel_top = cy - SHIFT_PANEL_H / 2.0;
            draw_rect(left + SHIFT_PANEL_IN, panel_top, SHIFT_PANEL_W, SHIFT_PANEL_H, INK_COLOR);
            draw_rect(right - SHIFT_PANEL_IN - SHIFT_PANEL_W, panel_top, SHIFT_PANEL_W, SHIFT_PANEL_H, INK_COLOR);
        }
        Glyph::FrameFilter => {
            // On whole pixels: the cells keep one size.
            let (left, top, cy) = ((cx - FILTER_W / 2.0).round(), (cy - FILTER_SQUARE / 2.0).round(), cy.round());
            for row in 0..FILTER_CELLS {
                for col in (row % 2..FILTER_CELLS).step_by(2) {
                    let (x, y) = (left + col as f32 * FILTER_CELL, top + row as f32 * FILTER_CELL);
                    draw_rect(x, y, FILTER_CELL, FILTER_CELL, INK_COLOR);
                }
            }
            draw_outline(left, top, FILTER_SQUARE, FILTER_SQUARE, OVERLAY_LINE, INK_COLOR);
            draw_side_triangle(left + FILTER_SQUARE + FILTER_GAP, cy, FILTER_HEAD_W, FILTER_HEAD_H, INK_COLOR);
            let average = left + FILTER_W - FILTER_SQUARE;
            draw_rect(average, top, FILTER_SQUARE, FILTER_SQUARE, FILTER_AVERAGE_COLOR);
            draw_outline(average, top, FILTER_SQUARE, FILTER_SQUARE, OVERLAY_LINE, INK_COLOR);
        }
        Glyph::Beacon => draw_diamond(cx.round(), cy.round(), BEACON_GLYPH, BEACON_GLYPH, INK_COLOR),
        Glyph::Display => {
            let (left, top) = ((cx - DISPLAY_W / 2.0).round(), (cy - DISPLAY_H / 2.0).round());
            let (cx, t) = (cx.round(), GLYPH_STROKE);
            draw_outline(left, top, DISPLAY_W, DISPLAY_SCREEN_H, t, INK_COLOR);
            draw_rect(cx - DISPLAY_NECK_W / 2.0, top + DISPLAY_SCREEN_H, DISPLAY_NECK_W, t, INK_COLOR);
            draw_rect(cx - DISPLAY_BASE_W / 2.0, top + DISPLAY_SCREEN_H + t, DISPLAY_BASE_W, t, INK_COLOR);
        }
        Glyph::Autofire => {
            let (a_w, a_h) = cells_size(&TALL_A, LETTER_CELL);
            // Centred over the A and the crosshair, on whole pixels: the ring stays symmetric.
            let ax = (cx + (a_w + AIM_A_GAP - AIM_TICK_OUT) / 2.0).round();
            let ay = (cy + (a_h + AIM_A_GAP - AIM_TICK_OUT) / 2.0).round();
            draw_pixel_ring(ax, ay, AIM_R, GLYPH_STROKE, INK_COLOR);
            let (t, len) = (AIM_TICK_STROKE, AIM_TICK_OUT - AIM_TICK_IN);
            draw_rect(ax - t / 2.0, ay - AIM_TICK_OUT, t, len, INK_COLOR);
            draw_rect(ax - t / 2.0, ay + AIM_TICK_IN, t, len, INK_COLOR);
            draw_rect(ax - AIM_TICK_OUT, ay - t / 2.0, len, t, INK_COLOR);
            draw_rect(ax + AIM_TICK_IN, ay - t / 2.0, len, t, INK_COLOR);
            draw_cells(&TALL_A, ax - AIM_A_GAP - a_w, ay - AIM_A_GAP - a_h, LETTER_CELL);
        }
        Glyph::Repeat => {
            // On whole pixels: each row stays one line.
            let (left, cy) = ((cx - REPEAT_W / 2.0).round(), cy.round());
            let (right, t) = (left + REPEAT_W, REPEAT_SHAFT);
            let (upper, lower) = (cy - REPEAT_H / 2.0, cy + REPEAT_H / 2.0);
            let (head_w, head_h) = (REPEAT_HEAD_W, REPEAT_HEAD_H);
            draw_rect(left, upper - t / 2.0, REPEAT_W - head_w, t, INK_COLOR);
            draw_side_triangle(right - head_w, upper, head_w, head_h, INK_COLOR);
            draw_rect(left + head_w, lower - t / 2.0, REPEAT_W - head_w, t, INK_COLOR);
            draw_side_triangle(left + head_w, lower, -head_w, head_h, INK_COLOR);
            let side_h = REPEAT_H - head_h / 2.0 - REPEAT_GAP + t / 2.0;
            draw_rect(left, upper - t / 2.0, t, side_h, INK_COLOR);
            draw_rect(right - t, lower + t / 2.0 - side_h, t, side_h, INK_COLOR);
        }
        Glyph::FocusRepeat => {
            let r = HOP_STEP / 2.0;
            let last = (HOP_ZONES - 1) as f32 * HOP_STEP;
            // The head starts on the hops' bottom row.
            let drop = HOP_HEAD_H - OVERLAY_LINE;
            let (w, h) = (last + HOP_ZONE, r + drop + HOP_ZONE);
            let (left, top) = ((cx - w / 2.0).round(), (cy - h / 2.0).round());
            let (arc_y, zone_top) = (top + r, top + r + drop);
            // On whole pixels: every hop keeps one shape.
            let hop_mid = |zone: f32| (zone + HOP_ZONE / 2.0 - r).floor();
            for k in 0..HOP_ZONES {
                let x = left + k as f32 * HOP_STEP;
                if k + 1 < HOP_ZONES {
                    draw_outline(x, zone_top, HOP_ZONE, HOP_ZONE, HOP_LINE, INK_COLOR);
                } else {
                    draw_rect(x, zone_top, HOP_ZONE, HOP_ZONE, INK_COLOR);
                }
                if k > 0 {
                    draw_pixel_arc(hop_mid(x), arc_y, r, HOP_LINE, (TURN_DEG / 2.0, TURN_DEG), INK_COLOR);
                }
            }
            let head_x = hop_mid(left + last) + r - HOP_LINE / 2.0;
            draw_down_triangle(head_x, zone_top, HOP_HEAD_W, HOP_HEAD_H, INK_COLOR);
        }
        Glyph::Pad => {
            // On whole pixels: the pill and the cross stay symmetric.
            let (cx, cy) = (cx.round(), cy.round());
            let r = PAD_H / 2.0;
            draw_pixel_stadium(cx, cy, PAD_W / 2.0 - r, r, PAD_LINE, INK_COLOR);
            let (dx, t) = (cx - PAD_CROSS_AT, PAD_CROSS_LINE);
            draw_rect(dx - PAD_CROSS / 2.0, cy - t / 2.0, PAD_CROSS, t, INK_COLOR);
            draw_rect(dx - t / 2.0, cy - PAD_CROSS / 2.0, t, PAD_CROSS, INK_COLOR);
            let (bx, step) = (cx + PAD_BUTTONS_AT, PAD_BUTTON_STEP / 2.0);
            draw_rect(bx - step - PAD_BUTTON / 2.0, cy + step - PAD_BUTTON / 2.0, PAD_BUTTON, PAD_BUTTON, INK_COLOR);
            draw_rect(bx + step - PAD_BUTTON / 2.0, cy - step - PAD_BUTTON / 2.0, PAD_BUTTON, PAD_BUTTON, INK_COLOR);
        }
        Glyph::Touch => {
            let (w, h) = cells_size(&TOUCH_HAND, TOUCH_CELL);
            let (left, top) = ((cx - w / 2.0).round(), (cy - h / 2.0 - TOUCH_RAISE).round());
            draw_cells(&TOUCH_HAND, left, top, TOUCH_CELL);
        }
        Glyph::RoomCrew => {
            let (a_w, a_h) = cells_size(&TALL_A, LETTER_CELL);
            let (left, top) = ((cx - (ROOM_W + ROOM_A_GAP + a_w) / 2.0).round(), (cy - ROOM_H / 2.0).round());
            draw_outline(left, top, ROOM_W, ROOM_H, ROOM_LINE, INK_COLOR);
            let (mid, head) = (left + ROOM_W / 2.0, top + ROOM_HEAD_AT);
            draw_pixel_ring(mid, head, ROOM_HEAD_R, ROOM_HEAD_R, INK_COLOR);
            let body = head + ROOM_HEAD_R + ROOM_BODY_GAP;
            draw_rect(mid - ROOM_BODY_W / 2.0, body, ROOM_BODY_W, ROOM_BODY_H, INK_COLOR);
            draw_cells(&TALL_A, left + ROOM_W + ROOM_A_GAP, (cy - a_h / 2.0).round(), LETTER_CELL);
        }
        Glyph::HoldTime => {
            let (cx, cy) = (cx.round(), cy.round());
            draw_pixel_arc(cx, cy, HOLD_RING_R, HOLD_RING_LINE, HOLD_RING_SPAN, INK_COLOR);
            draw_pixel_ring(cx, cy, HOLD_BUTTON_R, HOLD_BUTTON_R, INK_COLOR);
            let tick_top = cy - HOLD_RING_R - OVERLAY_LINE;
            draw_rect(cx - HOLD_TICK_W / 2.0, tick_top, HOLD_TICK_W, HOLD_TICK_H, INK_COLOR);
        }
        Glyph::Stick => {
            let (cx, cy) = (cx.round(), cy.round());
            let base = cy + STICK_BASE_AT;
            draw_rect(cx - STICK_BASE_W / 2.0, base, STICK_BASE_W, STICK_BASE_H, INK_COLOR);
            let (kx, ky) = (cx + STICK_KNOB_AT.0, cy + STICK_KNOB_AT.1);
            // The shaft row by row, each row's middle on the line from the base to the knob.
            let mut y = ky;
            while y < base {
                let x = cx + (kx - cx) * (base - y) / (base - ky);
                draw_rect((x - STICK_SHAFT_W / 2.0).round(), y, STICK_SHAFT_W, OVERLAY_LINE, INK_COLOR);
                y += OVERLAY_LINE;
            }
            draw_pixel_ring(kx, ky, STICK_KNOB_R, STICK_KNOB_R, INK_COLOR);
            draw_pixel_arc(cx, cy, STICK_SWING_R, STICK_SWING_LINE, STICK_SWING_SPAN, INK_COLOR);
        }
        Glyph::Buttons(first, second) => {
            let (cx, cy) = (cx.round(), cy.round());
            let (bx, by) = BUTTON_AT;
            let (l_w, l_h) = cells_size(&first, LETTER_CELL);
            for (letter, x, y) in [(first, cx - bx, cy + by), (second, cx + bx, cy - by)] {
                draw_pixel_ring(x, y, BUTTON_R, BUTTON_LINE, INK_COLOR);
                draw_cells(&letter, x - l_w / 2.0, y - l_h / 2.0, LETTER_CELL);
            }
            // Over the top left corner toward the second, under the bottom right toward the first.
            let (r, t) = (SWAP_ARC_R, SWAP_ARC_LINE);
            draw_pixel_arc(cx - bx, cy - by, r, t, (180.0, 270.0), INK_COLOR);
            draw_side_triangle(cx - bx, cy - by - r + t / 2.0, SWAP_ARROW_W, SWAP_ARROW_H, INK_COLOR);
            draw_pixel_arc(cx + bx, cy + by, r, t, (0.0, 90.0), INK_COLOR);
            draw_side_triangle(cx + bx, cy + by + r - t / 2.0, -SWAP_ARROW_W, SWAP_ARROW_H, INK_COLOR);
        }
        Glyph::CancelPause => {
            let (l_w, l_h) = cells_size(&LETTER_B, LETTER_CELL);
            let (r, (bx, by)) = (PAUSE_RING_R, PAUSE_B_AT);
            // The ring and the B centred as one box.
            let (rx, ry) = ((cx - (bx + l_w / 2.0 - r) / 2.0).round(), (cy - (by + l_h / 2.0 - r) / 2.0).round());
            let (from, to) = PAUSE_RING_GAP;
            draw_pixel_arc(rx, ry, r, PAUSE_RING_LINE, (to, from + TURN_DEG), INK_COLOR);
            let (w, h, gap) = (PAUSE_RING_BAR_W, PAUSE_RING_BAR_H, PAUSE_RING_BAR_GAP);
            let (left, top) = (rx - gap / 2.0 - w, (ry - h / 2.0).round());
            draw_rect(left, top, w, h, INK_COLOR);
            draw_rect(left + w + gap, top, w, h, INK_COLOR);
            draw_cells(&LETTER_B, rx + bx - l_w / 2.0, ry + by - l_h / 2.0, LETTER_CELL);
        }
        Glyph::PointerSize => {
            let (w, h) = cells_size(&POINTER_ROWS, OVERLAY_LINE);
            let t = SIZE_STROKE;
            // On whole pixels: each row stays one line.
            let top = (cy - (h + SIZE_LINE_GAP + (t + SIZE_TICK_H) / 2.0) / 2.0).round();
            draw_cells(&POINTER_ROWS, (cx - w / 2.0).round(), top, OVERLAY_LINE);
            let (left, line_top) = ((cx - SIZE_LINE_W / 2.0).round(), top + h + SIZE_LINE_GAP);
            draw_rect(left, line_top, SIZE_LINE_W, t, INK_COLOR);
            let tick_top = line_top + (t - SIZE_TICK_H) / 2.0;
            draw_rect(left, tick_top, t, SIZE_TICK_H, INK_COLOR);
            draw_rect(left + SIZE_LINE_W - t, tick_top, t, SIZE_TICK_H, INK_COLOR);
        }
    }
}
