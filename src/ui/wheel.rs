//! Wheels: plates built from the layout and the game, laid out around the middle of the
//! screen or the cursor with icons and power pips, picked by the sticks and the D-pad, and
//! acted on.

use std::f32::consts::{PI, SQRT_2};

use super::draw::*;
use super::glyph::{draw_glyph, Glyph};
use super::overlay::band_text_top;
use super::plates::{
    armament_of, crew_plates, is_knob, layout_plates, show_tuning, swap_glyph, system_plates, times, weapon_slot,
    Armament, AUTOPAUSE_BADGES, AUTO_BADGE, SWAP_BADGES, ZOOM_OFF_BADGE,
};
use super::text::{self, TEXT_SCALE_AUTO, TEXT_SIZES};
use super::zoom::{self, ZOOM_AUTO, ZOOM_MIN};
use crate::config::layout::{Action, Builtin, WheelKind, WHEEL_SLOTS};
use crate::config::settings::{
    option_name, Tuning, AUTOPAUSE_AIM, AUTOPAUSE_ALL, AUTOPAUSE_WHEEL, CREW_MARKER_KEY, FOCUS_BRACKETS_KEY,
    FRAME_FILTER_KEY, HUD_SHIFT_KEY, SHARPEN_KEY, SWAP_BUTTONS_KEY, TOUCH_KEY,
};
use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::game::actions::{tap_key, toggle_pause, MouseButton};
use crate::game::api::{api, GlColor, Ptr};
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;
use crate::input::bindings::Source;
use crate::input::touch;

/// CrewAnimation::RenderIcon draws the face small, centred on the origin.
const CREW_ICON_SCALE: f32 = 2.0;
/// Frames between refills of an open wheel from the game.
const WHEEL_REFRESH_FRAMES: i32 = 6;
const WHEEL_ON: f32 = 0.5;
/// Square plates on a circle, leaving the middle to the names.
const WHEEL_RADIUS: f32 = 125.0;
const WHEEL_TILE: f32 = 60.0;
/// A plate holds an icon box, and under it a row of power pips when the item takes power.
const TILE_ICON: f32 = 40.0;
const PIP_H: f32 = 5.0;
const PIP_GAP: f32 = 2.0;
const PIP_MAX_W: f32 = 9.0;
const CORNER_ICON: f32 = 22.0;
/// A plate's icon, label, pips and corner icon keep CONTENT_GAP inside its frame's slate line;
/// over pips the icon box shrinks to keep that gap above them too.
const CONTENT_GAP: f32 = 3.0;
const CONTENT_INSET: f32 = (PLATE_FRAME.band + PLATE_FRAME.line) as f32 + CONTENT_GAP;
const PIP_ICON: f32 = WHEEL_TILE - 2.0 * CONTENT_INSET - CONTENT_GAP - PIP_H;
const WHEEL_MARGIN: f32 = 8.0;
/// The wheel's middle, an info card: the title on its header strip, under it the pick's name
/// and detail lines. The strip keeps CARD_PAD from the card's edges, as the last line does.
const WHEEL_LINE: f32 = 22.0;
const WHEEL_TEXT_PAD: f32 = 6.0;
const CARD_PAD: f32 = 5.0;
const HEADER_PAD: f32 = 3.0;
const CARD_GAP: f32 = 4.0;
/// The backdrop under the plates reaches this far past them: an octagon or a disc past their
/// sides, a circle past their outer corners.
const WHEEL_DISC_PAD: f32 = 14.0;
const BACKDROP_R: f32 = WHEEL_RADIUS + WHEEL_TILE / 2.0 + WHEEL_DISC_PAD;
const BACKDROP_ROUND_R: f32 = WHEEL_RADIUS + WHEEL_TILE / 2.0 * SQRT_2 + WHEEL_DISC_PAD;
/// The side of the square a wheel's largest backdrop covers.
pub const WHEEL_SIZE: f32 = 2.0 * BACKDROP_ROUND_R;
/// The octagon's cut: a plate's corner keeps as far from a slanted side as a plate's side
/// does from a straight one.
const BACKDROP_CUT: i32 = ((2.0 - SQRT_2) * (WHEEL_RADIUS + WHEEL_DISC_PAD) + 0.5) as i32;
const BACKDROP_FRAME: FrameStyle = FrameStyle { cut: BACKDROP_CUT, ..BOX_FRAME };
/// The round backdrop's band, as wide as the octagon's.
const BACKDROP_BAND: f32 = BOX_FRAME.band as f32;
/// The selected plate grows by this much on each side.
const SELECTED_GROW: f32 = 4.0;
/// Plates as the game's buttons: pale, the pick yellow and grown, a disabled one grey and
/// unlit; a powered one's band green.
const PLATE_FRAME: FrameStyle = FrameStyle { band: 2, ..BUTTON_FRAME };
const PLATE_POWERED_FRAME: FrameStyle = FrameStyle { band_color: POWER_COLOR, ..PLATE_FRAME };
/// A weapon picked to aim: its band twice as wide, band and glow in the game's selection yellow.
const PLATE_PICKED_FRAME: FrameStyle = FrameStyle {
    band: 2 * PLATE_FRAME.band,
    band_color: BUTTON_SELECTED_COLOR,
    glow: Some(BUTTON_SELECTED_COLOR),
    ..PLATE_FRAME
};
const PLATE_DISABLED_FRAME: FrameStyle = FrameStyle { glow: None, ..PLATE_FRAME };

const DISC_COLOR: GlColor = GlColor::new(0.02, 0.06, 0.06, 0.5);
/// The game's event window fill and its dark box fill, both a little see-through.
const WINDOW_COLOR: GlColor = GlColor::new(0.47, 0.39, 0.38, 0.85);
const DARK_COLOR: GlColor = GlColor::new(0.16, 0.14, 0.13, 0.85);
const DIM_COLOR: GlColor = GlColor::new(0.0, 0.0, 0.0, 0.5);
const POWER_COLOR: GlColor = GlColor::new(0.47, 0.95, 0.47, 1.0);
/// What lies under a wheel's plates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backdrop {
    /// An octagon in the game's window frame, filled as its event window.
    #[default]
    Octagon,
    /// The octagon filled as the game's dark boxes.
    OctagonDark,
    /// A circle in the window frame, filled as the event window.
    Circle,
    /// The circle filled as the game's dark boxes.
    CircleDark,
    /// The whole frame dimmed.
    Dim,
    /// A dark disc, half see-through.
    Disc,
}

/// Backdrops by their names in the config.
pub const BACKDROPS: [(&str, Backdrop); 6] = [
    ("octagon", Backdrop::Octagon),
    ("octagon_dark", Backdrop::OctagonDark),
    ("circle", Backdrop::Circle),
    ("circle_dark", Backdrop::CircleDark),
    ("dim", Backdrop::Dim),
    ("disc", Backdrop::Disc),
];

/// Where a wheel opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WheelPlace {
    /// In the middle of the frame part the screen shows.
    #[default]
    Center,
    /// Around the cursor.
    Cursor,
}

/// Wheel places by their names in the config.
pub const WHEEL_PLACES: [(&str, WheelPlace); 2] = [("center", WheelPlace::Center), ("cursor", WheelPlace::Cursor)];

/// Draws `backdrop` under a wheel's plates around (`ox`, `oy`).
fn draw_backdrop(backdrop: Backdrop, (ox, oy): (f32, f32)) {
    let r = BACKDROP_R;
    match backdrop {
        Backdrop::Octagon => draw_frame(ox - r, oy - r, 2.0 * r, 2.0 * r, &BACKDROP_FRAME, WINDOW_COLOR),
        Backdrop::OctagonDark => draw_frame(ox - r, oy - r, 2.0 * r, 2.0 * r, &BACKDROP_FRAME, DARK_COLOR),
        Backdrop::Circle => draw_round_frame(ox, oy, BACKDROP_ROUND_R, BACKDROP_BAND, WINDOW_COLOR),
        Backdrop::CircleDark => draw_round_frame(ox, oy, BACKDROP_ROUND_R, BACKDROP_BAND, DARK_COLOR),
        Backdrop::Dim => draw_rect(0.0, 0.0, GAME_W as f32, GAME_H as f32, DIM_COLOR),
        Backdrop::Disc => draw_disc(ox, oy, r, DISC_COLOR),
    }
}

/// A plate as drawn and acted on, refilled from the layout and the game.
#[derive(Clone, Default)]
pub struct Plate {
    pub bound: bool,
    pub available: bool,
    pub powered: bool,
    /// A weapon picked to aim with the other picked ones.
    pub picked: bool,
    /// A slot with nothing in it, drawn as a faint frame.
    pub empty: bool,
    pub action: Option<Action>,
    pub label: String,
    pub badge: String,
    /// The label's and the badge's widths in the plate font, as measured at the last refill.
    pub(super) widths: (f32, f32),
    pub(super) icon: Option<Ptr>,
    /// The icon's region in texture pixels, when only part of it is the icon.
    pub(super) region: Option<[f32; 4]>,
    /// The icon is a face cut from a game button: its glyph lets the button's slate base show
    /// through.
    pub(super) icon_base: bool,
    /// The icon is drawn at its own size, not fitted to the box: pixel art keeps even lines.
    pub(super) icon_native: bool,
    /// A glyph drawn in place of an icon.
    pub(super) glyph: Option<Glyph>,
    /// How far the art sits right of the texture's centre, in texture pixels.
    pub(super) icon_shift: f32,
    /// A crew member's animation, drawn as the game's portrait in place of an icon.
    pub(super) portrait: Option<Ptr>,
    /// A small icon in the plate's corner, over its main icon.
    pub(super) corner_icon: Option<Ptr>,
    /// Power pips: how many, and how many lit.
    pub(super) pips: i32,
    pub(super) pips_on: i32,
}

/// The open wheel and the plate the sticks point at.
#[derive(Default)]
pub struct WheelUi {
    pub open: Option<usize>,
    pub sel: Option<usize>,
    pub plates: [Plate; WHEEL_SLOTS],
    /// Frames until the open wheel refills from the game.
    pub refresh_wait: i32,
    /// Whether the open wheel paused the game, so its close unpauses.
    paused: bool,
    /// Whether the game was already aiming when the wheel opened; only a new aim closes it.
    pub opened_aiming: bool,
    /// Whether a weapon was picked in the open wheel, whose aim keeps it open.
    pub picking: bool,
    /// The wheel the open one was opened from, and the plate that opened it.
    parent: Option<(usize, usize)>,
    /// The act button is held on a weapon or drone plate, which acts on its release; true
    /// once D-pad left or right was pressed meanwhile, and then it does not act.
    pub grab: Option<bool>,
    /// The open wheel's title and its width, and the size its plate labels fit at, as of the
    /// last refill.
    title: String,
    title_w: f32,
    label_k: f32,
}

impl WheelUi {
    fn pickable(&self, slot: usize) -> bool {
        self.plates[slot].bound && self.plates[slot].available
    }

    /// Whether wheel `index` is open, or the one open was opened from it.
    pub fn open_from(&self, index: usize) -> bool {
        self.open == Some(index) || self.parent.is_some_and(|(w, _)| w == index)
    }
}

/// The point the plates of a wheel opened at (cx, cy) circle, kept inside the screen.
fn wheel_origin(cx: f32, cy: f32) -> (f32, f32) {
    let reach = WHEEL_RADIUS + WHEEL_TILE / 2.0 + WHEEL_MARGIN;
    (clampf(cx, reach, GAME_W as f32 - reach), clampf(cy, reach, GAME_H as f32 - reach))
}

/// The frame area a wheel opened at (cx, cy) covers with its largest backdrop: left, top,
/// right, bottom.
pub fn wheel_area(cx: f32, cy: f32) -> [f32; 4] {
    let (ox, oy) = wheel_origin(cx, cy);
    let r = WHEEL_SIZE / 2.0;
    [ox - r, oy - r, ox + r, oy + r]
}

/// Plate centre for a wheel slot around an origin.
fn plate_center((ox, oy): (f32, f32), slot: usize) -> (f32, f32) {
    let angle = slot as f32 * PI / 4.0;
    (ox + angle.sin() * WHEEL_RADIUS, oy - angle.cos() * WHEEL_RADIUS)
}

/// Draws a plate's icon or portrait centred at cx,cy, an icon fitted to a `size` square.
fn draw_plate_icon(pl: &Plate, cx: f32, cy: f32, size: f32, tint: GlColor) {
    let a = api();
    if let Some(portrait) = pl.portrait {
        // SAFETY: the renderer's matrix stack around the game's own portrait drawing.
        unsafe {
            (a.push_matrix)();
            (a.translate)(cx, cy, 0.0);
            (a.scale)(CREW_ICON_SCALE, CREW_ICON_SCALE, 1.0);
            (a.crew_icon)(portrait, true);
            (a.pop_matrix)();
        }
        return;
    }
    let Some(icon) = pl.icon else {
        return;
    };
    let (tw, th) = texture_size(icon);
    let (sw, sh) = pl.region.map_or((tw, th), |r| (r[2], r[3]));
    if tw <= 0.0 || th <= 0.0 || sw <= 0.0 || sh <= 0.0 {
        return;
    }
    let k = if pl.icon_native { 1.0 } else { size / sw.max(sh) };
    let (w, h) = (sw * k, sh * k);
    let (x, y) = (cx - w / 2.0 - pl.icon_shift * k, cy - h / 2.0);
    match pl.region {
        Some([rx, ry, rw, rh]) => {
            if pl.icon_base {
                draw_rect(x, y, w, h, INK_COLOR);
            }
            // SAFETY: the renderer's blit on a game texture; the region goes in as fractions.
            unsafe {
                (a.blit_partial)(icon, x, y, w, h, rx / tw, (rx + rw) / tw, ry / th, (ry + rh) / th, 1.0, tint, false)
            }
        }
        // SAFETY: as above.
        None => unsafe { (a.blit)(icon, x, y, w, h, 0.0, tint, false) },
    }
}

/// Draws a row of power pips on whole pixels, centred at cx with its top at y, in one batch.
fn draw_pips(n: i32, lit: i32, cx: f32, y: f32, available: bool) {
    let n_f = n as f32;
    let pw = PIP_MAX_W.min((WHEEL_TILE - 2.0 * CONTENT_INSET - (n_f - 1.0) * PIP_GAP) / n_f).trunc();
    let left = (cx - (n_f * pw + (n_f - 1.0) * PIP_GAP) / 2.0).round();
    let y = y.round();
    let x_of = |i: i32| left + i as f32 * (pw + PIP_GAP);
    let fill = if available { POWER_COLOR } else { TEXT_DISABLED_COLOR };
    draw_batched(|| {
        // Every fill before every outline: one run per colour.
        for i in 0..lit.clamp(0, n) {
            draw_rect(x_of(i), y, pw, PIP_H, fill);
        }
        for i in 0..n {
            draw_outline(x_of(i), y, pw, PIP_H, OVERLAY_LINE, INK_COLOR);
        }
    });
}

/// Whether a plate shows its label in place of an icon.
fn shows_label(pl: &Plate) -> bool {
    pl.bound && !pl.empty && pl.available && pl.icon.is_none() && pl.portrait.is_none() && pl.glyph.is_none()
}

/// The size, up to the font's own, at which every label shown in a plate fits its width.
fn label_scale(plates: &[Plate]) -> f32 {
    let room = WHEEL_TILE - 2.0 * CONTENT_INSET;
    plates.iter().filter(|p| shows_label(p)).map(|p| room / p.widths.0).fold(1.0, f32::min)
}

/// Whether a slot shows a plate: an empty or unbound one shows nothing.
fn shows_plate(pl: &Plate) -> bool {
    !pl.empty && pl.bound
}

/// A plate's fill as `selected` or not, by its state.
fn plate_fill(pl: &Plate, selected: bool) -> GlColor {
    if !pl.available {
        BUTTON_OFF_COLOR
    } else if selected {
        BUTTON_SELECTED_COLOR
    } else {
        BUTTON_PALE_COLOR
    }
}

/// The frame of a plate centred at cx,cy: grown while selected, styled by its state.
fn plate_frame(pl: &Plate, cx: f32, cy: f32, selected: bool) -> Frame<'static> {
    let style = if !pl.available {
        &PLATE_DISABLED_FRAME
    } else if pl.picked {
        &PLATE_PICKED_FRAME
    } else if pl.powered {
        &PLATE_POWERED_FRAME
    } else {
        &PLATE_FRAME
    };
    let grow = if pl.available && selected { SELECTED_GROW } else { 0.0 };
    let half = WHEEL_TILE / 2.0 + grow;
    (cx - half, cy - half, 2.0 * half, 2.0 * half, style, plate_fill(pl, selected))
}

/// The size and the centre height of the icon box of a plate centred at cy.
fn icon_box(pl: &Plate, cy: f32) -> (f32, f32) {
    if pl.pips > 0 {
        (PIP_ICON, cy - WHEEL_TILE / 2.0 + CONTENT_INSET + PIP_ICON / 2.0)
    } else {
        (TILE_ICON, cy)
    }
}

/// Draws a plate's icon, portrait or label, its label `label_k` times the font's size, and
/// its corner icon; its glyph and pips are drawn apart.
fn draw_plate_art(pl: &Plate, font: i32, label_k: f32, cx: f32, cy: f32, selected: bool) {
    let (icon, icon_cy) = icon_box(pl, cy);
    if pl.icon.is_some() || pl.portrait.is_some() {
        draw_plate_icon(pl, cx, icon_cy, icon, art_tint(plate_fill(pl, selected)));
    } else if pl.glyph.is_none() && shows_label(pl) {
        set_color(INK_COLOR);
        draw_text_scaled(font, cx, icon_cy, label_k, &pl.label);
        set_color(TEXT_COLOR);
    }
    if let Some(corner) = pl.corner_icon {
        let inset = WHEEL_TILE / 2.0 - CORNER_ICON - CONTENT_INSET;
        let (ix, iy) = (cx + inset, cy + inset);
        draw_rect(ix, iy, CORNER_ICON, CORNER_ICON, INK_COLOR);
        // SAFETY: the renderer's blit on a game texture.
        unsafe { (api().blit)(corner, ix, iy, CORNER_ICON, CORNER_ICON, 0.0, TEXT_COLOR, false) };
    }
}

/// Draws a plate's glyph when it has one and no icon.
fn draw_plate_glyph(pl: &Plate, font: i32, cx: f32, cy: f32) {
    if let (Some(glyph), None, None) = (pl.glyph, pl.icon, pl.portrait) {
        let (icon, icon_cy) = icon_box(pl, cy);
        draw_glyph(glyph, font, cx, icon_cy, icon);
    }
}

/// Draws a plate's power pips when it has any.
fn draw_plate_pips(pl: &Plate, cx: f32, cy: f32) {
    if pl.pips > 0 {
        draw_pips(pl.pips, pl.pips_on, cx, cy + WHEEL_TILE / 2.0 - CONTENT_INSET - PIP_H, pl.available);
    }
}

impl Layer {
    /// Whether a wheel is open around the cursor.
    pub fn wheel_at_cursor(&self) -> bool {
        self.wheel.open.is_some() && self.cfg.wheel_place == WheelPlace::Cursor
    }

    /// The point the open wheel's plates circle: the middle of the frame part the screen
    /// shows, or the cursor, as `wheel_place` sets.
    fn open_wheel_origin(&self) -> (f32, f32) {
        let (x, y) = match self.cfg.wheel_place {
            WheelPlace::Center => self.zoom.view.map_or((GAME_W as f32 / 2.0, GAME_H as f32 / 2.0), |v| v.center()),
            WheelPlace::Cursor => (self.cursor.x, self.cursor.y),
        };
        // On whole pixels: the plates keep their offsets, and their cached shapes.
        let (x, y) = wheel_origin(x, y);
        (x.round(), y.round())
    }

    pub fn draw_wheel(&self) {
        if self.wheel.open.and_then(|w| self.layout.wheels.get(w)).is_none() {
            return;
        }
        let origin = self.open_wheel_origin();
        let (ox, oy) = origin;
        let font = self.plate_font;
        draw_backdrop(self.cfg.wheel_backdrop, origin);
        // The middle: the wheel's title alone, or above the pick's name and detail.
        let sel = self.wheel.sel.map(|s| &self.wheel.plates[s]);
        let lines = match sel {
            None => 1,
            Some(p) if p.badge.is_empty() => 2,
            Some(_) => 3,
        };
        let body_h = if lines > 1 { CARD_GAP + (lines - 2) as f32 * WHEEL_LINE + WHEEL_FONT_H } else { 0.0 };
        let header_h = WHEEL_FONT_H + 2.0 * HEADER_PAD;
        let card_h = 2.0 * CARD_PAD + header_h + body_h;
        let mut text_w = self.wheel.title_w;
        if let Some(p) = sel {
            text_w = text_w.max(p.widths.0).max(p.widths.1);
        }
        let card_w = text_w + 2.0 * (WHEEL_TEXT_PAD + CARD_PAD);
        let (x, y) = ((ox - card_w / 2.0).round(), (oy - card_h / 2.0).round());
        draw_frame(x, y, card_w, card_h, &BOX_FRAME, BUTTON_PALE_COLOR);
        let header_top = y + CARD_PAD;
        draw_frame(x + CARD_PAD, header_top, card_w - 2.0 * CARD_PAD, header_h, &HEADER_FRAME, HEADER_COLOR);
        set_color(TEXT_COLOR);
        draw_text_center(font, ox, band_text_top(header_top, header_top + header_h), &self.wheel.title);
        if let Some(p) = sel {
            let top = y + CARD_PAD + header_h + CARD_GAP;
            set_color(INK_COLOR);
            draw_text_center(font, ox, top, &p.label);
            if !p.badge.is_empty() {
                set_color(HEADER_COLOR);
                draw_text_center(font, ox, top + WHEEL_LINE, &p.badge);
            }
        }
        set_color(TEXT_COLOR);
        // The plates do not overlap: frames by layer, then their art, glyphs and pips.
        let shown = || {
            let sel = self.wheel.sel;
            self.wheel.plates.iter().enumerate().filter(|(_, pl)| shows_plate(pl)).map(move |(i, pl)| {
                let (cx, cy) = plate_center(origin, i);
                (pl, cx, cy, Some(i) == sel)
            })
        };
        let frames: Vec<Frame> = shown().map(|(pl, cx, cy, selected)| plate_frame(pl, cx, cy, selected)).collect();
        draw_frames(&frames);
        for (pl, cx, cy, selected) in shown() {
            draw_plate_art(pl, font, self.wheel.label_k, cx, cy, selected);
        }
        draw_batched(|| shown().for_each(|(pl, cx, cy, _)| draw_plate_glyph(pl, font, cx, cy)));
        draw_batched(|| shown().for_each(|(pl, cx, cy, _)| draw_plate_pips(pl, cx, cy)));
    }

    /// Points the wheel from the stronger stick; the selection stays when both are centred
    /// and while a plate is held.
    pub fn point_wheel(&mut self, left: [f32; 2], right: [f32; 2]) {
        let [x, y] = if right[0].hypot(right[1]) > left[0].hypot(left[1]) { right } else { left };
        if x.hypot(y) < WHEEL_ON || self.wheel.grab.is_some() {
            return;
        }
        // In f64: atan2f carries a glibc symbol version too new for older systems.
        let mut angle = (x as f64).atan2(-y as f64) as f32;
        if angle < 0.0 {
            angle += 2.0 * PI;
        }
        let slot = (angle / (PI / 4.0)).round() as usize % WHEEL_SLOTS;
        if self.wheel.pickable(slot) && Some(slot) != self.wheel.sel {
            self.wheel.sel = Some(slot);
            self.buzz();
        }
    }

    /// Steps the selection to the next pickable plate clockwise (1) or counter-clockwise (-1).
    pub fn step_wheel(&mut self, dir: i32) {
        let n = WHEEL_SLOTS as i32;
        let mut slot = self.wheel.sel.map_or(if dir > 0 { -1 } else { 0 }, |s| s as i32);
        for _ in 0..n {
            slot = (slot + dir + n) % n;
            if self.wheel.pickable(slot as usize) {
                self.wheel.sel = Some(slot as usize);
                self.buzz();
                return;
            }
        }
    }

    /// Moves the held plate's weapon or drone one slot that way, or steps the selection while
    /// no plate is held.
    pub fn wheel_sideways(&mut self, dir: i32) {
        let Some(moved) = self.wheel.grab.as_mut() else {
            self.step_wheel(dir);
            return;
        };
        *moved = true;
        self.move_plate(dir);
    }

    /// Whether the act button on the selected plate waits for its release: a weapon or drone
    /// there may move first.
    pub fn grabs_plate(&self) -> bool {
        self.selected_armament().is_some()
    }

    /// The bar, its control and the slot of the weapon or drone on the selected plate of a
    /// power wheel; None on other plates and on empty slots.
    fn selected_armament(&self) -> Option<(&'static Armament, Obj, usize)> {
        let w = self.wheel.open?;
        let Some(Action::Key { code, shift: false }) = self.wheel.plates[self.wheel.sel?].action else {
            return None;
        };
        let (arm, slot) = armament_of(self.layout.wheels[w].kind, code)?;
        let control = (arm.control)()?;
        state::armament_item(control, slot).map(|_| (arm, control, slot))
    }

    /// Trades the selected plate's weapon or drone with the item in the next slot (1) or the
    /// previous (-1), when that slot has a plate on the wheel; the selection follows it.
    fn move_plate(&mut self, dir: i32) {
        let Some((arm, control, slot)) = self.selected_armament() else {
            return;
        };
        let Some(to) = slot.checked_add_signed(dir as isize).filter(|&to| state::armament_item(control, to).is_some())
        else {
            return;
        };
        let moved = Some(Action::Key { code: arm.first_key + to as i32, shift: false });
        let Some(plate) = self.wheel.plates.iter().position(|p| p.action == moved) else {
            return;
        };
        (arm.move_item)(control, slot, to);
        if arm.volley {
            self.swap_volley_slots(slot, to);
        }
        self.refresh_wheel();
        self.wheel.sel = Some(plate);
        self.buzz();
        if self.debug {
            log!("wheel: slot {slot} moved to {to}");
        }
    }

    /// Fills the open wheel's plates from the layout and the game: crew and systems
    /// wholesale, power and action wheels their availability, badges and icons.
    pub fn refresh_wheel(&mut self) {
        self.wheel.refresh_wait = WHEEL_REFRESH_FRAMES;
        let Some(w) = self.wheel.open.filter(|&w| w < self.layout.wheels.len()) else {
            return;
        };
        let def = &self.layout.wheels[w];
        self.wheel.plates = match def.kind {
            WheelKind::Crew => crew_plates(),
            WheelKind::Systems => system_plates(),
            _ => layout_plates(def, &self.layout.wheels, &mut self.images),
        };
        match def.kind {
            WheelKind::Settings => self.show_settings(),
            WheelKind::Power => self.show_picked(),
            WheelKind::Action => self.show_repeat(),
            _ => {}
        }
        let font = self.plate_font;
        let width = |text: &str| if text.is_empty() { 0.0 } else { text_width(font, text) };
        self.wheel.title = self.layout.wheels[w].name.to_ascii_uppercase();
        self.wheel.title_w = width(&self.wheel.title);
        for pl in &mut self.wheel.plates {
            pl.widths = (width(&pl.label), width(&pl.badge));
        }
        self.wheel.label_k = label_scale(&self.wheel.plates);
    }

    /// Makes the repeat plates pickable once an ability was used from a wheel, its plate's
    /// label as their badge.
    fn show_repeat(&mut self) {
        let last = self.bind.last_ability_key;
        let used = |p: &Plate| matches!(p.action, Some(Action::Key { code, .. }) if Some(code) == last);
        let name = self.wheel.plates.iter().find(|p| used(p)).map(|p| p.label.clone()).unwrap_or_default();
        for pl in &mut self.wheel.plates {
            if pl.action == Some(Action::Builtin(Builtin::RepeatAbility)) {
                pl.available = last.is_some();
                pl.badge.clone_from(&name);
            }
        }
    }

    /// Marks the weapon plates picked to aim.
    fn show_picked(&mut self) {
        let picked = self.picked_weapons();
        for pl in &mut self.wheel.plates {
            let slot = match pl.action {
                Some(Action::Key { code, shift: false }) => weapon_slot(code),
                _ => None,
            };
            pl.picked = pl.powered && slot.is_some_and(|s| picked.contains(&s));
        }
    }

    /// Fills the settings plates' state: lit while on or set by hand, pips for the setting or
    /// the switch, and the value as the badge; a setting is pickable only where it takes effect.
    fn show_settings(&mut self) {
        let cursor = self.bind.dpad_mouse;
        let (zoom, text_scale) = (self.zoom.level, self.cfg.text_scale);
        let shrunk = self.frame_per_window() > 1.0;
        let narrow = zoom::narrower_than_frame();
        let autopause = self.cfg.autopause;
        let swaps = self.face_swaps_apply();
        let touchscreen = touch::touchscreen_present();
        for pl in &mut self.wheel.plates {
            if let Some(Action::Set(i)) = pl.action {
                show_tuning(pl, self.cfg.tuning(i));
                let key = option_name(i);
                if key == SWAP_BUTTONS_KEY {
                    pl.badge = SWAP_BADGES[self.cfg.swap_buttons as usize].to_string();
                    pl.glyph = Some(swap_glyph(self.cfg.swap_buttons));
                }
                pl.available = match key {
                    SHARPEN_KEY => shrunk && self.cfg.frame_filter,
                    FRAME_FILTER_KEY => shrunk,
                    FOCUS_BRACKETS_KEY | CREW_MARKER_KEY => self.cfg.overlay,
                    HUD_SHIFT_KEY => narrow,
                    SWAP_BUTTONS_KEY => swaps,
                    TOUCH_KEY => touchscreen,
                    _ => true,
                };
                continue;
            }
            let Some(Action::Builtin(b)) = pl.action else {
                continue;
            };
            match b {
                Builtin::DpadMode => show_tuning(pl, Tuning::Switch(cursor)),
                Builtin::Autopause => {
                    (pl.powered, pl.pips, pl.pips_on) = (autopause != 0, AUTOPAUSE_ALL, autopause);
                    pl.badge = AUTOPAUSE_BADGES[autopause as usize].to_string();
                }
                Builtin::Zoom => {
                    let level = zoom::resolved_level(zoom, self.zoom.span);
                    pl.powered = zoom != ZOOM_AUTO;
                    (pl.pips, pl.pips_on) = zoom::setting_pips(zoom, self.zoom.span);
                    pl.badge = if !pl.powered {
                        format!("{AUTO_BADGE} {}", times(level))
                    } else if level > ZOOM_MIN {
                        times(level)
                    } else {
                        ZOOM_OFF_BADGE.to_string()
                    };
                }
                Builtin::TextSize => {
                    pl.powered = text_scale != TEXT_SCALE_AUTO;
                    (pl.pips, pl.pips_on) = (TEXT_SIZES, text::size_index(text_scale));
                    pl.badge =
                        if pl.powered { times(text_scale) } else { format!("{AUTO_BADGE} {}", times(text::scale())) };
                }
                _ => {}
            }
        }
    }

    /// Whether the wheel at `index` may open now: a settings wheel anywhere, the others
    /// where wheel_screen allows.
    pub fn wheel_allowed(&self, index: usize) -> bool {
        self.layout.wheels.get(index).is_some_and(|w| w.kind == WheelKind::Settings) || self.wheel_screen()
    }

    /// Whether the open wheel may stay open.
    pub fn open_wheel_allowed(&self) -> bool {
        self.wheel.open.is_some_and(|w| self.wheel_allowed(w))
    }

    /// Whether the open wheel is a settings wheel, which saves the tuned values on its close.
    pub fn settings_open(&self) -> bool {
        self.wheel.open.is_some_and(|w| self.layout.wheels.get(w).is_some_and(|w| w.kind == WheelKind::Settings))
    }

    /// Closes the open wheel, unpausing what it paused unless the help it opened or an aim
    /// taken on it keeps the pause as its own; a settings wheel saves its values.
    pub fn close_wheel(&mut self) {
        let settings = self.settings_open();
        if self.wheel.open.take().is_none() {
            return;
        }
        if self.wheel.paused && state::game_paused() {
            let aiming = state::aiming() || self.actions.volley.is_some();
            if self.help.shown {
                self.help_takes_pause();
            } else if self.cfg.autopauses(AUTOPAUSE_AIM) && aiming {
                self.pad.aim_paused = true;
            } else {
                toggle_pause();
            }
        }
        self.wheel.paused = false;
        self.wheel.sel = None;
        self.wheel.parent = None;
        self.wheel.picking = false;
        self.wheel.grab = None;
        if settings {
            self.save_tuned();
        }
    }

    /// Opens a wheel on its top plate, or the first pickable one clockwise, in place of the
    /// open one; a second press of the open wheel, or of the one it was opened from, closes it.
    pub fn open_wheel(&mut self, index: usize, by: Source) {
        if self.wheel.open_from(index) {
            self.close_wheel();
            return;
        }
        if index >= self.layout.wheels.len() || (self.wheel.open.is_none() && !self.wheel_allowed(index)) {
            return;
        }
        self.wheel.open = Some(index);
        self.wheel.sel = None;
        self.wheel.parent = None;
        self.wheel.opened_aiming = state::aiming();
        self.wheel.picking = false;
        self.wheel.grab = None;
        self.refresh_wheel();
        self.wheel.sel = (0..WHEEL_SLOTS).find(|&i| self.wheel.pickable(i));
        if self.cfg.autopauses(AUTOPAUSE_WHEEL) && state::ship_view() && !state::game_paused() {
            toggle_pause();
            self.wheel.paused = true;
        }
        if self.debug {
            log!("{}: wheel {}", by.name(), self.layout.wheels[index].name);
        }
    }

    /// Opens wheel `index` from the open one's selected plate, which back_wheel returns to.
    fn open_sub_wheel(&mut self, index: usize) {
        let parent = self.wheel.open.zip(self.wheel.sel);
        self.open_wheel(index, Source::Plate);
        if self.wheel.open == Some(index) {
            self.wheel.parent = parent;
        }
    }

    /// Returns to the wheel the open one was opened from, on the plate that opened it; closes
    /// a wheel opened on its own.
    pub fn back_wheel(&mut self) {
        let Some((wheel, slot)) = self.wheel.parent.take() else {
            self.close_wheel();
            return;
        };
        self.open_wheel(wheel, Source::Plate);
        if self.wheel.open == Some(wheel) && self.wheel.pickable(slot) {
            self.wheel.sel = Some(slot);
        }
    }

    /// Performs the selected plate: keys and clicks run as taps, a wheel plate opens its wheel
    /// in place; an action wheel, or a settings wheel off a stepped setting, closes after, the
    /// others refill. The abilities key is remembered for repeat_ability.
    pub fn act_on_plate(&mut self) {
        let (Some(w), Some(sel)) = (self.wheel.open, self.wheel.sel) else {
            return;
        };
        if !self.wheel.pickable(sel) {
            return;
        }
        let kind = self.layout.wheels[w].kind;
        let action = self.wheel.plates[sel].action;
        // A picked weapon is armed already: its key would drop the others picked.
        if self.wheel.plates[sel].picked {
            self.close_wheel();
            return;
        }
        match action {
            // With Shift, a crew member joins the selection.
            Some(Action::Key { code, .. }) if kind == WheelKind::Crew => tap_key(code, state::any_crew_selected()),
            Some(Action::Key { code, shift }) => {
                tap_key(code, shift);
                if kind == WheelKind::Action && state::ability_system(code).is_some() {
                    self.bind.last_ability_key = Some(code);
                }
            }
            Some(Action::LClick) => self.click_tap(MouseButton::Left),
            Some(Action::RClick) => self.click_tap(MouseButton::Right),
            Some(Action::Builtin(b)) => self.perform_tap(Action::Builtin(b), Source::Plate),
            Some(Action::Set(i)) => {
                self.tune(i, None);
                self.buzz();
            }
            Some(Action::Wheel(next)) => {
                self.open_sub_wheel(next);
                return;
            }
            _ => {}
        }
        if self.debug {
            log!("wheel {}: {}", self.layout.wheels[w].name, self.wheel.plates[sel].label);
        }
        let knob = action.is_some_and(is_knob);
        if kind == WheelKind::Action || (kind == WheelKind::Settings && !knob) {
            self.close_wheel();
        } else if self.wheel.open.is_some() {
            self.refresh_wheel();
        }
    }

    /// Acts on the plate of the open wheel at frame point (`x`, `y`); closes the wheel when
    /// no plate is there.
    pub fn touch_wheel(&mut self, x: f32, y: f32) {
        let origin = self.open_wheel_origin();
        let half = WHEEL_TILE / 2.0;
        let slot = (0..WHEEL_SLOTS).find(|&i| {
            let (cx, cy) = plate_center(origin, i);
            (x - cx).abs() <= half && (y - cy).abs() <= half
        });
        match slot {
            Some(slot) if self.wheel.pickable(slot) => {
                self.wheel.sel = Some(slot);
                self.act_on_plate();
            }
            Some(_) => {}
            None => self.close_wheel(),
        }
    }

    /// Adds power to the selected item of a power or systems wheel or an action plate's system, or
    /// takes it for a negative dir, picking or dropping a weapon past its power; on the crew wheel,
    /// adds the member to the selection or drops them; on the settings wheel, steps the setting.
    pub fn adjust_plate(&mut self, dir: i32) {
        let (Some(w), Some(sel)) = (self.wheel.open, self.wheel.sel) else {
            return;
        };
        let kind = self.layout.wheels[w].kind;
        let plate = &self.wheel.plates[sel];
        let Some(action) = plate.action.filter(|_| plate.bound && plate.available) else {
            return;
        };
        if kind == WheelKind::Settings {
            match action {
                Action::Builtin(Builtin::Zoom) => self.zoom_step(dir > 0),
                Action::Builtin(Builtin::TextSize) => self.text_size_step(dir > 0),
                Action::Builtin(Builtin::Autopause) => self.tune_autopause(Some(dir > 0)),
                Action::Builtin(Builtin::DpadMode) => self.bind.dpad_mouse = dir > 0,
                Action::Set(i) => {
                    self.tune(i, Some(dir > 0));
                    self.buzz();
                }
                _ => return,
            }
            self.refresh_wheel();
            return;
        }
        let Action::Key { code, .. } = action else {
            return;
        };
        if kind == WheelKind::Action && state::powered_by(code).is_empty() {
            return;
        }
        let weapon = weapon_slot(code).filter(|_| kind == WheelKind::Power);
        // A powered weapon is picked past its power, and dropped before it loses power.
        if let Some(slot) = weapon.filter(|_| plate.powered && (dir > 0 || plate.picked)) {
            if self.debug {
                let how = if dir > 0 { "picked" } else { "dropped" };
                log!("wheel {}: {} {how}", self.layout.wheels[w].name, plate.label);
            }
            self.pick_weapon(slot, dir > 0);
            self.wheel.picking = true;
            self.refresh_wheel();
            return;
        }
        match kind {
            WheelKind::Crew => {
                let hotkey = (code - KEY_F1) as usize;
                let Some((_, member)) = state::boxed_crew().into_iter().find(|&(i, _)| i == hotkey) else {
                    return;
                };
                if dir > 0 {
                    tap_key(code, state::any_crew_selected());
                } else if let Some(gui) = state::gui() {
                    // SAFETY: DeselectCrew on the GUI's CrewControl with one of its crew.
                    unsafe { (api().deselect_crew)(gui.at(GUI_CREW_CONTROL).ptr(), member.ptr()) };
                }
            }
            WheelKind::Power | WheelKind::Systems | WheelKind::Action => {
                // Powering a weapon disarms the armed one: the picked ones are armed again.
                let picked = if dir > 0 && weapon.is_some() { self.picked_weapons() } else { Vec::new() };
                let all = self.actions.volley.as_ref().is_some_and(|v| v.all);
                tap_key(code, dir < 0);
                if self.debug {
                    log!(
                        "wheel {}: {} power {}",
                        self.layout.wheels[w].name,
                        plate.label,
                        if dir < 0 { "down" } else { "up" }
                    );
                }
                if !picked.is_empty() {
                    self.arm_volley(&picked, all);
                }
            }
            _ => return,
        }
        self.refresh_wheel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::{Input, Layout, LABEL_CHARS};
    use crate::config::settings::{FLICK_ON, SWAP_ALL};
    use crate::game::fake::{calls, with_game, Call, Game, WEAPON_SIZE};
    use crate::ui::draw::rects_of;
    use crate::ui::glyph::{HELP_GLYPH, LETTER_A, LETTER_B, LETTER_X, LETTER_Y, TEXT_GLYPH};
    use crate::ui::plates::{EMPTY_LABEL, OFF_BADGE, ON_BADGE, SWITCH_PIPS, WEAPON_ICON_SHIFT};
    use crate::ui::zoom::View;
    use std::mem::size_of;

    const ENEMY_SHIP_ID: i32 = PLAYER_SHIP_ID + 1;

    /// The plates of the layout's first wheel, opened and refilled from the game.
    fn plates(l: &mut Layer, layout: &str) -> [Plate; WHEEL_SLOTS] {
        l.layout = Layout::parse(layout).0;
        l.wheel.open = Some(0);
        l.refresh_wheel();
        l.wheel.plates.clone()
    }

    /// An armament box holding an item of a blueprint with a literal title and an icon name.
    fn armament(g: &mut Game, title: &str, icon: &str, power: i32, powered: bool) -> Option<Obj> {
        let bp = g.arena.alloc(WEAPONBP_ICON + size_of::<usize>());
        g.literal(bp, BLUEPRINT_SHORT_TITLE, title);
        g.string(bp, WEAPONBP_ICON, icon);
        let item = g.arena.alloc(WEAPON_SIZE);
        item.set(WEAPON_BLUEPRINT, bp.addr());
        item.set(WEAPON_POWER, power);
        item.set(WEAPON_POWERED, powered as u8);
        let armament_box = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
        armament_box.set(ARMAMENTBOX_ITEM, item.addr());
        Some(armament_box)
    }

    #[test]
    fn pips_fill_their_outlines_and_look_alike_wherever_the_row_is() {
        // The most power bars of a system in the game.
        const MOST_PIPS: i32 = 8;
        for n in 1..=MOST_PIPS {
            let row = |cx: f32, y: f32| {
                let rects = rects_of(|| draw_pips(n, n, cx, y, true));
                let [x0, y0, ..] = rects[0].0;
                let local: Vec<_> = rects.iter().map(|&([x, y, w, h], c)| ([x - x0, y - y0, w, h], c)).collect();
                (rects, local)
            };
            let (rects, local) = row(100.3, 40.6);
            assert_eq!(local, row(200.7, 80.2).1, "{n} pips");
            let (fills, outlines) = rects.split_at(n as usize);
            for (fill, bars) in fills.iter().zip(outlines.chunks(outlines.len() / fills.len())) {
                let [x, y, w, h] = fill.0;
                let left = bars.iter().map(|b| b.0[0]).min();
                let top = bars.iter().map(|b| b.0[1]).min();
                let right = bars.iter().map(|b| b.0[0] + b.0[2]).max();
                let bottom = bars.iter().map(|b| b.0[1] + b.0[3]).max();
                assert_eq!((left, top, right, bottom), (Some(x), Some(y), Some(x + w), Some(y + h)), "{n} pips");
            }
        }
    }

    #[test]
    fn plates_go_clockwise_from_the_top() {
        let o = (640.0, 360.0);
        let (x, y) = plate_center(o, 0);
        assert!((x - 640.0).abs() < 1e-3 && (y - (360.0 - WHEEL_RADIUS)).abs() < 1e-3);
        let (x, y) = plate_center(o, 2);
        assert!((x - (640.0 + WHEEL_RADIUS)).abs() < 1e-3 && (y - 360.0).abs() < 1e-3);
    }

    #[test]
    fn a_wheel_opens_in_the_middle_of_the_view_or_at_the_cursor() {
        let mut l = Layer::default();
        (l.cursor.x, l.cursor.y) = (300.0, 300.0);
        assert_eq!(l.open_wheel_origin(), (GAME_W as f32 / 2.0, GAME_H as f32 / 2.0));
        l.zoom.view = Some(View { x: 100.0, y: 50.0, w: 640.0, h: 480.0, k: 1.0 });
        assert_eq!(l.open_wheel_origin(), (420.0, 290.0));
        l.cfg.wheel_place = WheelPlace::Cursor;
        assert_eq!(l.open_wheel_origin(), (300.0, 300.0));
    }

    #[test]
    fn origin_stays_on_screen() {
        let reach = WHEEL_RADIUS + WHEEL_TILE / 2.0 + WHEEL_MARGIN;
        assert_eq!(wheel_origin(0.0, 0.0), (reach, reach));
    }

    #[test]
    fn system_plates_keep_their_slots_and_read_the_power_bar() {
        with_game(|g| {
            let mut system = |kind: i32, power: i32, max: i32| {
                let sys = g.arena.alloc(SYSTEM_ICON + size_of::<usize>());
                sys.set(SYSTEM_TYPE, kind);
                sys.set(SYSTEM_POWER, power);
                sys.set(SYSTEM_POWER_MAX, max);
                let icon = g.arena.alloc(size_of::<usize>());
                sys.set(SYSTEM_ICON, icon.addr());
                let sys_box = g.arena.alloc(SYSBOX_SYSTEM + size_of::<usize>());
                sys_box.set(SYSBOX_SYSTEM, sys.addr());
                Some(sys_box)
            };
            let boxes = [system(SYS_PILOT, 1, 1), system(SYS_CLONEBAY, 0, 1), system(SYS_SHIELDS, 2, 4)];
            let no_system = g.arena.alloc(SYSBOX_SYSTEM + size_of::<usize>());
            let boxes = [boxes[0], None, Some(no_system), boxes[1], boxes[2]];
            g.arena.ptr_vector(g.gui.at(GUI_SYS_CONTROL + SYSCTRL_BOXES), 0, &boxes);
            let plates = plates(&mut Layer::default(), "[wheel:systems]\nkind = systems\n");
            let shields = &plates[0];
            assert!(shields.bound && shields.available && shields.powered && !shields.empty && shields.icon.is_some());
            assert_eq!((shields.label.as_str(), shields.badge.as_str()), ("Shields", "2/4"));
            assert_eq!((shields.pips, shields.pips_on), (4, 2));
            assert_eq!(shields.action, Some(Action::Key { code: 'a' as i32, shift: false }));
            let engines = &plates[1];
            assert!(engines.bound && engines.empty && !engines.available && engines.icon.is_none());
            assert_eq!(engines.label, "Engines");
            let medbay = &plates[2];
            assert!(medbay.available && !medbay.powered);
            assert_eq!((medbay.label.as_str(), medbay.badge.as_str()), ("Clone bay", "0/1"));
            assert!(plates[3..].iter().all(|p| p.empty && !p.available));
        });
    }

    #[test]
    fn power_plates_show_weapons_their_icons_and_empty_slots() {
        with_game(|g| {
            let laser = armament(g, "Burst Laser", "laser_burst", 2, true);
            let ion = armament(g, "Heavy Ion", "ion_heavy", 1, false);
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &[laser, ion]);
            weapons.set(WEAPONCTRL_AUTOFIRE, 1u8);
            let mut l = Layer::default();
            let (laser_icon, fallback) = (g.arena.alloc(size_of::<usize>()), g.arena.alloc(size_of::<usize>()));
            l.images.insert(&format!("{WEAPON_ICON_PREFIX}laser_burst{ICON_SUFFIX}"), laser_icon.addr());
            l.images.insert(&format!("{WEAPON_ICON_PREFIX}ion_heavy{ICON_SUFFIX}"), 0);
            l.images.insert(WEAPON_ICON_FALLBACK, fallback.addr());
            let plates =
                plates(&mut l, "[wheel:power]\nkind = power\nup = 1\nup_right = 2\nright = 3\ndown = v\nleft = 5\n");
            let laser = &plates[0];
            assert!(laser.available && laser.powered && !laser.empty);
            assert_eq!((laser.label.as_str(), laser.pips, laser.pips_on), ("Burst Laser", 2, 2));
            assert_eq!((laser.icon, laser.icon_shift), (Some(laser_icon.ptr()), WEAPON_ICON_SHIFT));
            let ion = &plates[1];
            assert!(ion.available && !ion.powered);
            assert_eq!((ion.label.as_str(), ion.pips, ion.pips_on), ("Heavy Ion", 1, 0));
            assert_eq!((ion.icon, ion.icon_shift), (Some(fallback.ptr()), 0.0));
            for empty in [&plates[2], &plates[6]] {
                assert!(empty.bound && empty.empty && !empty.available);
                assert_eq!(empty.label, EMPTY_LABEL);
            }
            let autofire = &plates[4];
            assert!(autofire.available && autofire.powered && !autofire.empty);
        });
    }

    #[test]
    fn the_all_weapons_plate_waits_for_a_powered_weapon() {
        with_game(|g| {
            let mut l = Layer::default();
            let layout = "[wheel:power]\nkind = power\nup_left = arm_all\n";
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            let ion = armament(g, "Heavy Ion", "ion_heavy", 1, false);
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &[ion]);
            assert!(!plates(&mut l, layout)[7].available);
            let laser = armament(g, "Burst Laser", "laser_burst", 2, true);
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &[laser]);
            assert!(plates(&mut l, layout)[7].available);
        });
    }

    #[test]
    fn the_dpad_picks_powered_weapons_and_drops_them_before_their_power() {
        with_game(|g| {
            let mut l = Layer::default();
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            let boxes = [
                armament(g, "Laser", "laser", 1, true),
                armament(g, "Burst Laser", "laser_burst", 2, true),
                armament(g, "Heavy Ion", "ion_heavy", 1, false),
            ];
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &boxes);
            plates(&mut l, "[wheel:power]\nkind = power\nup = 1\nright = 2\ndown = 3\n");
            let press = |l: &mut Layer, plate, dir| {
                l.wheel.sel = Some(plate);
                l.adjust_plate(dir);
                calls()
                    .into_iter()
                    .filter_map(|c| if let Call::KeyDown(k) = c { Some(k) } else { None })
                    .collect::<Vec<_>>()
            };
            let picked = |l: &Layer| [0, 2, 4].map(|i| l.wheel.plates[i].picked);
            let key = |slot| KEY_FIRST_WEAPON + slot;
            assert_eq!(press(&mut l, 0, 1), [key(0)]);
            assert_eq!(press(&mut l, 2, 1), [key(1), key(0)], "both armed, the first last");
            assert_eq!(picked(&l), [true, true, false]);
            assert!(l.wheel.picking, "the aim keeps the wheel open");
            assert_eq!(press(&mut l, 4, 1), [key(2), key(1), key(0)], "the picked armed again after powering");
            assert_eq!(press(&mut l, 0, -1), [key(1)], "dropped first");
            assert_eq!(picked(&l), [false, true, false]);
            assert_eq!(press(&mut l, 0, -1), [KEY_LSHIFT, key(0)], "then its power taken");
            l.wheel.sel = Some(2);
            l.act_on_plate();
            assert_eq!(l.wheel.open, None, "A on a picked weapon closes the wheel");
            assert_eq!(calls(), [], "without its key, which would drop the others");
        });
    }

    #[test]
    fn a_held_on_a_weapon_plate_moves_it_sideways_and_acts_on_release_only_unmoved() {
        with_game(|g| {
            let mut l = Layer::default();
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            let boxes = [armament(g, "Laser", "laser", 1, true), armament(g, "Heavy Ion", "ion_heavy", 1, true)];
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &boxes);
            let ion = state::armament_item(weapons, 1);
            plates(&mut l, "[wheel:power]\nkind = power\nup = 1\nright = 2\n");
            let a = |l: &mut Layer, down| l.builtin(Builtin::ContextClick, Source::Input(Input::FaceDown), down);
            l.wheel.sel = Some(2);
            a(&mut l, true);
            l.point_wheel([0.0, -1.0], [0.0, 0.0]);
            assert_eq!(l.wheel.sel, Some(2), "the stick does not repoint a held plate");
            l.wheel_sideways(-1);
            assert_eq!(calls(), [Call::Swap(weapons.addr(), 1, 0)]);
            assert_eq!((state::armament_item(weapons, 0), l.wheel.sel), (ion, Some(0)), "the ion first, selected");
            l.wheel_sideways(-1);
            a(&mut l, false);
            assert_eq!(calls(), [], "no slot before the first, and a moved plate does not act");
            plates(&mut l, "[wheel:power]\nkind = power\nup = 1\n");
            l.wheel.sel = Some(0);
            a(&mut l, true);
            l.wheel_sideways(1);
            a(&mut l, false);
            assert_eq!(calls(), [], "no plate for the next slot, so no move");
            a(&mut l, true);
            assert_eq!(calls(), [], "it acts on release");
            a(&mut l, false);
            assert_eq!(calls(), [Call::KeyDown(KEY_FIRST_WEAPON), Call::KeyUp(KEY_FIRST_WEAPON)]);
        });
    }

    #[test]
    fn lockdown_needs_a_living_crystal_and_plates_keep_their_icons() {
        with_game(|g| {
            let mut l = Layer::default();
            let door_icon = g.arena.alloc(size_of::<usize>());
            l.images.insert("door.png", door_icon.addr());
            let layout = "[wheel:actions]\nup = p \"Lockdown\"\nright = z \"Open doors\" door.png@1,2,3,4\n";
            g.player_manager();
            let lockdown_available = |l: &mut Layer| plates(l, layout)[0].available;
            assert!(!lockdown_available(&mut l));
            let crew = [g.crew("Ann", "human", PLAYER_SHIP_ID, false), g.crew("Cy", "crystal", PLAYER_SHIP_ID, true)];
            g.crew_boxes(&crew);
            assert!(!lockdown_available(&mut l));
            crew[1].set(CREW_DEAD, 0u8);
            assert!(lockdown_available(&mut l));
            let doors = &plates(&mut l, layout)[2];
            assert!(doors.available);
            assert_eq!(
                (doors.label.as_str(), doors.icon, doors.region),
                ("Open doors", Some(door_icon.ptr()), Some([1.0, 2.0, 3.0, 4.0]))
            );
        });
    }

    #[test]
    fn long_names_keep_to_the_label_length() {
        with_game(|g| {
            let long = "Heavy Laser Mark Two Prototype";
            let label: String = long.chars().take(LABEL_CHARS).collect();
            let weapon = armament(g, long, "laser", 1, false);
            g.arena.ptr_vector(g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL), ARMAMENT_BOXES, &[weapon]);
            g.player_manager();
            let member = g.crew(long, "human", ENEMY_SHIP_ID, false);
            member.at(CREW_BLUEPRINT + CREWBP_NAME).set(TEXTSTRING_LITERAL, 0u8);
            g.crew_boxes(&[member]);
            let mut l = Layer::default();
            l.images.insert(&format!("{WEAPON_ICON_PREFIX}laser{ICON_SUFFIX}"), 0);
            l.images.insert(WEAPON_ICON_FALLBACK, 0);
            assert_eq!(plates(&mut l, "[wheel:power]\nkind = power\nup = 1\n")[0].label, label);
            let translated: String = format!("[{long}]").chars().take(LABEL_CHARS).collect();
            assert_eq!(plates(&mut l, "[wheel:crew]\nkind = crew\n")[0].label, translated);
        });
    }

    #[test]
    fn settings_plates_show_their_values_and_step_them() {
        with_game(|_| {
            let mut l = Layer::default();
            l.images.insert(POINTER_ICON, 0);
            (l.zoom.level, l.cfg.text_scale, l.bind.dpad_mouse) = (2.0, 1.5, true);
            let layout =
                "[wheel:settings]\nkind = settings\nup = dpad_mode\nright = zoom\nleft = text_size\ndown = help\n";
            let p = plates(&mut l, layout);
            let shown = |p: &Plate| (p.powered, p.pips, p.pips_on, p.badge.clone());
            assert_eq!(shown(&p[0]), (true, SWITCH_PIPS, 1, ON_BADGE.to_string()));
            // The stand-in screen has the whole frame and three levels above it; 2 is the third.
            assert_eq!(shown(&p[2]), (true, 4, 3, "2x".to_string()));
            assert_eq!(shown(&p[6]), (true, TEXT_SIZES, 3, "1.5x".to_string()));
            assert_eq!(shown(&p[4]), (false, 0, 0, String::new()));
            l.wheel.sel = Some(2);
            l.adjust_plate(-1);
            l.adjust_plate(-1);
            assert_eq!(shown(&l.wheel.plates[2]), (true, 4, 1, ZOOM_OFF_BADGE.to_string()));
            l.adjust_plate(-1);
            assert_eq!(shown(&l.wheel.plates[2]), (false, 4, 0, format!("{AUTO_BADGE} 1.33x")));
            l.wheel.sel = Some(6);
            l.adjust_plate(1);
            assert_eq!(l.cfg.text_scale, 1.75);
            l.act_on_plate();
            assert_eq!((l.cfg.text_scale, l.wheel.open), (2.0, Some(0)));
            l.wheel.sel = Some(0);
            l.adjust_plate(-1);
            assert_eq!(shown(&l.wheel.plates[0]), (false, SWITCH_PIPS, 0, OFF_BADGE.to_string()));
            l.adjust_plate(1);
            assert_eq!((l.bind.dpad_mouse, l.wheel.open), (true, Some(0)));
            l.act_on_plate();
            assert_eq!((l.bind.dpad_mouse, l.wheel.open), (false, None));
        });
    }

    #[test]
    fn set_plates_show_and_step_their_settings() {
        with_game(|_| {
            let mut l = Layer::default();
            let layout = "[wheel:settings]\nkind = settings\nup = set overlay\nright = set rumble\n\
                          down = set sharpen\nleft = set speed_level\n";
            let p = plates(&mut l, layout);
            let shown = |p: &Plate| (p.powered, p.pips, p.pips_on, p.badge.clone());
            assert_eq!(shown(&p[0]), (true, SWITCH_PIPS, 1, ON_BADGE.to_string()));
            assert_eq!(shown(&p[2]), (true, 5, 2, "1".to_string()));
            assert_eq!(shown(&p[6]), (true, 6, 6, "6".to_string()));
            let glyphs = [0, 2, 4, 6].map(|i| p[i].glyph);
            assert_eq!(
                glyphs,
                [Some(Glyph::Overlay), Some(Glyph::Rumble), Some(Glyph::Sharpen), Some(Glyph::FastForward)]
            );
            assert!(!p[4].available, "the frame is shown whole");
            l.zoom.view = Some(View { x: 0.0, y: 0.0, w: GAME_W as f32, h: GAME_H as f32, k: 0.5 });
            l.refresh_wheel();
            assert!(l.wheel.plates[4].available, "a shrunk frame is filtered");
            l.wheel.sel = Some(2);
            l.adjust_plate(1);
            assert_eq!(shown(&l.wheel.plates[2]), (true, 5, 3, "1.5".to_string()));
            for _ in 0..4 {
                l.adjust_plate(-1);
            }
            assert_eq!(shown(&l.wheel.plates[2]), (false, 5, 0, OFF_BADGE.to_string()));
            l.wheel.sel = Some(6);
            l.act_on_plate();
            assert_eq!((l.cfg.speed_level, l.wheel.open), (1, Some(0)), "the top cycles to the lowest");
            l.wheel.sel = Some(0);
            l.act_on_plate();
            assert_eq!((l.cfg.overlay, l.wheel.open), (false, None));
        });
    }

    #[test]
    fn settings_plates_take_the_pointer_and_glyphs_unless_given_an_icon() {
        with_game(|g| {
            let mut l = Layer::default();
            let (pointer, door) = (g.arena.alloc(size_of::<usize>()), g.arena.alloc(size_of::<usize>()));
            l.images.insert(POINTER_ICON, pointer.addr());
            l.images.insert("door.png", door.addr());
            let layout = "[wheel:settings]\nkind = settings\nup = dpad_mode\nright = zoom\nleft = text_size\n\
                          down = help\nup_right = dpad_mode \"Mouse\" door.png@1,2,3,4\n";
            let p = plates(&mut l, layout);
            assert_eq!((p[0].icon, p[0].region, p[0].icon_base), (Some(pointer.ptr()), Some(POINTER_REGION), false));
            assert!(p[0].icon_native);
            assert_eq!(p[2].glyph, Some(Glyph::Zoom));
            assert_eq!(p[6].glyph, Some(Glyph::Text(TEXT_GLYPH)));
            assert_eq!(p[4].glyph, Some(Glyph::Text(HELP_GLYPH)));
            assert!(p.iter().all(|pl| !shows_label(pl)));
            assert_eq!((p[1].icon, p[1].icon_base, p[1].glyph), (Some(door.ptr()), true, None));
            assert!(!p[1].icon_native, "a layout icon is fitted");
        });
    }

    #[test]
    fn a_wheel_plate_opens_its_wheel_and_cancel_goes_back() {
        with_game(|_| {
            let mut l = Layer::default();
            let layout = "[wheel:settings]\nkind = settings\nup = set overlay\nleft = wheel view \"View\"\n\
                          [wheel:view]\nkind = settings\nup = set hud_shift\n";
            l.layout = Layout::parse(layout).0;
            let (settings, view) = (0, 1);
            l.open_wheel(settings, Source::Plate);
            assert_eq!(l.wheel.plates[6].glyph, Some(Glyph::Display));
            l.wheel.sel = Some(6);
            l.act_on_plate();
            assert_eq!((l.wheel.open, l.wheel.parent), (Some(view), Some((settings, 6))));
            l.back_wheel();
            assert_eq!((l.wheel.open, l.wheel.sel), (Some(settings), Some(6)), "back on the plate that opened it");
            l.back_wheel();
            assert_eq!(l.wheel.open, None);
            l.open_wheel(settings, Source::Plate);
            l.wheel.sel = Some(6);
            l.act_on_plate();
            l.open_wheel(settings, Source::Plate);
            assert_eq!(l.wheel.open, None, "the settings wheel's own button closes what it opened");
            l.open_wheel(settings, Source::Plate);
            l.wheel.sel = Some(6);
            l.act_on_plate();
            l.wheel.sel = Some(0);
            l.act_on_plate();
            assert_eq!((l.cfg.hud_shift, l.wheel.open, l.wheel.parent), (false, None, None));
        });
    }

    #[test]
    fn view_plates_are_pickable_only_where_they_take_effect() {
        with_game(|_| {
            let mut l = Layer::default();
            let layout = "[wheel:view]\nkind = settings\nup = set focus_brackets\nright = set crew_marker\n\
                          down = set frame_filter\nleft = set hud_shift\nup_left = set ipad_beacons\n";
            let p = plates(&mut l, layout);
            let glyphs = [0, 2, 4, 6, 7].map(|i| p[i].glyph);
            let drawn = [Glyph::Brackets, Glyph::Marker, Glyph::FrameFilter, Glyph::HudShift, Glyph::Beacon].map(Some);
            assert_eq!(glyphs, drawn);
            assert!(p[0].available && p[2].available);
            assert!(!p[4].available, "the frame is shown whole");
            assert!(p[6].available, "the stand-in screen is narrower than the frame");
            l.cfg.overlay = false;
            l.zoom.view = Some(View { x: 0.0, y: 0.0, w: GAME_W as f32, h: GAME_H as f32, k: 0.5 });
            l.refresh_wheel();
            let p = &l.wheel.plates;
            assert!(!p[0].available && !p[2].available, "the overlay is off");
            assert!(p[4].available, "the frame is shrunk");
        });
    }

    #[test]
    fn controls_plates_show_their_steps_and_touch_waits_for_a_touchscreen() {
        with_game(|_| {
            let mut l = Layer::default();
            let layout = "[wheel:controls]\nkind = settings\nup = set focus_repeat\nup_right = set flick_threshold\n\
                          down_right = set swap_buttons\ndown = set touch\ndown_left = set cancel_pause\n\
                          up_left = set tap_hold\n";
            let p = plates(&mut l, layout);
            let glyphs = [0, 1, 3, 4, 5, 7].map(|i| p[i].glyph);
            let drawn = [
                Glyph::Repeat,
                Glyph::Stick,
                Glyph::Buttons(LETTER_A, LETTER_B),
                Glyph::Touch,
                Glyph::CancelPause,
                Glyph::HoldTime,
            ];
            assert_eq!(glyphs, drawn.map(Some));
            let pips = |i: usize| (p[i].pips, p[i].pips_on);
            assert_eq!(pips(0), (3, 2), "repeat: medium of three");
            assert_eq!(pips(1), (6, 3), "stick: lit as sensitivity, more for a lower threshold");
            assert_eq!(pips(7), (5, 2), "hold time: a pip per step from the first");
            assert!(p[3].available, "no SDL mapping names the buttons");
            assert!(!p[4].available, "no touchscreen");
            l.wheel.sel = Some(1);
            l.adjust_plate(1);
            assert_eq!((l.wheel.plates[1].pips_on, l.cfg.flick_threshold < FLICK_ON), (4, true), "up: more sensitive");
        });
    }

    #[test]
    fn the_swap_plate_steps_through_the_pairs() {
        with_game(|_| {
            let mut l = Layer::default();
            let p = plates(&mut l, "[wheel:controls]\nkind = settings\nup = set swap_buttons\n");
            assert_eq!((p[0].powered, p[0].badge.as_str(), p[0].pips), (false, "Off", SWAP_ALL));
            l.wheel.sel = Some(0);
            let mut shown = Vec::new();
            for _ in 0..=SWAP_ALL {
                l.act_on_plate();
                shown.push((l.wheel.plates[0].badge.clone(), l.wheel.plates[0].glyph));
            }
            let (ab, xy) = (Some(Glyph::Buttons(LETTER_A, LETTER_B)), Some(Glyph::Buttons(LETTER_X, LETTER_Y)));
            let names = ["A/B", "X/Y", "Both", "Off"].map(String::from);
            assert_eq!(shown, names.into_iter().zip([ab, xy, ab, ab]).collect::<Vec<_>>());
        });
    }

    #[test]
    fn the_autopause_plate_steps_through_the_modes() {
        with_game(|_| {
            let mut l = Layer::default();
            let p = plates(&mut l, "[wheel:settings]\nkind = settings\nup = autopause\n");
            let shown = (p[0].powered, p[0].badge.as_str(), p[0].pips_on, p[0].glyph);
            assert_eq!(shown, (true, "Wheels", AUTOPAUSE_WHEEL, Some(Glyph::Pause)), "the wheel's pause alone is on");
            l.wheel.sel = Some(0);
            l.adjust_plate(1);
            l.adjust_plate(1);
            assert_eq!((l.cfg.autopause, l.wheel.plates[0].badge.as_str()), (AUTOPAUSE_ALL, "Both"));
            l.adjust_plate(1);
            assert_eq!(l.cfg.autopause, AUTOPAUSE_ALL, "the top mode stays");
            l.act_on_plate();
            assert_eq!((l.cfg.autopause, l.wheel.plates[0].powered), (0, false), "past the top mode, off");
            assert!(l.wheel.open.is_some(), "the wheel stays open");
            l.act_on_plate();
            assert_eq!(l.cfg.autopause, AUTOPAUSE_WHEEL);
        });
    }

    #[test]
    fn the_other_ship_plate_waits_for_an_enemy_ship_in_reach() {
        with_game(|g| {
            let mut l = Layer::default();
            let layout = "[wheel:ship]\nup = center_swap \"Other ship\"\n";
            let p = plates(&mut l, layout);
            assert_eq!((p[0].available, p[0].glyph), (false, Some(Glyph::Swap)));
            let enemy = g.enemy_manager();
            assert!(plates(&mut l, layout)[0].available);
            enemy.set(SHIPMANAGER_TARGETABLE + TARGETABLE_HOSTILE, 0u8);
            assert!(!plates(&mut l, layout)[0].available, "surrendered");
            enemy.set(SHIPMANAGER_PLAYER_CREW, 1u8);
            assert!(plates(&mut l, layout)[0].available, "the player's crew aboard");
        });
    }

    #[test]
    fn a_power_key_plate_shows_its_system_and_steps_its_power() {
        with_game(|g| {
            let mut l = Layer::default();
            let layout = "[wheel:ship]\ndown_right = y \"Artillery\"\n";
            g.player_manager();
            assert!(!plates(&mut l, layout)[3].available, "no artillery aboard");
            g.system(SYS_SHIELDS, 2, 4);
            g.system(SYS_ARTILLERY, 1, 3);
            let artillery = &plates(&mut l, layout)[3];
            assert!(artillery.available && artillery.powered);
            assert_eq!((artillery.pips, artillery.pips_on, artillery.badge.as_str()), (3, 1, "1/3"));
            l.wheel.sel = Some(3);
            let keys = |l: &mut Layer, dir| {
                l.adjust_plate(dir);
                calls()
                    .into_iter()
                    .filter_map(|c| if let Call::KeyDown(k) = c { Some(k) } else { None })
                    .collect::<Vec<_>>()
            };
            assert_eq!(keys(&mut l, 1), ['y' as i32]);
            assert_eq!(keys(&mut l, -1), [KEY_LSHIFT, 'y' as i32]);
            l.act_on_plate();
            assert_eq!(l.bind.last_ability_key, None, "power is no ability to repeat");
        });
    }

    #[test]
    fn the_autofire_plate_is_lit_on_any_wheel_and_drawn_as_a_glyph() {
        with_game(|g| {
            let mut l = Layer::default();
            let layout = "[wheel:ship]\ndown = v \"Autofire\"\n";
            let p = plates(&mut l, layout);
            assert_eq!((p[4].powered, p[4].glyph), (false, Some(Glyph::Autofire)));
            g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL).set(WEAPONCTRL_AUTOFIRE, 1u8);
            assert!(plates(&mut l, layout)[4].powered);
        });
    }

    #[test]
    fn the_repeat_plate_waits_for_an_ability_and_names_it() {
        with_game(|g| {
            let mut l = Layer::default();
            let layout = "[wheel:abilities]\nup = c \"Cloak\"\ndown_left = repeat_ability \"Repeat ability\"\n";
            g.player_manager();
            g.system(SYS_CLOAKING, 1, 1);
            let p = plates(&mut l, layout);
            assert_eq!((p[5].available, p[5].glyph), (false, Some(Glyph::Repeat)));
            l.wheel.sel = Some(0);
            l.act_on_plate();
            assert_eq!(l.bind.last_ability_key, Some('c' as i32));
            let repeat = &plates(&mut l, layout)[5];
            assert_eq!((repeat.available, repeat.badge.as_str()), (true, "Cloak"));
        });
    }

    #[test]
    fn crew_plates_skip_the_dead_and_keep_their_f_keys() {
        with_game(|g| {
            g.player_manager();
            let anim = g.arena.alloc(size_of::<usize>());
            let members = [
                g.crew("Ann", "human", ENEMY_SHIP_ID, false),
                g.crew("Bo", "human", ENEMY_SHIP_ID, true),
                g.crew("Cy", "rock", ENEMY_SHIP_ID, false),
            ];
            members[2].set(CREW_ANIM, anim.addr());
            g.crew_boxes(&members);
            g.arena.ptr_vector(g.gui.at(GUI_CREW_CONTROL + CREW_SELECTED), 0, &[Some(members[2])]);
            let plates = plates(&mut Layer::default(), "[wheel:crew]\nkind = crew\n");
            let key = |n: i32| Some(Action::Key { code: KEY_F1 + n, shift: false });
            let ann = &plates[0];
            assert!(ann.available && !ann.powered && ann.portrait.is_none());
            assert_eq!((ann.label.as_str(), ann.badge.as_str(), ann.action), ("Ann", "Enemy ship", key(0)));
            let cy = &plates[1];
            assert!(cy.available && cy.powered);
            assert_eq!((cy.label.as_str(), cy.action, cy.portrait), ("Cy", key(2), Some(anim.ptr())));
            assert!(plates[2..].iter().all(|p| p.bound && p.empty && !p.available && p.label == EMPTY_LABEL));
        });
    }
}
