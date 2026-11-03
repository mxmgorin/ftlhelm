//! The project's brand drawn with the game's primitives: the mark, the project icon's ship's
//! wheel whose rim is a wheel menu with its top plate picked, and the name in the banner's pixel
//! capitals.
//!
//! The mark is the icon's cell map drawn in whole-pixel cells, one batch per colour; the ground
//! under it shows through its gaps.

use super::draw::*;
use crate::game::api::GlColor;

/// The project icon as ICON_GRID rows of role letters: `b` band, `l` line, `g` ground, `i` ink,
/// `s` plates, `p` the picked plate, a space clear. The mark draws the last three.
const ICON: &str = include_str!("icon64.txt");
const ICON_GRID: usize = 64;

/// The mark's colours: its lines, its plates and the picked plate.
pub struct MarkColors {
    pub ink: GlColor,
    pub plates: GlColor,
    pub pick: GlColor,
}

/// Returns the colour of a role letter in the mark, None for the roles it leaves out.
fn role_color(role: u8, c: &MarkColors) -> Option<GlColor> {
    match role {
        b'i' => Some(c.ink),
        b's' => Some(c.plates),
        b'p' => Some(c.pick),
        _ => None,
    }
}

/// Returns the mark's width in cells: the widest span of its roles in a row.
fn mark_cells(c: &MarkColors) -> usize {
    let span = |row: &[u8]| {
        let drawn = |b: &u8| role_color(*b, c).is_some();
        match (row.iter().position(drawn), row.iter().rposition(drawn)) {
            (Some(first), Some(last)) => last - first + 1,
            _ => 0,
        }
    };
    ICON.lines().map(|row| span(row.as_bytes())).max().unwrap_or(0)
}

/// Draws the mark centred on (`cx`, `cy`), reaching about `r` pixels from its centre in cells of
/// whole pixels.
pub fn draw_mark(cx: f32, cy: f32, r: f32, c: &MarkColors) {
    let cell = (2.0 * r / mark_cells(c).max(1) as f32).floor().max(1.0);
    let half = cell * ICON_GRID as f32 / 2.0;
    let (left, top) = ((cx - half).round(), (cy - half).round());
    draw_batched(|| {
        for &role in b"isp" {
            let Some(color) = role_color(role, c) else { continue };
            for (y, row) in ICON.lines().enumerate() {
                let row = row.as_bytes();
                let mut x = 0;
                while x < row.len() {
                    let run = row[x..].iter().take_while(|&&b| b == row[x]).count();
                    if row[x] == role {
                        draw_rect(left + x as f32 * cell, top + y as f32 * cell, run as f32 * cell, cell, color);
                    }
                    x += run;
                }
            }
        }
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
    fn the_mark_is_the_icons_wheel_in_whole_cells_one_batch_per_colour_the_pick_on_top() {
        let c = MarkColors { ink: INK_COLOR, plates: BUTTON_OFF_COLOR, pick: BUTTON_SELECTED_COLOR };
        let rects = rects_of(|| draw_mark(100.0, 100.0, 60.0, &c));
        let cell = rects.first().expect("the mark has cells").0[3];
        assert!(cell > 0 && rects.iter().all(|(r, _)| r[3] == cell && r[2] % cell == 0), "whole cells");
        let colours: Vec<GlColor> = rects.chunk_by(|a, b| a.1 == b.1).map(|run| run[0].1).collect();
        assert_eq!(colours, [c.ink, c.plates, c.pick]);
        assert!(rects.iter().filter(|(_, col)| *col == c.pick).all(|(r, _)| r[1] + r[3] <= 100), "pick above");
    }

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
