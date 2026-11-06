//! The on-screen keyboard: keys over the game while it edits a text field, for the pad and
//! touch. It opens once the game takes typed text while touch, or a pad the mouse has not
//! moved after, is in use, and closes when the game stops taking it.
//!
//! The keys reach the game's own text handlers, so the field keeps its length limit and the
//! characters it allows. While open, the keyboard takes every press: the D-pad and the sticks
//! move between keys and to the name above them, where left and right move the caret; A
//! presses the focused key, and fixed buttons erase, type a space, confirm and cancel; a held
//! direction or erase repeats at the focus repeat's pace. Cancel puts back the name the field
//! held when the keyboard opened.
//!
//! A letter that starts a word is a capital unless Shift turns that off; otherwise Shift makes
//! the next letter a capital, and a second press locks capitals.
//!
//! The window keeps to the side of what is shown away from the cursor, which stands on the
//! field: the name being edited with its caret on top, the keys under it. It is laid out in
//! panel units around its centre and drawn `k` times its size, as the help is.

use super::draw::*;
use super::overlay::{band_text_top, draw_window, panel_scale, HELP_FONT, HELP_LINE, HELP_PAD, PANEL_MARGIN};
use super::text;
use crate::config::layout::Input;
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::actions::{text_event, type_char};
use crate::game::api::GlColor;
use crate::game::offsets::*;
use crate::game::state::{self, TextField};

/// The keys' columns; every row spans them all.
const COLS: usize = 10;
/// The character rows, lowercase.
const CHAR_ROWS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm-."];
/// The keys after each character row's own, with the columns each spans.
const ROW_ENDS: [&[(Key, usize)]; CHAR_ROWS.len()] = [&[], &[], &[], &[(Key::Erase, 1)]];
const BOTTOM_ROW: [(Key, usize); 4] = [(Key::Shift, 2), (Key::Space, 4), (Key::Cancel, 2), (Key::Done, 2)];
const ROW_COUNT: usize = CHAR_ROWS.len() + 1;
/// The key focused as the keyboard opens: the middle of the letters.
const START_FOCUS: (usize, usize) = (2, 4);
/// The steps of a direction input, in the D-pad's and the sticks' order: left, up, right, down.
const DIRECTIONS: [(i32, i32); 4] = [(-1, 0), (0, -1), (1, 0), (0, 1)];
/// A key: KEY_SIDE square, KEY_GAP from the next.
const KEY_SIDE: f32 = 34.0;
const KEY_GAP: f32 = 4.0;
/// The space between the name and the keys.
const SECTION_GAP: f32 = 8.0;
/// The name's text keeps this far in from its strip's ends.
const NAME_INSET: f32 = 8.0;
/// The counter shows only while this much stays clear between it and the name's caret.
const COUNT_GAP: f32 = 6.0;
/// The caret: CARET_W wide, CARET_GAP clear of the text before it, the strip's height less
/// CARET_INSET at each end, so it shows past the letters' outlines.
const CARET_W: f32 = 2.0;
const CARET_GAP: f32 = 1.0;
const CARET_INSET: f32 = 4.0;
/// The erase key's arrow: ERASE_W long, its head ERASE_HEAD long and ERASE_H tall, its shaft
/// ERASE_SHAFT thick.
const ERASE_W: f32 = 16.0;
const ERASE_HEAD: f32 = 7.0;
const ERASE_H: f32 = 12.0;
const ERASE_SHAFT: f32 = 4.0;
/// The window keeps this far from the cursor, clear of the field under it.
const FIELD_CLEARANCE: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Key {
    Char(char),
    Erase,
    Shift,
    Space,
    Cancel,
    Done,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Shift {
    #[default]
    Off,
    /// On for the letter that starts a word.
    Word,
    /// On for the next letter.
    Once,
    /// On until pressed again.
    Lock,
}

impl Shift {
    fn capital(self) -> bool {
        self != Shift::Off
    }

    /// The state a press of Shift leads to: off from a word's capital or locked capitals,
    /// else a step up.
    fn pressed(self) -> Shift {
        match self {
            Shift::Off => Shift::Once,
            Shift::Once => Shift::Lock,
            Shift::Word | Shift::Lock => Shift::Off,
        }
    }
}

/// The rows of keys, each key with its first column and the columns it spans.
fn rows() -> [Vec<(Key, usize, usize)>; ROW_COUNT] {
    let place = |keys: Vec<(Key, usize)>| {
        let mut first = 0;
        keys.into_iter()
            .map(|(key, span)| {
                first += span;
                (key, first - span, span)
            })
            .collect()
    };
    std::array::from_fn(|r| match CHAR_ROWS.get(r) {
        Some(chars) => place(chars.chars().map(|c| (Key::Char(c), 1)).chain(ROW_ENDS[r].iter().copied()).collect()),
        None => place(BOTTOM_ROW.to_vec()),
    })
}

/// The step a direction input takes; None for any other input.
fn direction(input: Input) -> Option<(i32, i32)> {
    let i = (input as usize).checked_sub(Input::DpadLeft as usize)?;
    Some(DIRECTIONS[i % DIRECTIONS.len()])
}

/// A key's title; None for the erase key, which shows an arrow.
fn title(key: Key, shift: Shift) -> Option<String> {
    let word = |w: &str| Some(w.to_string());
    match key {
        Key::Char(c) if shift.capital() => Some(c.to_ascii_uppercase().to_string()),
        Key::Char(c) => Some(c.to_string()),
        Key::Erase => None,
        Key::Shift if shift == Shift::Lock => word("Caps"),
        Key::Shift => word("Shift"),
        Key::Space => word("Space"),
        Key::Cancel => word("Cancel"),
        Key::Done => word("Done"),
    }
}

/// Whether the keyboard is open, its focus and Shift, the name to put back, and its repeats.
#[derive(Default)]
pub struct Keyboard {
    pub open: bool,
    /// The focused key's row, and the column it was reached at: a step up or down keeps it.
    row: usize,
    col: usize,
    /// The name above the keys has the focus instead; left and right move its caret.
    on_name: bool,
    shift: Shift,
    /// Whether the caret started a word when last looked.
    word_start: bool,
    /// The name as the keyboard found it; None for a field the layer cannot read.
    original: Option<String>,
    /// The window keeps to the top of what is shown, the field being in its bottom half.
    top: bool,
    /// The held input whose act repeats, and when it acts next.
    repeat: Option<(Input, f64)>,
}

impl Keyboard {
    fn focused(&self) -> Key {
        let row = &rows()[self.row];
        row.iter().find(|&&(_, first, span)| self.col < first + span).unwrap_or(&row[0]).0
    }

    /// Moves the focus a key along its row, or a row up or down at the same column, the name
    /// taking its turn above the top row; both wrap. On the name, a step along moves nothing.
    fn step(&mut self, (dx, dy): (i32, i32)) {
        let stops = ROW_COUNT as i32 + 1;
        let at = if self.on_name { 0 } else { self.row as i32 + 1 };
        match ((at + dy).rem_euclid(stops) as usize).checked_sub(1) {
            Some(row) => (self.on_name, self.row) = (false, row),
            None => self.on_name = true,
        }
        if dx != 0 && !self.on_name {
            let row = &rows()[self.row];
            let at = row.iter().position(|&(_, first, span)| self.col < first + span).unwrap_or(0);
            self.col = row[(at as i32 + dx).rem_euclid(row.len() as i32) as usize].1;
        }
    }

    /// Turns Shift on as the caret comes to a word's start, and off as it leaves one, unless
    /// it was set by hand.
    fn follow_word_start(&mut self, word_start: bool) {
        match (self.shift, word_start, self.word_start) {
            (Shift::Off, true, false) => self.shift = Shift::Word,
            (Shift::Word, false, true) => self.shift = Shift::Off,
            _ => {}
        }
        self.word_start = word_start;
    }
}

/// The keyboard window as laid out: its size in panel units, its centre in the frame and its
/// scale.
struct Panel {
    w: f32,
    h: f32,
    cx: f32,
    cy: f32,
    k: f32,
}

fn grid_w() -> f32 {
    COLS as f32 * (KEY_SIDE + KEY_GAP) - KEY_GAP
}

fn grid_h() -> f32 {
    ROW_COUNT as f32 * (KEY_SIDE + KEY_GAP) - KEY_GAP
}

impl Panel {
    fn top(&self) -> f32 {
        -self.h / 2.0
    }

    fn name_top(&self) -> f32 {
        self.top() + HELP_PAD
    }

    /// The strip that shows the name, in panel units.
    fn name_box(&self) -> (f32, f32, f32, f32) {
        (-self.w / 2.0 + HELP_PAD, self.name_top(), self.w - 2.0 * HELP_PAD, HELP_LINE)
    }

    fn keys_top(&self) -> f32 {
        self.name_top() + HELP_LINE + SECTION_GAP
    }

    /// The box of the key in `row` from column `first` over `span` columns, in panel units.
    fn key_box(&self, row: usize, first: usize, span: usize) -> (f32, f32, f32, f32) {
        let x = -grid_w() / 2.0 + first as f32 * (KEY_SIDE + KEY_GAP);
        let y = self.keys_top() + row as f32 * (KEY_SIDE + KEY_GAP);
        (x, y, span as f32 * (KEY_SIDE + KEY_GAP) - KEY_GAP, KEY_SIDE)
    }

    /// The point `(x, y)` of the frame in panel units.
    fn to_panel(&self, (x, y): (f32, f32)) -> (f32, f32) {
        ((x - self.cx) / self.k, (y - self.cy) / self.k)
    }

    fn contains(&self, (x, y): (f32, f32)) -> bool {
        x.abs() <= self.w / 2.0 && y.abs() <= self.h / 2.0
    }

    /// The key at panel point `at`: its row, its first column and the key.
    fn key_at(&self, at: (f32, f32)) -> Option<(usize, usize, Key)> {
        rows().iter().enumerate().find_map(|(r, row)| {
            row.iter()
                .find_map(|&(key, first, span)| within(self.key_box(r, first, span), at).then_some((r, first, key)))
        })
    }
}

/// Whether point `(px, py)` lies in the box `(x, y, w, h)`.
fn within((x, y, w, h): (f32, f32, f32, f32), (px, py): (f32, f32)) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// Draws the erase key's arrow pointing left, centred on screen point (`cx`, `cy`), `k`
/// times its size.
fn draw_erase(cx: f32, cy: f32, k: f32, color: GlColor) {
    let [w, head, h, shaft] = [ERASE_W, ERASE_HEAD, ERASE_H, ERASE_SHAFT].map(|v| (v * k).round());
    let (base, cy) = ((cx - w / 2.0).round() + head, cy.round());
    draw_side_triangle(base, cy, -head, h, color);
    draw_rect(base, cy - (shaft / 2.0).round(), w - head, shaft, color);
}

/// One text the keyboard prints, in panel units: its left edge or, when centred, its middle.
struct Print {
    x: f32,
    y: f32,
    text: String,
    color: GlColor,
    centred: bool,
}

impl Layer {
    /// Opens the keyboard when the game takes typed text while touch, or a pad the mouse has
    /// not moved after, is in use; closes it once the game stops. Follows the field's word
    /// starts for Shift.
    pub fn follow_text_input(&mut self) {
        let typing = state::text_input_on();
        let pad_or_touch = self.touch_in_use() || (self.pad.index.is_some() && !self.cursor.moved_by_mouse);
        if typing && !self.keyboard.open && pad_or_touch {
            self.open_keyboard(state::edited_name());
        } else if !typing && self.keyboard.open {
            self.close_keyboard();
        }
        if self.keyboard.open {
            let word_start = state::edited_name().is_some_and(|f| f.at_word_start());
            self.keyboard.follow_word_start(word_start);
        }
    }

    fn open_keyboard(&mut self, field: Option<TextField>) {
        self.close_wheel();
        let (_, y, _, h) = self.shown_area();
        let (row, col) = START_FOCUS;
        self.keyboard = Keyboard {
            open: true,
            row,
            col,
            original: field.map(|f| f.text.into_iter().collect()),
            top: self.cursor.y > y + h / 2.0,
            ..Keyboard::default()
        };
        if self.debug {
            log!("keyboard: open, name {:?}", self.keyboard.original);
        }
    }

    fn close_keyboard(&mut self) {
        self.keyboard = Keyboard::default();
        if self.debug {
            log!("keyboard: closed");
        }
    }

    /// The part of the frame shown: the zoomed view, else the whole frame.
    fn shown_area(&self) -> (f32, f32, f32, f32) {
        self.zoom.view.map_or((0.0, 0.0, GAME_W as f32, GAME_H as f32), |v| (v.x, v.y, v.w, v.h))
    }

    /// Takes a press while the keyboard is open, starting the repeat of one that repeats.
    pub fn keyboard_press(&mut self, input: Input, t: f64) {
        if self.keyboard_act(input) {
            self.keyboard.repeat = Some((input, self.next_step_at(t, true)));
        }
    }

    /// Does what `input` does in the keyboard: a direction moves the focus, or the caret along
    /// the focused name; A presses the focused key, X or L1 erases, Y or R1 types a space, Start
    /// confirms, B cancels. Returns whether it repeats while held.
    fn keyboard_act(&mut self, input: Input) -> bool {
        match input {
            Input::FaceDown if !self.keyboard.on_name => self.press_key(self.keyboard.focused()),
            Input::FaceLeft | Input::L1 => {
                self.press_key(Key::Erase);
                return true;
            }
            Input::FaceUp | Input::R1 => self.press_key(Key::Space),
            Input::FaceRight => self.press_key(Key::Cancel),
            Input::Start => self.press_key(Key::Done),
            _ => match direction(input) {
                Some((dx, 0)) if self.keyboard.on_name => {
                    text_event(if dx < 0 { TEXT_LEFT } else { TEXT_RIGHT });
                    return true;
                }
                Some(step) => {
                    self.keyboard.step(step);
                    return true;
                }
                None => {}
            },
        }
        false
    }

    /// Acts again for the held input that repeats, once due.
    pub fn repeat_keyboard(&mut self, t: f64) {
        let Some((input, at)) = self.keyboard.repeat else {
            return;
        };
        if !self.pad.held[input as usize] {
            self.keyboard.repeat = None;
        } else if t >= at {
            self.keyboard.repeat = Some((input, self.next_step_at(t, false)));
            self.keyboard_act(input);
        }
    }

    /// Presses `key`: types or erases in the game's field, turns Shift, or confirms or cancels
    /// the edit and closes the keyboard.
    fn press_key(&mut self, key: Key) {
        let kb = &mut self.keyboard;
        match key {
            Key::Char(c) => {
                type_char(if kb.shift.capital() { c.to_ascii_uppercase() } else { c });
                if kb.shift != Shift::Lock {
                    kb.shift = Shift::Off;
                }
            }
            Key::Erase => text_event(TEXT_BACKSPACE),
            Key::Shift => kb.shift = kb.shift.pressed(),
            Key::Space => type_char(' '),
            Key::Cancel => {
                let edited = state::edited_name().map(|f| f.text.into_iter().collect::<String>());
                if let Some(original) = kb.original.take().filter(|o| edited.is_some_and(|e| e != *o)) {
                    text_event(TEXT_CLEAR);
                    original.chars().for_each(type_char);
                }
                text_event(TEXT_CANCEL);
                self.close_keyboard();
            }
            Key::Done => {
                text_event(TEXT_CONFIRM);
                self.close_keyboard();
            }
        }
    }

    /// Takes a touch at frame point `at` while the keyboard is open: the key there is focused
    /// and pressed, the name there focused. Returns whether the touch landed on the window.
    pub fn touch_keyboard(&mut self, at: (f32, f32)) -> bool {
        let panel = self.keyboard_panel();
        let at = panel.to_panel(at);
        if !panel.contains(at) {
            return false;
        }
        if let Some((row, col, key)) = panel.key_at(at) {
            (self.keyboard.on_name, self.keyboard.row, self.keyboard.col) = (false, row, col);
            self.press_key(key);
        } else if within(panel.name_box(), at) {
            self.keyboard.on_name = true;
        }
        true
    }

    /// Lays the keyboard out across what is shown, at the far side from the cursor, at the
    /// text size as far as the room there holds it.
    fn keyboard_panel(&self) -> Panel {
        let w = grid_w() + 2.0 * HELP_PAD;
        let h = 2.0 * HELP_PAD + HELP_LINE + SECTION_GAP + grid_h();
        let (x, y, area_w, area_h) = self.shown_area();
        let top = self.keyboard.top;
        let room = if top { self.cursor.y - y } else { y + area_h - self.cursor.y } - FIELD_CLEARANCE;
        let k = panel_scale(text::scale(), w, h, (area_w, room));
        let reach = PANEL_MARGIN + h * k / 2.0;
        let cy = if top { y + reach } else { y + area_h - reach };
        Panel { w, h, cx: x + area_w / 2.0, cy, k }
    }

    /// Draws the keyboard: the edited name with its caret, and the keys.
    pub fn draw_keyboard(&self) {
        let panel = self.keyboard_panel();
        let kb = &self.keyboard;
        let mut frames: Vec<Frame> = Vec::new();
        let mut prints: Vec<Print> = Vec::new();
        let mut print = |x: f32, y: f32, text: String, color: GlColor, centred: bool| {
            prints.push(Print { x, y, text, color, centred });
        };
        let (left, name_top, name_w, name_h) = panel.name_box();
        let (strip, name_ink, count_ink) = if kb.on_name {
            (BUTTON_SELECTED_COLOR, INK_COLOR, INK_COLOR)
        } else {
            (HEADER_COLOR, TEXT_COLOR, BUTTON_OFF_COLOR)
        };
        frames.push((left, name_top, name_w, name_h, &HEADER_FRAME, strip));
        let name_y = band_text_top(name_top, name_top + name_h);
        let field = state::edited_name();
        let mut caret = None;
        if let Some(f) = &field {
            let text: String = f.text.iter().collect();
            let before: String = f.text[..f.caret].iter().collect();
            caret = Some(left + NAME_INSET + text_width(HELP_FONT, &before) + CARET_GAP);
            let name_end = left + NAME_INSET + text_width(HELP_FONT, &text) + CARET_GAP + CARET_W;
            print(left + NAME_INSET, name_y, text, name_ink, false);
            if let Some(max) = f.max {
                let count = format!("{}/{max}", f.text.len());
                let x = left + name_w - NAME_INSET - text_width(HELP_FONT, &count);
                if x - COUNT_GAP >= name_end {
                    print(x, name_y, count, count_ink, false);
                }
            }
        }
        let mut erase_at = Vec::new();
        for (r, row) in rows().iter().enumerate() {
            for &(key, first, span) in row {
                let (x, y, w, h) = panel.key_box(r, first, span);
                let focused = !kb.on_name && r == kb.row && (first..first + span).contains(&kb.col);
                let lit = key == Key::Shift && kb.shift.capital() && !focused;
                let fill = match (focused, lit) {
                    (true, _) => BUTTON_SELECTED_COLOR,
                    (_, true) => INK_COLOR,
                    _ => BUTTON_OFF_COLOR,
                };
                let ink = if lit { TEXT_COLOR } else { INK_COLOR };
                frames.push((x, y, w, h, &HEADER_FRAME, fill));
                match title(key, kb.shift) {
                    Some(t) => print(x + w / 2.0, band_text_top(y, y + h), t, ink, true),
                    None => erase_at.push((x + w / 2.0, y + h / 2.0, ink)),
                }
            }
        }
        let (cx, cy, k) = (panel.cx, panel.cy, panel.k);
        let screen = |x: f32, y: f32| (cx + x * k, cy + y * k);
        draw_window(cx, cy, panel.w * k, panel.h * k);
        let frames: Vec<Frame> = frames
            .into_iter()
            .map(|(x, y, w, h, style, fill)| {
                let (sx, sy) = screen(x, y);
                (sx, sy, (w * k).max(1.0), (h * k).max(1.0), style, fill)
            })
            .collect();
        draw_frames(&frames);
        draw_batched(|| {
            for &(x, y, color) in &erase_at {
                let (sx, sy) = screen(x, y);
                draw_erase(sx, sy, k, color);
            }
            if let Some(x) = caret {
                let (sx, sy) = screen(x, name_top + CARET_INSET);
                let h = ((name_h - 2.0 * CARET_INSET) * k).round();
                draw_rect(sx.round(), sy.round(), (CARET_W * k).round().max(1.0), h, name_ink);
            }
        });
        with_scale(cx, cy, k, || {
            for p in &prints {
                set_color(p.color);
                if p.centred {
                    draw_text_center(HELP_FONT, p.x, p.y, &p.text);
                } else {
                    draw_text_left(HELP_FONT, p.x, p.y, &p.text);
                }
            }
            set_color(TEXT_COLOR);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::Layout;
    use crate::game::fake::{calls, set_text_input, with_game, Call, Game};
    use crate::input::pad::PadFrame;

    /// Makes the hangar's ship name the field being edited, holding `name` with the caret at
    /// its end.
    fn edit_ship_name(g: &mut Game, name: &str) {
        let menu = g.app.at(CAPP_MAIN_MENU);
        menu.set(MAIN_MENU_OPEN, 1u8);
        let hangar = menu.at(MAIN_MENU_HANGAR);
        hangar.set(HANGAR_OPEN, 1u8);
        let input = hangar.at(HANGAR_NAME_INPUT);
        let text: Vec<u32> = name.chars().map(u32::from).collect();
        g.arena.vector(input, TEXTINPUT_TEXT, &text);
        input.set(TEXTINPUT_CARET, text.len() as i32);
        input.set(TEXTINPUT_ACTIVE, 1u8);
        input.set(TEXTINPUT_MAX, 16i32);
        set_text_input(true);
    }

    /// A layer with a pad in use and the keyboard open on the field being edited.
    fn open_layer() -> Layer {
        let mut l = Layer::default();
        l.pad.index = Some(0);
        l.follow_text_input();
        assert!(l.keyboard.open);
        calls();
        l
    }

    fn typed() -> Vec<Call> {
        calls().into_iter().filter(|c| matches!(c, Call::Type(_) | Call::TextEvent(_))).collect()
    }

    #[test]
    fn every_row_spans_all_columns() {
        for row in rows() {
            let &(_, first, span) = row.last().expect("no row is empty");
            assert_eq!(first + span, COLS);
        }
    }

    #[test]
    fn steps_wrap_and_a_step_up_or_down_keeps_the_column() {
        let mut kb = Keyboard { row: 1, col: 3, ..Keyboard::default() };
        assert_eq!(kb.focused(), Key::Char('r'));
        kb.step((0, 1));
        kb.step((0, 1));
        kb.step((0, 1));
        assert_eq!(kb.focused(), Key::Space);
        kb.step((0, -1));
        assert_eq!(kb.focused(), Key::Char('v'));
        kb.step((0, 1));
        kb.step((1, 0));
        assert_eq!((kb.focused(), kb.col), (Key::Cancel, 6), "a step along lands on the key's first column");
        kb.step((1, 0));
        kb.step((1, 0));
        assert_eq!(kb.focused(), Key::Shift, "a row wraps");
        kb.step((0, 1));
        assert!(kb.on_name, "the name comes after the bottom row");
        kb.step((1, 0));
        assert!(kb.on_name, "a step along stays on the name");
        kb.step((0, 1));
        assert_eq!(kb.focused(), Key::Char('1'), "the top row comes after the name");
        kb.step((0, -1));
        kb.step((0, -1));
        assert_eq!((kb.on_name, kb.focused()), (false, Key::Shift), "the rows wrap");
    }

    #[test]
    fn a_word_starts_with_a_capital_and_shift_turns_it_off_or_locks() {
        let mut kb = Keyboard::default();
        kb.follow_word_start(true);
        assert_eq!(kb.shift, Shift::Word);
        kb.follow_word_start(false);
        assert_eq!(kb.shift, Shift::Off, "the word's start left");
        kb.follow_word_start(true);
        kb.shift = kb.shift.pressed();
        kb.follow_word_start(true);
        assert_eq!(kb.shift, Shift::Off, "turned off by hand at the start");
        kb.shift = kb.shift.pressed();
        kb.follow_word_start(false);
        assert_eq!(kb.shift, Shift::Once, "set by hand: kept off a word's start");
        assert_eq!(kb.shift.pressed(), Shift::Lock);
    }

    #[test]
    fn letters_type_as_capitals_once_and_locked_capitals_stay() {
        with_game(|g| {
            edit_ship_name(g, "");
            let mut l = open_layer();
            l.keyboard.col = 0;
            for _ in 0..2 {
                l.keyboard_press(Input::FaceDown, 0.0);
            }
            l.keyboard.shift = Shift::Lock;
            for _ in 0..2 {
                l.keyboard_press(Input::FaceDown, 0.0);
            }
            assert_eq!(typed(), [Call::Type('A'), Call::Type('a'), Call::Type('A'), Call::Type('A')]);
        });
    }

    #[test]
    fn the_fixed_buttons_erase_space_and_confirm() {
        with_game(|g| {
            edit_ship_name(g, "Kestrel");
            let mut l = open_layer();
            for input in [Input::FaceLeft, Input::L1, Input::FaceUp, Input::R1, Input::Start] {
                l.keyboard_press(input, 0.0);
            }
            assert_eq!(
                typed(),
                [
                    Call::TextEvent(TEXT_BACKSPACE),
                    Call::TextEvent(TEXT_BACKSPACE),
                    Call::Type(' '),
                    Call::Type(' '),
                    Call::TextEvent(TEXT_CONFIRM),
                ]
            );
            assert!(!l.keyboard.open);
        });
    }

    #[test]
    fn on_the_name_left_and_right_move_the_caret_and_a_types_nothing() {
        with_game(|g| {
            edit_ship_name(g, "Kestrel");
            let mut l = open_layer();
            for input in [Input::DpadUp, Input::DpadUp, Input::DpadUp] {
                l.keyboard_press(input, 0.0);
            }
            assert!(l.keyboard.on_name);
            for input in [Input::DpadLeft, Input::LsRight, Input::FaceDown] {
                l.keyboard_press(input, 0.0);
            }
            assert_eq!(typed(), [Call::TextEvent(TEXT_LEFT), Call::TextEvent(TEXT_RIGHT)]);
            l.keyboard_press(Input::DpadDown, 0.0);
            assert_eq!((l.keyboard.on_name, l.keyboard.focused()), (false, Key::Char('5')), "back at its column");
        });
    }

    #[test]
    fn cancel_puts_back_a_changed_name() {
        with_game(|g| {
            edit_ship_name(g, "Ab");
            let mut l = open_layer();
            l.keyboard_press(Input::FaceRight, 0.0);
            assert_eq!(typed(), [Call::TextEvent(TEXT_CANCEL)], "unchanged: left as it is");
            l = open_layer();
            edit_ship_name(g, "Abc");
            l.keyboard_press(Input::FaceRight, 0.0);
            let put_back =
                [Call::TextEvent(TEXT_CLEAR), Call::Type('A'), Call::Type('b'), Call::TextEvent(TEXT_CANCEL)];
            assert_eq!(typed(), put_back);
            assert!(!l.keyboard.open);
        });
    }

    #[test]
    fn a_held_direction_or_erase_repeats_until_released() {
        with_game(|g| {
            edit_ship_name(g, "Kestrel");
            let mut l = open_layer();
            l.cfg.focus_repeat = 1;
            l.pad.held[Input::FaceLeft as usize] = true;
            l.keyboard_press(Input::FaceLeft, 0.0);
            l.repeat_keyboard(0.1);
            l.repeat_keyboard(1.0);
            l.pad.held[Input::FaceLeft as usize] = false;
            l.repeat_keyboard(2.0);
            assert_eq!(typed(), [Call::TextEvent(TEXT_BACKSPACE); 2]);
            let col = l.keyboard.col;
            l.pad.held[Input::LsRight as usize] = true;
            l.keyboard_press(Input::LsRight, 0.0);
            l.repeat_keyboard(1.0);
            assert_eq!(l.keyboard.col, col + 2, "a stick steps as the D-pad does");
        });
    }

    #[test]
    fn the_keyboard_opens_for_a_pad_or_touch_and_closes_with_the_game() {
        with_game(|g| {
            edit_ship_name(g, "Kestrel");
            let mut l = Layer::default();
            l.follow_text_input();
            assert!(!l.keyboard.open, "no pad, no touch");
            l.pad.index = Some(0);
            l.cursor.moved_by_mouse = true;
            l.follow_text_input();
            assert!(!l.keyboard.open, "the mouse moved after the pad");
            l.cursor.moved_by_mouse = false;
            l.follow_text_input();
            assert!(l.keyboard.open);
            assert_eq!(l.keyboard.original.as_deref(), Some("Kestrel"));
            assert_eq!(l.keyboard.shift, Shift::Off, "the caret ends a word");
            set_text_input(false);
            l.follow_text_input();
            assert!(!l.keyboard.open);
        });
    }

    #[test]
    fn the_keyboard_takes_every_press_while_open() {
        with_game(|g| {
            edit_ship_name(g, "");
            let mut l = open_layer();
            l.layout = Layout::parse("[controls]\nb = esc\nstart = space\n").0;
            let mut p = PadFrame::default();
            p.down[Input::FaceDown as usize] = true;
            l.apply_pad(p, 0.0, 0.0);
            let mut p = PadFrame::default();
            p.down[Input::FaceRight as usize] = true;
            l.apply_pad(p, 0.0, 0.0);
            let keys = calls().into_iter().filter(|c| matches!(c, Call::KeyDown(_))).count();
            assert_eq!(keys, 0, "no binding ran");
            assert!(!l.keyboard.open, "B cancelled");
        });
    }

    #[test]
    fn the_counter_gives_way_to_a_name_that_reaches_it() {
        with_game(|g| {
            let prints_left = || calls().into_iter().filter(|c| matches!(c, Call::PrintLeft(..))).count();
            edit_ship_name(g, "Kestrel");
            let l = open_layer();
            l.draw_keyboard();
            assert_eq!(prints_left(), 2, "the name and its counter");
            let wide = "W".repeat(grid_w() as usize);
            edit_ship_name(g, &wide);
            l.draw_keyboard();
            assert_eq!(prints_left(), 1, "the name alone");
        });
    }

    #[test]
    fn a_touch_on_a_key_presses_it_and_off_the_window_passes() {
        with_game(|g| {
            edit_ship_name(g, "");
            let mut l = open_layer();
            let panel = l.keyboard_panel();
            let (x, y, w, h) = panel.key_box(1, 0, 1);
            let at = (panel.cx + (x + w / 2.0) * panel.k, panel.cy + (y + h / 2.0) * panel.k);
            assert!(l.touch_keyboard(at));
            assert_eq!(typed(), [Call::Type('Q')], "a word's first letter");
            assert_eq!((l.keyboard.row, l.keyboard.col), (1, 0), "the touched key takes the focus");
            let (x, y, w, h) = panel.name_box();
            assert!(l.touch_keyboard((panel.cx + (x + w / 2.0) * panel.k, panel.cy + (y + h / 2.0) * panel.k)));
            assert!(l.keyboard.on_name, "a touch on the name focuses it");
            let off = (panel.cx, panel.cy - panel.h * panel.k);
            assert!(!l.touch_keyboard(off));
        });
    }
}
