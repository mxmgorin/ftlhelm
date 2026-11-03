//! The project's brand drawn with the game's primitives: the mark, a ship's wheel whose rim is
//! a wheel menu with its top plate picked, and the name in the banner's pixel capitals.
//!
//! The mark keeps the banner's geometry in units of the banner's canvas, scaled to the radius
//! asked for. It is drawn in opaque colours, so parts laid over one another, spokes over the
//! rim and the hub, keep one shade; a faint mark mixes its colours toward the ground under it.

use super::draw::*;
use crate::game::api::GlColor;

/// The mark's plates, one per gap between spokes; the first points straight up.
const PLATES: usize = 8;
const PLATE_DEG: f32 = 360.0 / PLATES as f32;
const UP_DEG: f32 = -90.0;
/// Radii and widths in canvas units: the plates' band, the gaps cut between them, the rim
/// around them, the spokes from the hub to the handles, the hub ring and the cross inside it.
const PLATE_IN: f32 = 92.0;
const PLATE_OUT: f32 = 170.0;
const GAP_IN: f32 = 54.0;
const GAP_W: f32 = 26.0;
const RIM_R: f32 = 172.0;
const RIM_W: f32 = 16.0;
const SPOKE_OUT: f32 = 190.0;
const SPOKE_W: f32 = 12.0;
const HANDLE_OUT: f32 = 212.0;
const HANDLE_W: f32 = 30.0;
const HUB_R: f32 = 60.0;
const HUB_W: f32 = 14.0;
const CROSS_ARM: f32 = 24.0;
const CROSS_HALF: f32 = 32.0;
/// How far the mark reaches from its centre: the handles' far ends, caps included.
const MARK_REACH: f32 = HANDLE_OUT + HANDLE_W / 2.0;

/// The mark's colours: its lines, its plates, the picked plate, and the ground it stands on,
/// which shows through its gaps and hub.
pub struct MarkColors {
    pub ink: GlColor,
    pub plates: GlColor,
    pub pick: GlColor,
    pub ground: GlColor,
}

/// Draws the mark centred on (`cx`, `cy`), reaching `r` pixels from its centre.
pub fn draw_mark(cx: f32, cy: f32, r: f32, c: &MarkColors) {
    let (cx, cy) = (cx.round(), cy.round());
    let s = r / MARK_REACH;
    let gaps = (0..PLATES).map(|i| UP_DEG + PLATE_DEG / 2.0 + PLATE_DEG * i as f32);
    let band = (PLATE_OUT - PLATE_IN) * s;
    draw_arc(cx, cy, PLATE_OUT * s, band, (0.0, 360.0), c.plates);
    // The pie runs in whole steps: the picked plate overhangs its gaps, which cover it.
    let pick_span = (PLATE_DEG / PIE_STEP_DEG).ceil() * PIE_STEP_DEG;
    draw_arc(cx, cy, PLATE_OUT * s, band, (UP_DEG - pick_span / 2.0, UP_DEG + pick_span / 2.0), c.pick);
    for deg in gaps.clone() {
        draw_ray(cx, cy, (GAP_IN * s, RIM_R * s), deg, GAP_W * s, c.ground);
    }
    for deg in gaps.clone() {
        draw_ray(cx, cy, (HUB_R * s, SPOKE_OUT * s), deg, SPOKE_W * s, c.ink);
    }
    draw_arc(cx, cy, (RIM_R + RIM_W / 2.0) * s, RIM_W * s, (0.0, 360.0), c.ink);
    for deg in gaps {
        draw_ray(cx, cy, (SPOKE_OUT * s, HANDLE_OUT * s), deg, HANDLE_W * s, c.ink);
    }
    draw_disc(cx, cy, (HUB_R + HUB_W / 2.0) * s, c.ink);
    draw_disc(cx, cy, (HUB_R - HUB_W / 2.0) * s, c.ground);
    let (arm, half) = (CROSS_ARM * s, CROSS_HALF * s);
    draw_batched(|| {
        draw_rect(cx - arm / 2.0, cy - half, arm, 2.0 * half, c.ink);
        draw_rect(cx - half, cy - arm / 2.0, half - arm / 2.0, arm, c.ink);
        draw_rect(cx + arm / 2.0, cy - arm / 2.0, half - arm / 2.0, arm, c.ink);
    });
}

/// The banner's pixel capitals for the letters of the name, rows top down.
const PIXEL_ROWS: usize = 7;
#[rustfmt::skip]
const PIXEL_CAPS: [(char, [&str; PIXEL_ROWS]); 6] = [
    ('E', ["11111", "1....", "1....", "1111.", "1....", "1....", "11111"]),
    ('F', ["11111", "1....", "1....", "1111.", "1....", "1....", "1...."]),
    ('H', ["1...1", "1...1", "1...1", "11111", "1...1", "1...1", "1...1"]),
    ('L', ["1....", "1....", "1....", "1....", "1....", "1....", "11111"]),
    ('M', ["1...1", "11.11", "1.1.1", "1.1.1", "1...1", "1...1", "1...1"]),
    ('T', ["11111", "..1..", "..1..", "..1..", "..1..", "..1..", "..1.."]),
];
/// The name's first NAME_SPLIT letters take the first colour, the rest the second.
const NAME_SPLIT: usize = 3;

fn name() -> String {
    env!("CARGO_PKG_NAME").to_ascii_uppercase()
}

fn caps(letter: char) -> &'static [&'static str; PIXEL_ROWS] {
    let found = PIXEL_CAPS.iter().find(|(c, _)| *c == letter);
    &found.expect("every letter of the name has pixel capitals").1
}

/// The name's height in cells.
pub const NAME_ROWS: f32 = PIXEL_ROWS as f32;

/// The name's width in cells: its letters, a cell apart.
pub fn name_cols() -> f32 {
    let letters = name().chars().map(|ch| caps(ch)[0].len()).sum::<usize>();
    (letters + name().chars().count() - 1) as f32
}

/// Draws the name with its top left corner at (`x`, `y`), each pixel a `cell` square, its first
/// part in `first` and the rest in `rest`.
pub fn draw_name(x: f32, y: f32, cell: f32, (first, rest): (GlColor, GlColor)) {
    draw_batched(|| {
        let mut col = 0;
        for (i, letter) in name().chars().enumerate() {
            let rows = caps(letter);
            let color = if i < NAME_SPLIT { first } else { rest };
            for (r, row) in rows.iter().enumerate() {
                for (c, _) in row.chars().enumerate().filter(|&(_, bit)| bit == '1') {
                    draw_rect(x + (col + c) as f32 * cell, y + r as f32 * cell, cell, cell, color);
                }
            }
            col += rows[0].len() + 1;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_spans_its_letters_a_cell_apart_in_two_colours() {
        let (first, rest) = (INK_COLOR, BUTTON_SELECTED_COLOR);
        let rects = rects_of(|| draw_name(0.0, 0.0, 2.0, (first, rest)));
        let right = rects.iter().map(|(r, _)| r[0] + r[2]).max().expect("the name has pixels");
        assert_eq!(right as f32, 2.0 * name_cols());
        let split_x = 2 * (3 * 5 + 3);
        assert!(rects.iter().all(|&(r, c)| (r[0] < split_x) == (c == first)), "FTL then HELM");
    }
}
