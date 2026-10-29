//! Wheels: plates built from the layout and the game, laid out around the middle of the
//! screen or the cursor with icons and power pips, picked by the sticks and the D-pad, and
//! acted on.

use std::f32::consts::{FRAC_1_SQRT_2, PI, SQRT_2};

use super::draw::*;
use super::text::{self, TEXT_SCALE_AUTO, TEXT_SIZES};
use super::zoom::{self, ZOOM_AUTO, ZOOM_MIN};
use crate::config::layout::{truncate_label, Action, Builtin, WheelDef, WheelKind, WHEEL_SLOTS};
use crate::config::settings::{
    is_switch, option_name, Tuning, CREW_MARKER_KEY, FOCUS_BRACKETS_KEY, FRAME_FILTER_KEY, HUD_SHIFT_KEY,
    IPAD_BEACONS_KEY, OVERLAY_KEY, RUMBLE_KEY, SHARPEN_KEY, SPEED_LEVEL_KEY,
};
use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::game::actions::{tap_key, toggle_pause, MouseButton};
use crate::game::api::{api, GlColor, Ptr};
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;
use crate::input::bindings::Source;

/// Weapon art sits right of its texture's centre by this many texture pixels.
const WEAPON_ICON_SHIFT: f32 = 3.0;
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
/// Glyphs of plates without game art: text as wide as the icon box, up to GLYPH_TEXT_MAX
/// times its size; drawn ones in GLYPH_STROKE lines, corner brackets GLYPH_W by GLYPH_H.
const GLYPH_TEXT_MAX: f32 = 2.0;
const TEXT_GLYPH: TextGlyph = TextGlyph { text: "Aa", ink: (-0.5, 0.5) };
const HELP_GLYPH: TextGlyph = TextGlyph { text: "?", ink: (1.0, 1.0) };
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
/// The overlay glyph: layers, a diamond over the edge of one more below it, GLYPH_W by GLYPH_H.
const LAYER_H: f32 = 16.0;
const LAYER_STEP: f32 = GLYPH_H - LAYER_H;
/// The sharpen glyph: a four-point star, a tall diamond across a wide one.
const STAR_LONG: f32 = 30.0;
const STAR_SHORT: f32 = 10.0;
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
/// A plate that opens this wheel shows the display glyph.
const VIEW_WHEEL: &str = "view";
/// The label of a slot with nothing in it.
const EMPTY_LABEL: &str = "Empty";
const ZOOM_OFF_BADGE: &str = "Whole frame";
const AUTO_BADGE: &str = "Auto";
const ON_BADGE: &str = "On";
/// A switch plate's one pip, lit while the switch is on.
const SWITCH_PIPS: i32 = 1;
const OFF_BADGE: &str = "Off";

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

/// Text as a glyph: the text, and how far the font's ink sits off the middle of its advance
/// and line, in font pixels, as measured.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TextGlyph {
    text: &'static str,
    ink: (f32, f32),
}

/// A plate's own glyph, for a built-in or a setting the game has no art for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Glyph {
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
    /// Corner brackets round a crew marker.
    Overlay,
    /// A four-point star.
    Sharpen,
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
}

/// Where a power wheel finds the item a slot key powers.
struct Armament {
    first_key: i32,
    slots: i32,
    control: fn() -> Option<Obj>,
    blueprint: usize,
    powered: usize,
    power: usize,
    icon: usize,
    icon_prefix: &'static str,
    icon_fallback: &'static str,
    icon_shift: f32,
}

const ARMAMENTS: [Armament; 2] = [
    Armament {
        first_key: KEY_FIRST_WEAPON,
        slots: WEAPON_SLOTS,
        control: state::weapon_control,
        blueprint: WEAPON_BLUEPRINT,
        powered: WEAPON_POWERED,
        power: WEAPON_POWER,
        icon: WEAPONBP_ICON,
        icon_prefix: WEAPON_ICON_PREFIX,
        icon_fallback: WEAPON_ICON_FALLBACK,
        icon_shift: WEAPON_ICON_SHIFT,
    },
    Armament {
        first_key: KEY_FIRST_WEAPON + WEAPON_SLOTS,
        slots: DRONE_SLOTS,
        control: state::drone_control,
        blueprint: DRONE_BLUEPRINT,
        powered: DRONE_POWERED,
        power: DRONE_POWER,
        icon: DRONEBP_ICON,
        icon_prefix: DRONE_ICON_PREFIX,
        icon_fallback: DRONE_ICON_FALLBACK,
        icon_shift: 0.0,
    },
];

/// Fixed systems plates in power-bar order; a slot takes the medbay or the clone bay.
#[rustfmt::skip]
const SYSTEM_SLOTS: [(i32, Option<i32>); WHEEL_SLOTS] = [
    (SYS_SHIELDS, None), (SYS_ENGINES, None), (SYS_MEDBAY, Some(SYS_CLONEBAY)), (SYS_OXYGEN, None),
    (SYS_TELEPORTER, None), (SYS_CLOAKING, None), (SYS_MIND, None), (SYS_HACKING, None),
];

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
    icon: Option<Ptr>,
    /// The icon's region in texture pixels, when only part of it is the icon.
    region: Option<[f32; 4]>,
    /// The icon is a face cut from a game button: its glyph lets the button's slate base show
    /// through.
    icon_base: bool,
    /// The icon is drawn at its own size, not fitted to the box: pixel art keeps even lines.
    icon_native: bool,
    /// A glyph drawn in place of an icon.
    glyph: Option<Glyph>,
    /// How far the art sits right of the texture's centre, in texture pixels.
    icon_shift: f32,
    /// A crew member's animation, drawn as the game's portrait in place of an icon.
    portrait: Option<Ptr>,
    /// A small icon in the plate's corner, over its main icon.
    corner_icon: Option<Ptr>,
    /// Power pips: how many, and how many lit.
    pips: i32,
    pips_on: i32,
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

/// Draws a row of power pips centred at cx with its top at y.
fn draw_pips(n: i32, lit: i32, cx: f32, y: f32, available: bool) {
    let n_f = n as f32;
    let pw = PIP_MAX_W.min((WHEEL_TILE - 2.0 * CONTENT_INSET - (n_f - 1.0) * PIP_GAP) / n_f);
    let mut x = cx - (n_f * pw + (n_f - 1.0) * PIP_GAP) / 2.0;
    for i in 0..n {
        if i < lit {
            draw_rect(x, y, pw, PIP_H, if available { POWER_COLOR } else { TEXT_DISABLED_COLOR });
        }
        draw_rect_outline(x, y, pw, PIP_H, INK_COLOR);
        x += pw + PIP_GAP;
    }
}

/// Whether a plate shows its label in place of an icon.
fn shows_label(pl: &Plate) -> bool {
    pl.bound && !pl.empty && pl.available && pl.icon.is_none() && pl.portrait.is_none() && pl.glyph.is_none()
}

/// Draws `glyph` in slate centred at cx,cy, text fitted to `size` wide.
fn draw_glyph(glyph: Glyph, font: i32, cx: f32, cy: f32, size: f32) {
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
        Glyph::Overlay => {
            // On whole pixels: each row stays one line.
            let (cx, top) = (cx.round(), (cy - GLYPH_H / 2.0).round());
            draw_diamond(cx, top + LAYER_H / 2.0, GLYPH_W, LAYER_H, INK_COLOR);
            let below = top + LAYER_STEP + LAYER_H / 2.0;
            draw_chevron(cx, below, GLYPH_W, LAYER_H, GLYPH_STROKE, INK_COLOR);
        }
        Glyph::Sharpen => {
            let (cx, cy) = (cx.round(), cy.round());
            draw_diamond(cx, cy, STAR_SHORT, STAR_LONG, INK_COLOR);
            draw_diamond(cx, cy, STAR_LONG, STAR_SHORT, INK_COLOR);
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
    }
}

/// The size, up to the font's own, at which every label shown in a plate fits its width.
fn label_scale(plates: &[Plate], font: i32) -> f32 {
    let room = WHEEL_TILE - 2.0 * CONTENT_INSET;
    plates.iter().filter(|p| shows_label(p)).map(|p| room / text_width(font, &p.label)).fold(1.0, f32::min)
}

/// Draws a plate, its label `label_k` times the font's size; an empty slot shows nothing.
fn draw_plate(pl: &Plate, font: i32, label_k: f32, cx: f32, cy: f32, selected: bool) {
    let (x, y) = (cx - WHEEL_TILE / 2.0, cy - WHEEL_TILE / 2.0);
    if pl.empty || !pl.bound {
        return;
    }
    let style = if !pl.available {
        &PLATE_DISABLED_FRAME
    } else if pl.picked {
        &PLATE_PICKED_FRAME
    } else if pl.powered {
        &PLATE_POWERED_FRAME
    } else {
        &PLATE_FRAME
    };
    let (fill, grow) = if !pl.available {
        (BUTTON_OFF_COLOR, 0.0)
    } else if selected {
        (BUTTON_SELECTED_COLOR, SELECTED_GROW)
    } else {
        (BUTTON_PALE_COLOR, 0.0)
    };
    let side = WHEEL_TILE + 2.0 * grow;
    draw_frame(x - grow, y - grow, side, side, style, fill);
    let (icon, icon_cy) = if pl.pips > 0 { (PIP_ICON, y + CONTENT_INSET + PIP_ICON / 2.0) } else { (TILE_ICON, cy) };
    if pl.icon.is_some() || pl.portrait.is_some() {
        draw_plate_icon(pl, cx, icon_cy, icon, art_tint(fill));
    } else if let Some(glyph) = pl.glyph {
        draw_glyph(glyph, font, cx, icon_cy, icon);
    } else if shows_label(pl) {
        set_color(INK_COLOR);
        draw_text_scaled(font, cx, icon_cy, label_k, &pl.label);
        set_color(TEXT_COLOR);
    }
    if let Some(corner) = pl.corner_icon {
        let (ix, iy) = (x + WHEEL_TILE - CORNER_ICON - CONTENT_INSET, y + WHEEL_TILE - CORNER_ICON - CONTENT_INSET);
        draw_rect(ix, iy, CORNER_ICON, CORNER_ICON, INK_COLOR);
        // SAFETY: the renderer's blit on a game texture.
        unsafe { (api().blit)(corner, ix, iy, CORNER_ICON, CORNER_ICON, 0.0, TEXT_COLOR, false) };
    }
    if pl.pips > 0 {
        draw_pips(pl.pips, pl.pips_on, cx, y + WHEEL_TILE - CONTENT_INSET - PIP_H, pl.available);
    }
}

/// Sets a crew plate's corner icon and badge from the system in the member's room, or the
/// badge alone when the member is aboard the other ship.
fn crew_location(pl: &mut Plate, member: Obj) {
    if member.i32(CREW_SHIP) != PLAYER_SHIP_ID {
        pl.badge = "Enemy ship".to_string();
        return;
    }
    if let Some(sys) = state::system_in_room(member.i32(CREW_ROOM)) {
        pl.corner_icon = sys.obj(SYSTEM_ICON).map(Obj::ptr);
        pl.badge = state::system_name(sys.i32(SYSTEM_TYPE)).to_string();
    }
}

/// A weapon or drone plate's icon from its blueprint's icon name, and its pips.
fn armament_plate(images: &mut Images, pl: &mut Plate, arm: &Armament, item: Obj, bp: Obj) {
    let path = format!("{}{}{}", arm.icon_prefix, bp.string(arm.icon), ICON_SUFFIX);
    pl.icon = images.get(&path);
    pl.icon_shift = arm.icon_shift;
    if pl.icon.is_none() {
        pl.icon = images.get(arm.icon_fallback);
        pl.icon_shift = 0.0;
    }
    pl.region = None;
    pl.pips = item.i32(arm.power);
    pl.pips_on = if pl.powered { pl.pips } else { 0 };
}

/// A number as a badge shows it: two decimals at most, no trailing zeros.
fn badge_number(k: f32) -> String {
    let digits = format!("{k:.2}");
    digits.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A scale as a badge shows it: the number, then "x".
fn times(k: f32) -> String {
    format!("{}x", badge_number(k))
}

/// Whether a settings plate's action steps a setting by the D-pad, the wheel staying open.
fn is_knob(action: Action) -> bool {
    match action {
        Action::Builtin(b) => matches!(b, Builtin::Zoom | Builtin::TextSize),
        Action::Set(i) => !is_switch(i),
        _ => false,
    }
}

/// Shows a setting on its plate: lit while on, a pip per step or the switch's one, and the
/// value as the badge.
fn show_tuning(pl: &mut Plate, tuning: Tuning) {
    match tuning {
        Tuning::Switch(on) => {
            pl.powered = on;
            (pl.pips, pl.pips_on) = (SWITCH_PIPS, on as i32);
            pl.badge = if on { ON_BADGE } else { OFF_BADGE }.to_string();
        }
        Tuning::Steps { value, step, max } => {
            pl.powered = value > 0.0;
            (pl.pips, pl.pips_on) = ((max / step).round() as i32, (value / step).round() as i32);
            pl.badge = if pl.powered { badge_number(value) } else { OFF_BADGE.to_string() };
        }
    }
}

/// The glyph of a `set` plate whose setting has a look of its own.
fn setting_glyph(key: &str) -> Option<Glyph> {
    match key {
        OVERLAY_KEY => Some(Glyph::Overlay),
        RUMBLE_KEY => Some(Glyph::Rumble),
        SHARPEN_KEY => Some(Glyph::Sharpen),
        SPEED_LEVEL_KEY => Some(Glyph::FastForward),
        FOCUS_BRACKETS_KEY => Some(Glyph::Brackets),
        CREW_MARKER_KEY => Some(Glyph::Marker),
        HUD_SHIFT_KEY => Some(Glyph::HudShift),
        FRAME_FILTER_KEY => Some(Glyph::FrameFilter),
        IPAD_BEACONS_KEY => Some(Glyph::Beacon),
        _ => None,
    }
}

fn crew_plates() -> [Plate; WHEEL_SLOTS] {
    let mut plates: [Plate; WHEEL_SLOTS] = Default::default();
    let mut slot = 0;
    for i in 0..CREW_HOTKEYS {
        if slot == WHEEL_SLOTS {
            break;
        }
        let Some(member) = state::crew_at(i) else {
            break;
        };
        if state::crew_dead(member) {
            continue;
        }
        let pl = &mut plates[slot];
        slot += 1;
        pl.bound = true;
        pl.available = true;
        pl.powered = state::crew_is_selected(member);
        pl.portrait = member.obj(CREW_ANIM).map(Obj::ptr);
        crew_location(pl, member);
        pl.action = Some(Action::Key { code: KEY_F1 + i as i32, shift: false });
        pl.label = truncate_label(&state::display_text(member.at(CREW_BLUEPRINT + CREWBP_NAME)));
    }
    // Empty slots keep their places, so positions never shift.
    for pl in &mut plates[slot..] {
        pl.bound = true;
        pl.empty = true;
        pl.label = EMPTY_LABEL.to_string();
    }
    plates
}

fn system_plates() -> [Plate; WHEEL_SLOTS] {
    let mut plates: [Plate; WHEEL_SLOTS] = Default::default();
    let boxes = state::gui().map(|g| g.at(GUI_SYS_CONTROL + SYSCTRL_BOXES));
    let systems: Vec<Obj> =
        boxes.map(|b| b.vec_objs().filter_map(|(_, sys_box)| sys_box.obj(SYSBOX_SYSTEM)).collect()).unwrap_or_default();
    for (pl, &(kind, alt)) in plates.iter_mut().zip(SYSTEM_SLOTS.iter()) {
        pl.bound = true;
        pl.empty = true;
        pl.label = state::system_name(kind).to_string();
        pl.action = state::system_key(kind).map(|code| Action::Key { code, shift: false });
        let Some(sys) = systems.iter().find(|s| s.i32(SYSTEM_TYPE) == kind || Some(s.i32(SYSTEM_TYPE)) == alt) else {
            continue;
        };
        pl.available = true;
        pl.empty = false;
        pl.powered = sys.i32(SYSTEM_POWER) > 0;
        pl.icon = sys.obj(SYSTEM_ICON).map(Obj::ptr);
        pl.pips = sys.i32(SYSTEM_POWER_MAX);
        pl.pips_on = sys.i32(SYSTEM_POWER);
        pl.label = state::system_name(sys.i32(SYSTEM_TYPE)).to_string();
        pl.badge = format!("{}/{}", pl.pips_on, pl.pips);
    }
    plates
}

/// Gives the plate of built-in `b` its art when it has a look of its own: the game's pointer
/// for the mouse mode, glyphs for the zoom, the text size and the help.
fn builtin_art(images: &mut Images, pl: &mut Plate, b: Builtin) {
    match b {
        Builtin::DpadMode => {
            pl.icon = images.get(POINTER_ICON);
            pl.region = Some(POINTER_REGION);
            pl.icon_native = true;
        }
        Builtin::Zoom => pl.glyph = Some(Glyph::Zoom),
        Builtin::TextSize => pl.glyph = Some(Glyph::Text(TEXT_GLYPH)),
        Builtin::Help => pl.glyph = Some(Glyph::Text(HELP_GLYPH)),
        Builtin::CenterSwap => pl.glyph = Some(Glyph::Swap),
        Builtin::Autopause => pl.glyph = Some(Glyph::Pause),
        Builtin::ArmAll => pl.icon = images.get(CROSSHAIR_ICON),
        _ => {}
    }
}

/// The weapon slot a key arms, if it is a weapon's.
fn weapon_slot(code: i32) -> Option<usize> {
    (KEY_FIRST_WEAPON..KEY_FIRST_WEAPON + WEAPON_SLOTS).contains(&code).then(|| (code - KEY_FIRST_WEAPON) as usize)
}

/// Whether an armament control holds a powered item.
fn any_powered(control: Obj) -> bool {
    (0..WEAPON_SLOTS as usize).any(|i| state::armament_item(control, i).is_some_and(|w| w.flag(WEAPON_POWERED)))
}

/// The plates of `def`, a wheel of `wheels`.
fn layout_plates(def: &WheelDef, wheels: &[WheelDef], images: &mut Images) -> [Plate; WHEEL_SLOTS] {
    let mut plates: [Plate; WHEEL_SLOTS] = Default::default();
    for (pl, pdef) in plates.iter_mut().zip(def.plates.iter()) {
        let Some(pdef) = pdef else {
            continue;
        };
        pl.bound = true;
        pl.available = true;
        pl.action = Some(pdef.action);
        pl.label = pdef.label.clone();
        if let Some(icon) = &pdef.icon {
            pl.icon = images.get(&icon.path);
            pl.region = icon.region;
            pl.icon_base = icon.region.is_some();
        } else if let Action::Builtin(b) = pdef.action {
            builtin_art(images, pl, b);
        } else if let Action::Set(i) = pdef.action {
            pl.glyph = setting_glyph(option_name(i));
        } else if let Action::Wheel(w) = pdef.action {
            pl.glyph = wheels.get(w).filter(|w| w.name == VIEW_WHEEL).map(|_| Glyph::Display);
        }
        match pdef.action {
            Action::Builtin(Builtin::CenterSwap) => pl.available = state::enemy_ship().is_some(),
            Action::Builtin(Builtin::ArmAll) => pl.available = state::weapon_control().is_some_and(any_powered),
            _ => {}
        }
        let Action::Key { code, .. } = pdef.action else {
            continue;
        };
        let arm = ARMAMENTS
            .iter()
            .find(|a| def.kind == WheelKind::Power && code >= a.first_key && code < a.first_key + a.slots);
        if let Some(arm) = arm {
            let item = (arm.control)().and_then(|c| state::armament_item(c, (code - arm.first_key) as usize));
            let bp = item.and_then(|i| i.obj(arm.blueprint));
            pl.available = bp.is_some();
            pl.empty = bp.is_none();
            match (item, bp) {
                (Some(item), Some(bp)) => {
                    pl.powered = item.flag(arm.powered);
                    pl.label = truncate_label(&state::display_text(bp.at(BLUEPRINT_SHORT_TITLE)));
                    armament_plate(images, pl, arm, item, bp);
                }
                _ => pl.label = EMPTY_LABEL.to_string(),
            }
        } else if def.kind == WheelKind::Power && code == KEY_AUTOFIRE {
            pl.powered = state::weapon_control().is_some_and(|c| c.flag(WEAPONCTRL_AUTOFIRE));
        } else if def.kind == WheelKind::Action {
            if let Some(sys) = state::ability_system(code) {
                pl.available = state::has_system(sys);
            } else if code == KEY_LOCKDOWN {
                pl.available = state::has_crystal_crew();
            }
        }
    }
    plates
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
        wheel_origin(x, y)
    }

    pub fn draw_wheel(&self) {
        let Some(def) = self.wheel.open.and_then(|w| self.layout.wheels.get(w)) else {
            return;
        };
        let origin = self.open_wheel_origin();
        let (ox, oy) = origin;
        let font = self.plate_font;
        draw_backdrop(self.cfg.wheel_backdrop, origin);
        // The middle: the wheel's title alone, or above the pick's name and detail.
        let title = def.name.to_ascii_uppercase();
        let sel = self.wheel.sel.map(|s| &self.wheel.plates[s]);
        let lines = match sel {
            None => 1,
            Some(p) if p.badge.is_empty() => 2,
            Some(_) => 3,
        };
        let body_h = if lines > 1 { CARD_GAP + (lines - 2) as f32 * WHEEL_LINE + WHEEL_FONT_H } else { 0.0 };
        let header_h = WHEEL_FONT_H + 2.0 * HEADER_PAD;
        let card_h = 2.0 * CARD_PAD + header_h + body_h;
        let mut text_w = text_width(font, &title);
        if let Some(p) = sel {
            text_w = text_w.max(text_width(font, &p.label)).max(text_width(font, &p.badge));
        }
        let card_w = text_w + 2.0 * (WHEEL_TEXT_PAD + CARD_PAD);
        let (x, y) = ((ox - card_w / 2.0).round(), (oy - card_h / 2.0).round());
        draw_frame(x, y, card_w, card_h, &BOX_FRAME, BUTTON_PALE_COLOR);
        draw_frame(x + CARD_PAD, y + CARD_PAD, card_w - 2.0 * CARD_PAD, header_h, &HEADER_FRAME, HEADER_COLOR);
        set_color(TEXT_COLOR);
        draw_text_center(font, ox, y + CARD_PAD + HEADER_PAD, &title);
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
        let label_k = label_scale(&self.wheel.plates, font);
        for (i, pl) in self.wheel.plates.iter().enumerate() {
            let (cx, cy) = plate_center(origin, i);
            draw_plate(pl, font, label_k, cx, cy, Some(i) == self.wheel.sel);
        }
    }

    /// Points the wheel from the stronger stick; the selection stays when both are centred.
    pub fn point_wheel(&mut self, left: [f32; 2], right: [f32; 2]) {
        let [x, y] = if right[0].hypot(right[1]) > left[0].hypot(left[1]) { right } else { left };
        if x.hypot(y) < WHEEL_ON {
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
            _ => {}
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
        let autopause = self.autopause();
        for pl in &mut self.wheel.plates {
            if let Some(Action::Set(i)) = pl.action {
                show_tuning(pl, self.cfg.tuning(i));
                pl.available = match option_name(i) {
                    SHARPEN_KEY => shrunk && self.cfg.frame_filter,
                    FRAME_FILTER_KEY => shrunk,
                    FOCUS_BRACKETS_KEY | CREW_MARKER_KEY => self.cfg.overlay,
                    HUD_SHIFT_KEY => narrow,
                    _ => true,
                };
                continue;
            }
            let Some(Action::Builtin(b)) = pl.action else {
                continue;
            };
            match b {
                Builtin::DpadMode => show_tuning(pl, Tuning::Switch(cursor)),
                Builtin::Autopause => show_tuning(pl, Tuning::Switch(autopause)),
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
    fn wheel_allowed(&self, index: usize) -> bool {
        self.layout.wheels.get(index).is_some_and(|w| w.kind == WheelKind::Settings) || self.wheel_screen()
    }

    /// Whether the open wheel may stay open.
    pub fn open_wheel_allowed(&self) -> bool {
        self.wheel.open.is_some_and(|w| self.wheel_allowed(w))
    }

    /// Closes the open wheel, unpausing what it paused; a settings wheel saves its values.
    pub fn close_wheel(&mut self) {
        let Some(closed) = self.wheel.open.take() else {
            return;
        };
        if self.wheel.paused && state::game_paused() {
            toggle_pause();
        }
        self.wheel.paused = false;
        self.wheel.sel = None;
        self.wheel.parent = None;
        self.wheel.picking = false;
        if self.layout.wheels.get(closed).is_some_and(|w| w.kind == WheelKind::Settings) {
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
        self.refresh_wheel();
        self.wheel.sel = (0..WHEEL_SLOTS).find(|&i| self.wheel.pickable(i));
        if self.cfg.autopause_wheel && state::ship_view() && !state::game_paused() {
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
            Some(Action::Builtin(b)) => self.builtin_tap(b),
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

    /// Adds power to the selected item of a power or systems wheel, or takes it away for a
    /// negative dir, picking or dropping a weapon past its power; on the crew wheel, adds the
    /// member to the selection or drops them; on the settings wheel, steps the selected setting.
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
                Action::Builtin(Builtin::Autopause) => self.set_autopause(dir > 0),
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
                let Some(member) = state::crew_at((code - KEY_F1) as usize) else {
                    return;
                };
                if dir > 0 {
                    tap_key(code, state::any_crew_selected());
                } else if let Some(gui) = state::gui() {
                    // SAFETY: DeselectCrew on the GUI's CrewControl with one of its crew.
                    unsafe { (api().deselect_crew)(gui.at(GUI_CREW_CONTROL).ptr(), member.ptr()) };
                }
            }
            WheelKind::Power | WheelKind::Systems => {
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
    use crate::config::layout::{Layout, LABEL_CHARS};
    use crate::game::fake::{calls, with_game, Call, Game};
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
        let item = g.arena.alloc(WEAPON_POWER + size_of::<i32>());
        item.set(WEAPON_BLUEPRINT, bp.addr());
        item.set(WEAPON_POWER, power);
        item.set(WEAPON_POWERED, powered as u8);
        let armament_box = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
        armament_box.set(ARMAMENTBOX_ITEM, item.addr());
        Some(armament_box)
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
    fn lockdown_needs_a_living_crystal_and_plates_keep_their_icons() {
        with_game(|g| {
            let mut l = Layer::default();
            let door_icon = g.arena.alloc(size_of::<usize>());
            l.images.insert("door.png", door_icon.addr());
            let layout = "[wheel:actions]\nup = p \"Lockdown\"\nright = z \"Open doors\" door.png@1,2,3,4\n";
            let manager = g.player_manager();
            let lockdown_available = |l: &mut Layer| plates(l, layout)[0].available;
            assert!(!lockdown_available(&mut l));
            let crew = [
                Some(g.crew("Ann", "human", PLAYER_SHIP_ID, false)),
                Some(g.crew("Cy", "crystal", PLAYER_SHIP_ID, true)),
            ];
            g.arena.ptr_vector(manager, SHIPMANAGER_CREW, &crew);
            assert!(!lockdown_available(&mut l));
            crew[1].expect("a crew member").set(CREW_DEAD, 0u8);
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
            let manager = g.player_manager();
            let member = g.crew(long, "human", ENEMY_SHIP_ID, false);
            member.at(CREW_BLUEPRINT + CREWBP_NAME).set(TEXTSTRING_LITERAL, 0u8);
            g.arena.ptr_vector(manager, SHIPMANAGER_CREW, &[Some(member)]);
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
    fn the_autopause_plate_switches_both_pauses() {
        with_game(|_| {
            let mut l = Layer::default();
            let p = plates(&mut l, "[wheel:settings]\nkind = settings\nup = autopause\n");
            let shown = (p[0].powered, p[0].badge.as_str(), p[0].glyph);
            assert_eq!(shown, (true, ON_BADGE, Some(Glyph::Pause)), "the wheel's pause alone is on");
            l.wheel.sel = Some(0);
            l.adjust_plate(1);
            assert!(l.cfg.autopause_wheel && l.cfg.autopause_aim);
            l.act_on_plate();
            assert!(!l.cfg.autopause_wheel && !l.cfg.autopause_aim && l.wheel.open.is_none());
        });
    }

    #[test]
    fn the_other_ship_plate_waits_for_an_enemy_ship() {
        with_game(|g| {
            let mut l = Layer::default();
            let layout = "[wheel:ship]\nup = center_swap \"Other ship\"\n";
            let p = plates(&mut l, layout);
            assert_eq!((p[0].available, p[0].glyph), (false, Some(Glyph::Swap)));
            g.enemy_manager();
            assert!(plates(&mut l, layout)[0].available);
        });
    }

    #[test]
    fn crew_plates_skip_the_dead_and_keep_their_f_keys() {
        with_game(|g| {
            let manager = g.player_manager();
            let anim = g.arena.alloc(size_of::<usize>());
            let members = [
                g.crew("Ann", "human", ENEMY_SHIP_ID, false),
                g.crew("Bo", "human", ENEMY_SHIP_ID, true),
                g.crew("Cy", "rock", ENEMY_SHIP_ID, false),
            ];
            members[2].set(CREW_ANIM, anim.addr());
            g.arena.ptr_vector(manager, SHIPMANAGER_CREW, &members.map(Some));
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
