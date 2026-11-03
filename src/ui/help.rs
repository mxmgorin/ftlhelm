//! The help: a window over the game in pages, turned with L1 and R1 or D-pad left and right,
//! or by a touch on its tab row; any other button, or a touch elsewhere, closes it. On the
//! ship view the game stays paused while it shows, by its own pause or one a wheel hands over.
//!
//! The first page lists the base state's buttons with their tap and hold actions, then what
//! the D-pad and the sticks do; adjacent rows that do the same share one row. The other pages
//! are fixed controls on wheels, weapons, crew and doors; a row naming a built-in shows the
//! button bound to it, and is left out while none is.
//!
//! Buttons show as key caps, the D-pad as a cross with the meant arms dark, a stick as a disc.
//! The window keeps one size across its pages. Everything is laid out in panel units around
//! its centre and drawn `k` times its size: text under a scaled matrix, frames and glyphs on
//! whole screen pixels at the same places.

use super::brand::{draw_mark, draw_name, name_cols, MarkColors, NAME_ROWS};
use super::draw::*;
use super::overlay::{band_text_top, caps_middle, draw_window, panel_scale, HELP_FONT, HELP_LINE, HELP_PAD, TEXT_DY};
use super::text;
use crate::config::layout::{key_name, Action, Binding, Builtin, Input, Slot, SlotMode, SIL_BUTTONS};
use crate::config::settings::{option_name, Settings};
use crate::core::layer::Layer;
use crate::game::actions::toggle_pause;
use crate::game::api::GlColor;
use crate::game::offsets::*;
use crate::game::state;
use crate::input::pad::toggled_mode;

const HELP_RULE_H: f32 = 1.0;
/// A rule sits this far above the top of the row it opens.
const HELP_RULE_GAP: f32 = 2.0;
/// The header row's strip keeps this far in from the window's edges.
const HELP_HEADER_INSET: f32 = 6.0;
const HELP_COL_GAP: f32 = 24.0;
/// The space between the tab row and the header row.
const HELP_TAB_SPACE: f32 = 6.0;
/// The rows besides a page's own: the tabs, the header and the footer.
const HELP_FIXED_ROWS: usize = 3;
/// A tab: its title TAB_PAD in from its sides, tabs TAB_GAP apart.
const TAB_H: f32 = 26.0;
const TAB_PAD: f32 = 10.0;
const TAB_GAP: f32 = 6.0;
/// A key cap: KEY_H tall, its title KEY_PAD in from its sides, at least square.
const KEY_H: f32 = 22.0;
const KEY_PAD: f32 = 5.0;
const KEY_FRAME: FrameStyle = FrameStyle { cut: 3, ..HEADER_FRAME };
/// A rule: a plain strip.
const RULE_FRAME: FrameStyle = FrameStyle { cut: 0, ..HEADER_FRAME };
/// The space between the inputs of one row.
const CHIP_GAP: f32 = 4.0;
/// The D-pad glyph: a cross as tall as a key cap, its arms DPAD_ARM_W wide.
const DPAD_ARM_W: f32 = 8.0;
const PLUS_TEXT: &str = "+";
/// A held click drags: a selection box over the ship, an item on a screen.
const CLICK_HOLD_TITLE: &str = "Drag select";
/// A held pause shows a zoomed view at its lowest level.
const PAUSE_HOLD_TITLE: &str = "Zoom out";
const CURSOR_ROLE: &str = "Cursor";
const ZONE_ROLE: &str = "Next zone";
const BACK_TITLE: &str = "Back";
/// The name in the footer: pixels NAME_CELL across on a header-coloured plate a key cap tall,
/// NAME_PAD cells around them, set off from the byline by NAME_GAP.
const NAME_CELL: f32 = 2.0;
const NAME_PAD: f32 = (KEY_H / NAME_CELL - NAME_ROWS) / 2.0;
const NAME_GAP: f32 = 2.0 * CHIP_GAP;
/// The mark under the table: it reaches this share of the rows' height from the middle of
/// them, its colours mixed from the window's fill toward the brand's by these shares.
const MARK_SHARE: f32 = 0.46;
const MARK_INK: f32 = 0.16;
const MARK_PLATES: f32 = 0.26;
const MARK_PICK: f32 = 0.22;

/// The D-pad's arms as bits of a glyph's mask.
const ARM_LEFT: u8 = 1;
const ARM_UP: u8 = 2;
const ARM_RIGHT: u8 = 4;
const ARM_DOWN: u8 = 8;
const ARMS_SIDE: u8 = ARM_LEFT | ARM_RIGHT;
const ARMS_UPDOWN: u8 = ARM_UP | ARM_DOWN;
const ARMS_ALL: u8 = ARMS_SIDE | ARMS_UPDOWN;

const INPUT_TITLES: [&str; SIL_BUTTONS] =
    ["Guide", "Start", "Select", "Y", "X", "B", "A", "L1", "R1", "L2", "R2", "L3", "R3"];
/// The buttons in the order the first page lists them.
#[rustfmt::skip]
const BUTTON_ORDER: [Input; SIL_BUTTONS] = [
    Input::FaceDown, Input::FaceRight, Input::FaceLeft, Input::FaceUp, Input::L1, Input::R1,
    Input::L2, Input::R2, Input::Start, Input::Select, Input::L3, Input::R3, Input::Home,
];
/// The buttons that turn the pages back and on.
const PAGE_BACK: Input = Input::L1;
const PAGE_ON: Input = Input::R1;

/// What the game's own hotkeys do, as the help names them.
#[rustfmt::skip]
const KEY_TITLES: &[(i32, &str)] = &[
    (KEY_ESCAPE, "Menu"), (KEY_SPACE, "Pause"), ('j' as i32, "Jump"), ('u' as i32, "Upgrades"),
    (KEY_AUTOFIRE, "Autofire"), ('z' as i32, "Open doors"), ('x' as i32, "Close doors"), ('q' as i32, "All crew"),
    (KEY_ENTER, "To stations"), ('/' as i32, "Save stations"), ('i' as i32, "Cargo"),
];

fn builtin_title(b: Builtin) -> &'static str {
    match b {
        Builtin::Cancel => "Cancel",
        Builtin::ContextClick => "Click",
        Builtin::ArmNext => "Next weapon",
        Builtin::ArmAll => "All weapons",
        Builtin::AimCrew => "Next crew",
        Builtin::RepeatAbility => "Repeat ability",
        Builtin::Precision => "Doors mode",
        Builtin::CenterSwap => "Other ship",
        Builtin::PrevTab => "Previous tab",
        Builtin::NextTab => "Next tab",
        Builtin::Help => "Help",
        Builtin::RoomDoors => "Room doors",
        Builtin::Pause => "Pause",
        Builtin::FastForward => "Fast forward",
        Builtin::DpadMode => "Mouse / zones",
        Builtin::Zoom => "Zoom",
        Builtin::TextSize => "Text size",
        Builtin::Autopause => "Autopause",
    }
}

/// Whether an action acts while held: states, doors mode, fast forward.
fn is_hold_action(a: Action) -> bool {
    matches!(a, Action::State(_) | Action::Builtin(Builtin::Precision | Builtin::FastForward))
}

/// Whether the help shows, the page it shows, and whether it holds a pause of its own.
#[derive(Default)]
pub struct Help {
    pub shown: bool,
    page: usize,
    paused: bool,
}

/// What stands for an input in a row's button column.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Chip {
    /// A button's key cap with its title.
    Key(&'static str),
    /// The D-pad with the arms of this mask dark.
    Dpad(u8),
    /// A stick's disc with its side's letter.
    Stick(&'static str),
    /// A plus between the inputs of a combination.
    Plus,
}

const LEFT_STICK: Chip = Chip::Stick("L");
const RIGHT_STICK: Chip = Chip::Stick("R");

/// A table row: its inputs, then a text per column after the button column.
#[derive(Clone, PartialEq, Debug)]
struct Row {
    chips: Vec<Chip>,
    cells: Vec<String>,
}

/// An input a fixed page names: the first button bound to a built-in, every button whose hold
/// opens a wheel, or a fixed chip.
#[derive(Clone, Copy)]
enum Who {
    Bound(Builtin),
    WheelButtons,
    Fixed(Chip),
}

/// A fixed page's row, listed while `shown` holds for the settings.
struct Line {
    who: &'static [Who],
    text: &'static str,
    shown: fn(&Settings) -> bool,
}

fn always(_: &Settings) -> bool {
    true
}

const fn line(who: &'static [Who], text: &'static str) -> Line {
    Line { who, text, shown: always }
}

/// A fixed page: its tab's title, its column heads and its rows.
struct Page {
    title: &'static str,
    head: &'static [&'static str],
    lines: &'static [Line],
}

const A: Who = Who::Bound(Builtin::ContextClick);
const B: Who = Who::Bound(Builtin::Cancel);
const PLUS: Who = Who::Fixed(Chip::Plus);
const SIDE: Who = Who::Fixed(Chip::Dpad(ARMS_SIDE));
const UPDOWN: Who = Who::Fixed(Chip::Dpad(ARMS_UPDOWN));
const DPAD: Who = Who::Fixed(Chip::Dpad(ARMS_ALL));
const UP: Who = Who::Fixed(Chip::Dpad(ARM_UP));
const DOWN: Who = Who::Fixed(Chip::Dpad(ARM_DOWN));
const LSTICK: Who = Who::Fixed(LEFT_STICK);
const RSTICK: Who = Who::Fixed(RIGHT_STICK);

const BUTTONS_TITLE: &str = "Buttons";
const BUTTONS_HEAD: &[&str] = &["BUTTON", "TAP", "HOLD"];
const ACTION_HEAD: &[&str] = &["BUTTON", "ACTION"];

#[rustfmt::skip]
const PAGES: [Page; 4] = [
    Page { title: "Wheels", head: &["BUTTON", "IN A WHEEL"], lines: &[
        line(&[LSTICK, RSTICK], "Point at a plate"),
        line(&[SIDE], "Previous, next plate"),
        line(&[UPDOWN], "Add, take power, step a setting"),
        line(&[A], "Act on the plate"),
        line(&[A, PLUS, SIDE], "Move a weapon or drone"),
        line(&[B], "Close, or back to the wheel before"),
        line(&[Who::WheelButtons], "Close, or switch wheels"),
    ] },
    Page { title: "Weapons", head: ACTION_HEAD, lines: &[
        line(&[Who::Bound(Builtin::ArmNext)], "Arm the next weapon"),
        line(&[A], "Aim at the room"),
        line(&[B], "Cancel the aim"),
        line(&[A, PLUS, SIDE], "Move a weapon or drone"),
        line(&[SIDE], "Beam: turn the line"),
        line(&[UPDOWN], "Beam: slide its start"),
        line(&[A, PLUS, DPAD], "Beam: move the line"),
        line(&[A], "Beam: fire"),
        line(&[B], "Beam: take the start back"),
        line(&[UP], "Wheel: pick for a volley"),
        line(&[DOWN], "Wheel: drop a pick"),
    ] },
    Page { title: "Crew", head: ACTION_HEAD, lines: &[
        line(&[Who::Bound(Builtin::AimCrew)], "Select the next crew member"),
        line(&[A], "On a room: send the crew there"),
        Line { who: &[A], text: "On a room with crew: select them", shown: |c| c.room_crew_pick },
        line(&[B], "Clear the selection"),
        line(&[UPDOWN], "Wheel: add, drop a crew member"),
    ] },
    Page { title: "Doors", head: ACTION_HEAD, lines: &[
        line(&[Who::Bound(Builtin::Precision)], "Held: the doors are the zones"),
        line(&[A], "In doors mode: open, close a door"),
        line(&[Who::Bound(Builtin::RoomDoors)], "Open, close the room's doors"),
    ] },
];
const PAGE_COUNT: usize = PAGES.len() + 1;

/// A page as drawn: its tab's title, its column heads and its rows.
struct Sheet {
    title: &'static str,
    head: &'static [&'static str],
    rows: Vec<Row>,
}

/// An item of the tab row: the key cap turning the pages back, a page's tab, or the key cap
/// turning them on.
#[derive(Clone, Copy, PartialEq, Debug)]
enum TabItem {
    Back,
    Tab(usize),
    On,
}

/// The help window as laid out: its pages, each page's column starts from the table's left
/// edge, its size in panel units, its centre in the frame and its scale.
struct Panel {
    sheets: Vec<Sheet>,
    cols: Vec<Vec<f32>>,
    w: f32,
    h: f32,
    cx: f32,
    cy: f32,
    k: f32,
    /// The title of the button bound to cancel, if one is.
    back_key: Option<&'static str>,
    byline: String,
}

/// One text the help prints, in panel units: its left edge or, when centred, its middle.
struct Print<'a> {
    x: f32,
    y: f32,
    text: &'a str,
    color: GlColor,
    centred: bool,
}

/// What the help draws, in panel units, gathered so each kind is drawn in one go.
#[derive(Default)]
struct Ink<'a> {
    frames: Vec<(f32, f32, f32, f32, &'static FrameStyle, GlColor)>,
    /// Each D-pad glyph's top left corner and dark arms.
    crosses: Vec<(f32, f32, u8)>,
    /// Each stick disc's centre.
    discs: Vec<(f32, f32)>,
    /// The mark's centre and reach, and the name's top left corner.
    mark: (f32, f32, f32),
    name: (f32, f32),
    prints: Vec<Print<'a>>,
}

impl<'a> Ink<'a> {
    fn print(&mut self, x: f32, y: f32, text: &'a str, color: GlColor, centred: bool) {
        self.prints.push(Print { x, y, text, color, centred });
    }

    /// Lays `chips` out from `x` on the row whose top is `y`.
    fn chips(&mut self, chips: &[Chip], mut x: f32, y: f32) {
        let top = caps_middle(y + TEXT_DY) - KEY_H / 2.0;
        for &chip in chips {
            let w = chip_w(chip);
            match chip {
                Chip::Key(title) => {
                    self.frames.push((x, top, w, KEY_H, &KEY_FRAME, INK_COLOR));
                    self.print(x + w / 2.0, y + TEXT_DY, title, TEXT_COLOR, true);
                }
                Chip::Dpad(arms) => self.crosses.push((x, top, arms)),
                Chip::Stick(side) => {
                    self.discs.push((x + w / 2.0, top + KEY_H / 2.0));
                    self.print(x + w / 2.0, y + TEXT_DY, side, TEXT_COLOR, true);
                }
                Chip::Plus => self.print(x, y + TEXT_DY, PLUS_TEXT, INK_COLOR, false),
            }
            x += w + CHIP_GAP;
        }
    }
}

fn chip_w(chip: Chip) -> f32 {
    match chip {
        Chip::Key(title) => (text_width(HELP_FONT, title) + 2.0 * KEY_PAD).max(KEY_H),
        Chip::Dpad(_) | Chip::Stick(_) => KEY_H,
        Chip::Plus => text_width(HELP_FONT, PLUS_TEXT),
    }
}

fn chips_w(chips: &[Chip]) -> f32 {
    chips.iter().map(|&c| chip_w(c) + CHIP_GAP).sum::<f32>() - CHIP_GAP
}

/// The starts of a sheet's columns from the table's left edge, and the table's width.
fn columns(sheet: &Sheet) -> (Vec<f32>, f32) {
    let head_w = |c: usize| sheet.head.get(c).map_or(0.0, |h| text_width(HELP_FONT, h));
    let mut widths = vec![sheet.rows.iter().map(|r| chips_w(&r.chips)).fold(head_w(0), f32::max)];
    for c in 1..sheet.head.len() {
        let cell_w = |r: &Row| r.cells.get(c - 1).map_or(0.0, |t| text_width(HELP_FONT, t));
        widths.push(sheet.rows.iter().map(cell_w).fold(head_w(c), f32::max));
    }
    let mut starts = Vec::new();
    let mut x = 0.0;
    for w in widths {
        starts.push(x);
        x += w + HELP_COL_GAP;
    }
    (starts, x - HELP_COL_GAP)
}

fn tab_w(title: &str) -> f32 {
    text_width(HELP_FONT, title) + 2.0 * TAB_PAD
}

/// The tab row's items with their left edges from the row's left edge and their widths.
fn tab_items(sheets: &[Sheet]) -> Vec<(TabItem, f32, f32)> {
    let mut items = vec![(TabItem::Back, chip_w(Chip::Key(INPUT_TITLES[PAGE_BACK as usize])))];
    items.extend(sheets.iter().enumerate().map(|(i, s)| (TabItem::Tab(i), tab_w(s.title))));
    items.push((TabItem::On, chip_w(Chip::Key(INPUT_TITLES[PAGE_ON as usize]))));
    let mut x = 0.0;
    items
        .into_iter()
        .map(|(item, w)| {
            let placed = (item, x, w);
            x += w + TAB_GAP;
            placed
        })
        .collect()
}

/// Joins adjacent rows with the same cells into one row with the inputs of both.
fn merge_alike(rows: Vec<Row>) -> Vec<Row> {
    let mut merged: Vec<Row> = Vec::new();
    for row in rows {
        match merged.last_mut() {
            Some(last) if last.cells == row.cells => last.chips.extend(row.chips),
            _ => merged.push(row),
        }
    }
    merged
}

impl Panel {
    /// The window's top left corner in panel units.
    fn corner(&self) -> (f32, f32) {
        (-self.w / 2.0, -self.h / 2.0)
    }

    fn tab_row_top(&self) -> f32 {
        self.corner().1 + HELP_PAD
    }

    fn header_top(&self) -> f32 {
        self.tab_row_top() + HELP_LINE + HELP_TAB_SPACE
    }

    /// The tab row's items with their left edges in panel units and their widths.
    fn tabs(&self) -> Vec<(TabItem, f32, f32)> {
        let items = tab_items(&self.sheets);
        let row_w = items.last().map_or(0.0, |&(_, x, w)| x + w);
        items.into_iter().map(|(item, x, w)| (item, x - row_w / 2.0, w)).collect()
    }

    /// The point `(x, y)` of the frame in panel units.
    fn to_panel(&self, (x, y): (f32, f32)) -> (f32, f32) {
        ((x - self.cx) / self.k, (y - self.cy) / self.k)
    }

    /// Gathers what the window shows of page `page`, the window itself aside.
    fn ink(&self, page: usize) -> Ink<'_> {
        let mut ink = Ink::default();
        let (left, top) = self.corner();
        let x0 = left + HELP_PAD;
        let tab_top = self.tab_row_top();
        for (item, x, w) in self.tabs() {
            match item {
                TabItem::Back => ink.chips(&[Chip::Key(INPUT_TITLES[PAGE_BACK as usize])], x, tab_top),
                TabItem::On => ink.chips(&[Chip::Key(INPUT_TITLES[PAGE_ON as usize])], x, tab_top),
                TabItem::Tab(i) => {
                    let fill = if i == page { BUTTON_SELECTED_COLOR } else { BUTTON_OFF_COLOR };
                    let frame_top = tab_top + (HELP_LINE - TAB_H) / 2.0;
                    ink.frames.push((x, frame_top, w, TAB_H, &HEADER_FRAME, fill));
                    let text_y = band_text_top(frame_top, frame_top + TAB_H);
                    ink.print(x + w / 2.0, text_y, self.sheets[i].title, INK_COLOR, true);
                }
            }
        }
        let header_top = self.header_top();
        let header_bottom = header_top + HELP_LINE - HELP_RULE_GAP;
        let header_w = self.w - 2.0 * HELP_HEADER_INSET;
        ink.frames.push((
            left + HELP_HEADER_INSET,
            header_top,
            header_w,
            header_bottom - header_top,
            &HEADER_FRAME,
            HEADER_COLOR,
        ));
        let sheet = &self.sheets[page];
        let cols = &self.cols[page];
        let head_y = band_text_top(header_top, header_bottom);
        for (title, &x) in sheet.head.iter().zip(cols) {
            ink.print(x0 + x, head_y, title, TEXT_COLOR, false);
        }
        let body_top = header_top + HELP_LINE;
        let body_bottom = self.footer_top() - HELP_RULE_GAP;
        ink.mark = (0.0, (body_top + body_bottom) / 2.0, (body_bottom - body_top) * MARK_SHARE);
        let mut line_y = body_top;
        for row in &sheet.rows {
            ink.chips(&row.chips, x0, line_y);
            for (cell, &x) in row.cells.iter().zip(&cols[1..]) {
                ink.print(x0 + x, line_y + TEXT_DY, cell, INK_COLOR, false);
            }
            line_y += HELP_LINE;
        }
        let rule_y = self.footer_top() - HELP_RULE_GAP;
        ink.frames.push((x0, rule_y, self.w - 2.0 * HELP_PAD, HELP_RULE_H, &RULE_FRAME, INK_COLOR));
        let footer_y = band_text_top(rule_y + HELP_RULE_H, top + self.h);
        let cell = self.name_cell();
        let (plate_w, plate_h) = (name_plate_w(cell), (NAME_ROWS + 2.0 * NAME_PAD) * cell);
        let plate_y = caps_middle(footer_y) - plate_h / 2.0;
        ink.frames.push((x0, plate_y, plate_w, plate_h, &KEY_FRAME, HEADER_COLOR));
        ink.name = (x0 + NAME_PAD * cell, plate_y + NAME_PAD * cell);
        ink.print(x0 + plate_w + NAME_GAP, footer_y, &self.byline, TEXT_DISABLED_COLOR, false);
        if let Some(key) = self.back_key {
            let x = -left - HELP_PAD - back_prompt_w(key);
            ink.chips(&[Chip::Key(key)], x, footer_y - TEXT_DY);
            ink.print(x + chip_w(Chip::Key(key)) + CHIP_GAP, footer_y, BACK_TITLE, INK_COLOR, false);
        }
        ink
    }

    /// The side of the name's pixels in panel units: whole screen pixels.
    fn name_cell(&self) -> f32 {
        (NAME_CELL * self.k).round().max(1.0) / self.k
    }

    /// The top of the footer row, under the tallest page's rows.
    fn footer_top(&self) -> f32 {
        let rows = self.sheets.iter().map(|s| s.rows.len()).max().unwrap_or(0);
        self.header_top() + HELP_LINE * (rows + 1) as f32
    }
}

/// The width of the name's plate, its pixels `cell` across.
fn name_plate_w(cell: f32) -> f32 {
    (name_cols() + 2.0 * NAME_PAD) * cell
}

/// The footer's byline: the owner of the project's repository.
fn byline() -> String {
    let owner = env!("CARGO_PKG_REPOSITORY").rsplit('/').nth(1).unwrap_or_default();
    format!("by {owner}")
}

/// The width of the footer's prompt: the back button's key cap and its title.
fn back_prompt_w(key: &'static str) -> f32 {
    chip_w(Chip::Key(key)) + CHIP_GAP + text_width(HELP_FONT, BACK_TITLE)
}

/// Draws a D-pad cross at screen point (`x`, `y`), `side` pixels square with `arm` wide arms,
/// the arms of `dark` in ink and the rest pale grey.
fn draw_cross(x: f32, y: f32, side: f32, arm: f32, dark: u8) {
    let reach = ((side - arm) / 2.0).floor();
    let (x, y) = (x.round(), y.round());
    let mid = (x + reach, y + reach);
    let shade = |bit: u8| if dark & bit != 0 { INK_COLOR } else { BUTTON_OFF_COLOR };
    draw_rect(mid.0, mid.1, arm, arm, INK_COLOR);
    draw_rect(x, mid.1, reach, arm, shade(ARM_LEFT));
    draw_rect(mid.0 + arm, mid.1, reach, arm, shade(ARM_RIGHT));
    draw_rect(mid.0, y, arm, reach, shade(ARM_UP));
    draw_rect(mid.0, mid.1 + arm, arm, reach, shade(ARM_DOWN));
}

impl Layer {
    /// Shows the help on its first page, pausing the ship view, or hides it, resuming a pause
    /// of its own; the first hide clears `help_at_start` in the layout file.
    pub fn show_help(&mut self, shown: bool) {
        if shown && !self.help.shown {
            self.help.page = 0;
            if state::ship_view() && !state::game_paused() {
                toggle_pause();
                self.help.paused = true;
            }
        } else if !shown && self.help.shown {
            if self.help.paused && state::game_paused() {
                toggle_pause();
            }
            self.help.paused = false;
        }
        self.help.shown = shown;
        if !shown && self.cfg.help_at_start {
            self.cfg.help_at_start = false;
            self.save_tuned();
        }
    }

    /// Makes the game's pause the help's own, so the help's close resumes it.
    pub fn help_takes_pause(&mut self) {
        self.help.paused = true;
    }

    /// Takes a press while the help shows: the page buttons and D-pad left and right turn the
    /// pages, D-pad up and down and the sticks do nothing, any other button closes it.
    pub fn help_press(&mut self, input: Input) {
        match input {
            PAGE_BACK | Input::DpadLeft => self.turn_help(-1),
            PAGE_ON | Input::DpadRight => self.turn_help(1),
            _ if (input as usize) < SIL_BUTTONS => self.show_help(false),
            _ => {}
        }
    }

    fn turn_help(&mut self, dir: i32) {
        self.help.page = (self.help.page as i32 + dir).rem_euclid(PAGE_COUNT as i32) as usize;
    }

    /// Takes a touch at frame point `at` while the help shows: a tab shows its page, a page
    /// button's key cap turns the page, a touch elsewhere closes the help.
    pub fn touch_help(&mut self, at: (f32, f32)) {
        let panel = self.help_panel();
        let (x, y) = panel.to_panel(at);
        let row_top = panel.tab_row_top();
        let in_row = y >= row_top && y < row_top + HELP_LINE;
        match panel.tabs().into_iter().find(|&(_, left, w)| in_row && x >= left && x < left + w) {
            Some((TabItem::Back, ..)) => self.turn_help(-1),
            Some((TabItem::On, ..)) => self.turn_help(1),
            Some((TabItem::Tab(i), ..)) => self.help.page = i,
            None => self.show_help(false),
        }
    }

    fn describe(&self, a: Action) -> String {
        match a {
            Action::Key { code, .. } => {
                if let Some(&(_, title)) = KEY_TITLES.iter().find(|(c, _)| *c == code) {
                    title.to_string()
                } else if let Some(name) = key_name(code) {
                    name.to_string()
                } else if (KEY_F1..KEY_F1 + KEY_F_COUNT).contains(&code) {
                    format!("f{}", code - KEY_F1 + 1)
                } else {
                    u8::try_from(code).map_or_else(|_| code.to_string(), |c| (c as char).to_string())
                }
            }
            Action::LClick => "click".to_string(),
            Action::RClick => "right click".to_string(),
            Action::State(s) => format!("{} page", self.layout.states.get(s).map_or("?", |s| &s.name)),
            Action::Wheel(w) => {
                let name = self.layout.wheels.get(w).map_or("?", |w| &w.name);
                let mut chars = name.chars();
                let first: String = chars.next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                format!("{first}{} wheel", chars.as_str())
            }
            Action::Builtin(b) => builtin_title(b).to_string(),
            Action::Set(i) => option_name(i).to_string(),
        }
    }

    /// The tap and hold columns of a binding.
    fn help_row(&self, b: Binding) -> (String, String) {
        match b.tap {
            Some(tap) => (self.describe(tap), self.describe(b.hold)),
            None if b.hold == Action::Builtin(Builtin::ContextClick) => {
                (self.describe(b.hold), CLICK_HOLD_TITLE.to_string())
            }
            None if b.hold == Action::Builtin(Builtin::Pause) => (self.describe(b.hold), PAUSE_HOLD_TITLE.to_string()),
            None if is_hold_action(b.hold) => (String::new(), self.describe(b.hold)),
            None => (self.describe(b.hold), String::new()),
        }
    }

    /// What a pointer source does outside a wheel: moves the cursor, steps between zones, or
    /// acts as keys, which the help leaves out.
    fn pointer_role(&self, slot: Slot) -> Option<&'static str> {
        match toggled_mode(slot, self.slot_mode(slot), self.bind.dpad_mouse) {
            SlotMode::Mouse => Some(CURSOR_ROLE),
            SlotMode::Flick => Some(ZONE_ROLE),
            SlotMode::Keys if slot == Slot::Dpad && self.cfg.snap_dpad => Some(ZONE_ROLE),
            SlotMode::Keys => None,
        }
    }

    /// The first page's rows: the base state's buttons in BUTTON_ORDER, then the D-pad and
    /// the sticks.
    fn button_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(base) = self.layout.states.first() {
            for input in BUTTON_ORDER {
                if let Some(b) = base.bind[input as usize] {
                    let (tap, hold) = self.help_row(b);
                    rows.push(Row { chips: vec![Chip::Key(INPUT_TITLES[input as usize])], cells: vec![tap, hold] });
                }
            }
        }
        let pointers = [(Slot::Dpad, Chip::Dpad(ARMS_ALL)), (Slot::LStick, LEFT_STICK), (Slot::RStick, RIGHT_STICK)];
        for (slot, chip) in pointers {
            if let Some(role) = self.pointer_role(slot) {
                rows.push(Row { chips: vec![chip], cells: vec![role.to_string(), String::new()] });
            }
        }
        merge_alike(rows)
    }

    /// The key caps of the base state's buttons, in BUTTON_ORDER, whose binding `matches`.
    fn buttons_where(&self, matches: impl Fn(Binding) -> bool) -> Vec<Chip> {
        let Some(base) = self.layout.states.first() else {
            return Vec::new();
        };
        BUTTON_ORDER
            .iter()
            .filter(|&&i| base.bind[i as usize].is_some_and(&matches))
            .map(|&i| Chip::Key(INPUT_TITLES[i as usize]))
            .collect()
    }

    /// The key cap of the first button bound to `b`, by tap or hold.
    fn bound_key(&self, b: Builtin) -> Option<Chip> {
        let action = Action::Builtin(b);
        self.buttons_where(|x| x.hold == action || x.tap == Some(action)).first().copied()
    }

    /// A fixed page's rows; a row naming a built-in no button is bound to is left out.
    fn fixed_rows(&self, page: &Page) -> Vec<Row> {
        let row = |l: &Line| {
            let mut chips = Vec::new();
            for &who in l.who {
                match who {
                    Who::Bound(b) => chips.push(self.bound_key(b)?),
                    Who::WheelButtons => {
                        let keys = self.buttons_where(|x| matches!(x.hold, Action::Wheel(_)));
                        if keys.is_empty() {
                            return None;
                        }
                        chips.extend(keys);
                    }
                    Who::Fixed(chip) => chips.push(chip),
                }
            }
            Some(Row { chips, cells: vec![l.text.to_string()] })
        };
        page.lines.iter().filter(|l| (l.shown)(&self.cfg)).filter_map(row).collect()
    }

    fn help_sheets(&self) -> Vec<Sheet> {
        let first = Sheet { title: BUTTONS_TITLE, head: BUTTONS_HEAD, rows: self.button_rows() };
        let fixed = PAGES.iter().map(|p| Sheet { title: p.title, head: p.head, rows: self.fixed_rows(p) });
        std::iter::once(first).chain(fixed).collect()
    }

    /// Lays the help out in the middle of the frame, at the text size as far as the frame
    /// holds it.
    fn help_panel(&self) -> Panel {
        let sheets = self.help_sheets();
        let (cols, widths): (Vec<_>, Vec<_>) = sheets.iter().map(columns).unzip();
        let tabs_w = tab_items(&sheets).last().map_or(0.0, |&(_, x, w)| x + w);
        let back_key = match self.bound_key(Builtin::Cancel) {
            Some(Chip::Key(title)) => Some(title),
            _ => None,
        };
        let byline = byline();
        let prompt_w = back_key.map_or(0.0, |key| HELP_COL_GAP + back_prompt_w(key));
        let footer_w = name_plate_w(NAME_CELL) + NAME_GAP + text_width(HELP_FONT, &byline) + prompt_w;
        let content_w = widths.into_iter().fold(tabs_w.max(footer_w), f32::max);
        let rows = sheets.iter().map(|s| s.rows.len()).max().unwrap_or(0);
        let w = content_w + 2.0 * HELP_PAD;
        let h = 2.0 * HELP_PAD + HELP_TAB_SPACE + HELP_LINE * (rows + HELP_FIXED_ROWS) as f32;
        let (frame_w, frame_h) = (GAME_W as f32, GAME_H as f32);
        let k = panel_scale(text::scale(), w, h, (frame_w, frame_h));
        Panel { sheets, cols, w, h, cx: frame_w / 2.0, cy: frame_h / 2.0, k, back_key, byline }
    }

    /// Draws the help's open page.
    pub fn draw_help(&self) {
        let panel = self.help_panel();
        let ink = panel.ink(self.help.page);
        let (cx, cy, k) = (panel.cx, panel.cy, panel.k);
        let screen = |x: f32, y: f32| (cx + x * k, cy + y * k);
        draw_window(cx, cy, panel.w * k, panel.h * k);
        let (mx, my, reach) = ink.mark;
        let (sx, sy) = screen(mx, my);
        let fill = BUTTON_PALE_COLOR;
        let faint = MarkColors {
            ink: mixed(fill, INK_COLOR, MARK_INK),
            plates: mixed(fill, BUTTON_OFF_COLOR, MARK_PLATES),
            pick: mixed(fill, BUTTON_SELECTED_COLOR, MARK_PICK),
            ground: fill,
        };
        draw_mark(sx, sy, reach * k, &faint);
        let frames: Vec<Frame> = ink
            .frames
            .iter()
            .map(|&(x, y, w, h, style, fill)| {
                let (sx, sy) = screen(x, y);
                (sx, sy, (w * k).max(1.0), (h * k).max(1.0), style, fill)
            })
            .collect();
        draw_frames(&frames);
        let side = (KEY_H * k).round();
        let arm = (DPAD_ARM_W * k).round();
        draw_batched(|| {
            for &(x, y, dark) in &ink.crosses {
                let (sx, sy) = screen(x, y);
                draw_cross(sx, sy, side, arm, dark);
            }
        });
        for &(x, y) in &ink.discs {
            let (sx, sy) = screen(x, y);
            draw_disc(sx.round(), sy.round(), side / 2.0, INK_COLOR);
        }
        let (nx, ny) = screen(ink.name.0, ink.name.1);
        let name_colors = (BUTTON_PALE_COLOR, BUTTON_SELECTED_COLOR);
        draw_name(nx.round(), ny.round(), panel.name_cell() * k, name_colors);
        with_scale(cx, cy, k, || {
            for p in &ink.prints {
                set_color(p.color);
                if p.centred {
                    draw_text_center(HELP_FONT, p.x, p.y, p.text);
                } else {
                    draw_text_left(HELP_FONT, p.x, p.y, p.text);
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
    use crate::game::fake::{calls, with_game, Call};
    use crate::input::bindings::Source;

    fn layer(controls: &str) -> Layer {
        let mut l = Layer::default();
        l.layout = Layout::parse(&format!("[controls]\n{controls}")).0;
        l
    }

    fn row(chips: &[Chip], cells: &[&str]) -> Row {
        Row { chips: chips.to_vec(), cells: cells.iter().map(|c| c.to_string()).collect() }
    }

    #[test]
    fn the_buttons_page_lists_the_pad_in_order_and_joins_rows_alike() {
        let controls = "start = pause\nl3 = dpad_mode\nr3 = dpad_mode\na = context_click\nb = wheel x | cancel\n\
                        left_analog = flick\nright_analog = flick\n[wheel:x]\nkind = action\n";
        let mut l = layer(controls);
        let joined = [Chip::Key("L3"), Chip::Key("R3")];
        let pointers = [Chip::Dpad(ARMS_ALL), LEFT_STICK, RIGHT_STICK];
        assert_eq!(
            l.button_rows(),
            [
                row(&[Chip::Key("A")], &["Click", "Drag select"]),
                row(&[Chip::Key("B")], &["Cancel", "X wheel"]),
                row(&[Chip::Key("Start")], &["Pause", "Zoom out"]),
                row(&joined, &["Mouse / zones", ""]),
                row(&pointers, &["Next zone", ""]),
            ]
        );
        l.bind.dpad_mouse = true;
        assert_eq!(l.button_rows().last(), Some(&row(&pointers, &["Cursor", ""])), "in mouse mode");
        l.bind.dpad_mouse = false;
        l.cfg.snap_dpad = false;
        assert_eq!(l.button_rows().last(), Some(&row(&pointers[1..], &["Next zone", ""])), "a D-pad without zones");
    }

    #[test]
    fn a_fixed_row_names_the_bound_button_and_is_left_out_without_one() {
        let mut l = layer(
            "x = cancel\ny = wheel crew\nback = wheel settings\n[wheel:crew]\nkind = crew\n\
                           [wheel:settings]\nkind = settings\n",
        );
        let wheels = l.fixed_rows(&PAGES[0]);
        assert!(!wheels.iter().any(|r| r.cells[0] == "Act on the plate"), "no button clicks");
        assert!(wheels.contains(&row(&[Chip::Key("X")], &["Close, or back to the wheel before"])));
        let openers = [Chip::Key("Y"), Chip::Key("Select")];
        assert!(wheels.contains(&row(&openers, &["Close, or switch wheels"])));
        assert!(l.fixed_rows(&PAGES[3]).is_empty(), "no doors bindings");
        l.layout = Layout::parse("[controls]\na = context_click\n").0;
        let crew_picks = |l: &Layer| l.fixed_rows(&PAGES[2]).iter().filter(|r| r.chips == [Chip::Key("A")]).count();
        assert_eq!(crew_picks(&l), 2);
        l.cfg.room_crew_pick = false;
        assert_eq!(crew_picks(&l), 1, "a room's crew is not picked");
    }

    #[test]
    fn page_buttons_turn_the_pages_round_and_other_buttons_close_the_help() {
        let mut l = Layer::default();
        l.show_help(true);
        l.help_press(PAGE_BACK);
        assert_eq!(l.help.page, PAGE_COUNT - 1);
        l.help_press(Input::DpadRight);
        l.help_press(PAGE_ON);
        assert_eq!(l.help.page, 1);
        l.help_press(Input::DpadUp);
        l.help_press(Input::LsLeft);
        assert!(l.help.shown && l.help.page == 1);
        l.help_press(Input::FaceDown);
        assert!(!l.help.shown);
        l.show_help(true);
        assert_eq!(l.help.page, 0, "it opens on the first page");
    }

    #[test]
    fn the_byline_names_the_repository_owner() {
        assert_eq!(byline(), "by mxmgorin");
    }

    #[test]
    fn closing_the_help_ends_its_showing_at_start() {
        let mut l = Layer::default();
        l.show_help(true);
        assert!(l.cfg.help_at_start);
        l.show_help(false);
        assert!(!l.help.shown && !l.cfg.help_at_start);
    }

    #[test]
    fn the_help_pauses_the_ship_view_and_resumes_only_its_own_pause() {
        with_game(|g| {
            let mut l = Layer::default();
            l.layout = Layout::parse("[wheel:settings]\nkind = settings\ndown = help\n").0;
            g.player_manager();
            let space = [Call::KeyDown(KEY_SPACE), Call::KeyUp(KEY_SPACE)];
            calls();
            l.show_help(true);
            assert_eq!(calls(), space, "paused");
            g.gui.set(layout().gui_paused, true);
            l.show_help(false);
            assert_eq!(calls(), space, "resumed");
            l.show_help(true);
            calls();
            l.show_help(false);
            assert_eq!(calls(), [], "paused before: left paused");
            g.gui.set(layout().gui_paused, false);
            l.open_wheel(0, Source::Plate);
            assert_eq!(calls(), space, "the wheel pauses");
            g.gui.set(layout().gui_paused, true);
            l.act_on_plate();
            assert!(l.help.shown && l.wheel.open.is_none());
            assert_eq!(calls(), [], "the wheel's pause handed over");
            l.show_help(false);
            assert_eq!(calls(), space);
        });
    }

    #[test]
    fn a_touch_on_a_tab_shows_its_page_and_elsewhere_closes_the_help() {
        with_game(|_| {
            let mut l = layer("l1 = prev_tab\n");
            l.show_help(true);
            let panel = l.help_panel();
            let at = |item: TabItem| {
                let (_, x, w) = panel.tabs().into_iter().find(|t| t.0 == item).expect("every item is laid out");
                let y = panel.tab_row_top() + HELP_LINE / 2.0;
                (panel.cx + (x + w / 2.0) * panel.k, panel.cy + y * panel.k)
            };
            l.touch_help(at(TabItem::Tab(2)));
            assert_eq!(l.help.page, 2);
            l.touch_help(at(TabItem::On));
            assert_eq!(l.help.page, 3);
            l.touch_help((panel.cx, panel.cy));
            assert!(!l.help.shown);
        });
    }
}
