//! Wheels: plates built from the layout and the game, laid out around the cursor with
//! icons and power pips, picked by the sticks and the D-pad, and acted on.

use std::f32::consts::PI;

use super::draw::*;
use crate::config::layout::{Action, WheelKind, WHEEL_SLOTS};
use crate::core::layer::Layer;
use crate::core::util::{clampf, log};
use crate::game::actions::{tap_key, MouseButton};
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
const TILE_PAD: f32 = 5.0;
const PIP_H: f32 = 5.0;
const PIP_GAP: f32 = 2.0;
const PIP_MAX_W: f32 = 9.0;
const CORNER_ICON: f32 = 22.0;
const CORNER_INSET: f32 = 2.0;
const WHEEL_MARGIN: f32 = 8.0;
/// Lines of the wheel's middle: title, then the pick's name and detail.
const WHEEL_LINE: f32 = 22.0;
const WHEEL_TEXT_PAD: f32 = 6.0;
/// The disc under the plates reaches this far past them.
const WHEEL_DISC_PAD: f32 = 14.0;
/// The selected plate grows by this much on each side, inside a solid frame this thick.
const SELECTED_GROW: f32 = 4.0;
const SELECTED_FRAME: f32 = 3.0;

const PLATE_SELECTED_COLOR: GlColor = GlColor::new(0.42, 0.38, 0.1, 0.95);
pub const PLATE_SELECTED_EDGE_COLOR: GlColor = GlColor::new(0.96, 0.84, 0.28, 1.0);
const MIDDLE_COLOR: GlColor = GlColor::new(0.0, 0.03, 0.03, 0.6);
const DISC_COLOR: GlColor = GlColor::new(0.02, 0.06, 0.06, 0.5);
const PLATE_DISABLED_COLOR: GlColor = GlColor::new(0.05, 0.07, 0.07, 0.6);
const PLATE_EMPTY_EDGE_COLOR: GlColor = GlColor::new(0.5, 0.56, 0.54, 0.7);
const CORNER_COLOR: GlColor = GlColor::new(0.0, 0.03, 0.03, 0.8);
const WELL_COLOR: GlColor = GlColor::new(0.7, 0.76, 0.72, 1.0);
const WELL_POWERED_COLOR: GlColor = GlColor::new(0.55, 0.88, 0.55, 1.0);
/// Wheel plates: the pick solid, the rest see-through.
const PLATE_IDLE_COLOR: GlColor = GlColor::new(0.05, 0.11, 0.1, 0.75);
const PLATE_POWERED_COLOR: GlColor = GlColor::new(0.12, 0.36, 0.2, 0.8);
const PLATE_DISABLED_EDGE_COLOR: GlColor = GlColor::new(0.3, 0.34, 0.33, 0.8);
const PIP_ON_COLOR: GlColor = GlColor::new(0.47, 0.95, 0.47, 1.0);

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
        first_key: '1' as i32,
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
        first_key: '1' as i32 + WEAPON_SLOTS,
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
    /// A slot with nothing in it, drawn as a faint frame.
    pub empty: bool,
    pub action: Option<Action>,
    pub label: String,
    pub badge: String,
    icon: Option<Ptr>,
    /// The icon's region in texture pixels, when only part of it is the icon.
    region: Option<[f32; 4]>,
    /// Dark art that needs a light box under it, as in the game's weapon boxes.
    icon_well: bool,
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
}

impl WheelUi {
    fn pickable(&self, slot: usize) -> bool {
        self.plates[slot].bound && self.plates[slot].available
    }
}

/// The point the plates circle, kept inside the screen.
fn wheel_origin(cx: f32, cy: f32) -> (f32, f32) {
    let reach = WHEEL_RADIUS + WHEEL_TILE / 2.0 + WHEEL_MARGIN;
    (clampf(cx, reach, GAME_W as f32 - reach), clampf(cy, reach, GAME_H as f32 - reach))
}

/// Plate centre for a wheel slot around an origin.
fn plate_center((ox, oy): (f32, f32), slot: usize) -> (f32, f32) {
    let angle = slot as f32 * PI / 4.0;
    (ox + angle.sin() * WHEEL_RADIUS, oy - angle.cos() * WHEEL_RADIUS)
}

fn texture_size(texture: Ptr) -> (f32, f32) {
    // SAFETY: a texture the game handed out.
    let t = unsafe { Obj::from_ptr(texture) };
    t.map_or((0.0, 0.0), |t| (t.i32(TEXTURE_W) as f32, t.i32(TEXTURE_H) as f32))
}

/// Draws a plate's icon or portrait centred at cx,cy.
fn draw_plate_icon(pl: &Plate, cx: f32, cy: f32, tint: GlColor) {
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
    let k = TILE_ICON / sw.max(sh);
    let (w, h) = (sw * k, sh * k);
    let (x, y) = (cx - w / 2.0 - pl.icon_shift * k, cy - h / 2.0);
    match pl.region {
        // SAFETY: the renderer's blits on a game texture; the region goes in as fractions.
        Some([rx, ry, rw, rh]) => unsafe {
            (a.blit_partial)(icon, x, y, w, h, rx / tw, (rx + rw) / tw, ry / th, (ry + rh) / th, 1.0, tint, false)
        },
        None => unsafe { (a.blit)(icon, x, y, w, h, 0.0, tint, false) },
    }
}

/// Draws a row of power pips centred at cx with its top at y.
fn draw_pips(n: i32, lit: i32, cx: f32, y: f32, available: bool) {
    let n_f = n as f32;
    let pw = PIP_MAX_W.min((WHEEL_TILE - 2.0 * TILE_PAD - (n_f - 1.0) * PIP_GAP) / n_f);
    let mut x = cx - (n_f * pw + (n_f - 1.0) * PIP_GAP) / 2.0;
    for i in 0..n {
        if i < lit {
            draw_rect(x, y, pw, PIP_H, if available { PIP_ON_COLOR } else { TEXT_DISABLED_COLOR });
        } else {
            draw_rect_outline(x, y, pw, PIP_H, if available { PLATE_EDGE_COLOR } else { PLATE_DISABLED_EDGE_COLOR });
        }
        x += pw + PIP_GAP;
    }
}

fn draw_plate(pl: &Plate, font: i32, cx: f32, cy: f32, selected: bool) {
    let (x, y) = (cx - WHEEL_TILE / 2.0, cy - WHEEL_TILE / 2.0);
    if pl.empty || !pl.bound {
        draw_rect_outline(x, y, WHEEL_TILE, WHEEL_TILE, PLATE_EMPTY_EDGE_COLOR);
        return;
    }
    let (fill, edge) = if !pl.available {
        (PLATE_DISABLED_COLOR, PLATE_DISABLED_EDGE_COLOR)
    } else if selected {
        (PLATE_SELECTED_COLOR, PLATE_SELECTED_EDGE_COLOR)
    } else {
        (if pl.powered { PLATE_POWERED_COLOR } else { PLATE_IDLE_COLOR }, PLATE_EDGE_COLOR)
    };
    if selected && pl.available {
        let frame = SELECTED_GROW + SELECTED_FRAME;
        let side = WHEEL_TILE + 2.0 * frame;
        draw_rect(x - frame, y - frame, side, side, PLATE_SELECTED_EDGE_COLOR);
        let side = WHEEL_TILE + 2.0 * SELECTED_GROW;
        draw_panel(x - SELECTED_GROW, y - SELECTED_GROW, side, side, fill, edge);
    } else {
        draw_panel(x, y, WHEEL_TILE, WHEEL_TILE, fill, edge);
    }
    // With pips the icon box sits at the top, without it is centred.
    let icon_cy = if pl.pips > 0 { y + TILE_PAD + TILE_ICON / 2.0 } else { cy };
    let tint = if pl.available { TEXT_COLOR } else { TEXT_DISABLED_COLOR };
    if pl.icon_well {
        let well = if pl.powered { WELL_POWERED_COLOR } else { WELL_COLOR };
        draw_rect(cx - TILE_ICON / 2.0, icon_cy - TILE_ICON / 2.0, TILE_ICON, TILE_ICON, well);
    }
    if pl.icon.is_some() || pl.portrait.is_some() {
        draw_plate_icon(pl, cx, icon_cy, tint);
    } else if pl.available {
        set_color(tint);
        draw_text_center(font, cx, icon_cy - WHEEL_FONT_H / 2.0, &pl.label);
        set_color(TEXT_COLOR);
    }
    if let Some(corner) = pl.corner_icon {
        let (ix, iy) = (x + WHEEL_TILE - CORNER_ICON - CORNER_INSET, y + WHEEL_TILE - CORNER_ICON - CORNER_INSET);
        draw_rect(ix, iy, CORNER_ICON, CORNER_ICON, CORNER_COLOR);
        // SAFETY: the renderer's blit on a game texture.
        unsafe { (api().blit)(corner, ix, iy, CORNER_ICON, CORNER_ICON, 0.0, TEXT_COLOR, false) };
    }
    if pl.pips > 0 {
        draw_pips(pl.pips, pl.pips_on, cx, y + WHEEL_TILE - TILE_PAD - PIP_H, pl.available);
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

impl Layer {
    pub fn draw_wheel(&self) {
        let Some(def) = self.wheel.open.and_then(|w| self.layout.wheels.get(w)) else {
            return;
        };
        let origin = wheel_origin(self.cursor.x, self.cursor.y);
        let (ox, oy) = origin;
        let font = self.plate_font;
        draw_disc(ox, oy, WHEEL_RADIUS + WHEEL_TILE / 2.0 + WHEEL_DISC_PAD, DISC_COLOR);
        // The middle: the wheel's title alone, or above the pick's name and detail.
        let title = def.name.to_ascii_uppercase();
        let sel = self.wheel.sel.map(|s| &self.wheel.plates[s]);
        let lines = match sel {
            None => 1,
            Some(p) if p.badge.is_empty() => 2,
            Some(_) => 3,
        };
        let text_h = (lines - 1) as f32 * WHEEL_LINE + WHEEL_FONT_H;
        let top = oy - text_h / 2.0;
        // A shade under the text alone keeps it readable over the ship.
        let mut text_w = text_width(font, &title);
        if let Some(p) = sel {
            text_w = text_w.max(text_width(font, &p.label)).max(text_width(font, &p.badge));
        }
        draw_rect(
            ox - text_w / 2.0 - WHEEL_TEXT_PAD,
            top - WHEEL_TEXT_PAD,
            text_w + 2.0 * WHEEL_TEXT_PAD,
            text_h + 2.0 * WHEEL_TEXT_PAD,
            MIDDLE_COLOR,
        );
        set_color(PLATE_EDGE_COLOR);
        draw_text_center(font, ox, top, &title);
        if let Some(p) = sel {
            set_color(TEXT_COLOR);
            draw_text_center(font, ox, top + WHEEL_LINE, &p.label);
            if !p.badge.is_empty() {
                set_color(PLATE_SELECTED_EDGE_COLOR);
                draw_text_center(font, ox, top + 2.0 * WHEEL_LINE, &p.badge);
            }
        }
        set_color(TEXT_COLOR);
        for (i, pl) in self.wheel.plates.iter().enumerate() {
            let (cx, cy) = plate_center(origin, i);
            draw_plate(pl, font, cx, cy, Some(i) == self.wheel.sel);
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

    /// A weapon or drone plate's icon from its blueprint's icon name, and its pips.
    fn armament_plate(&mut self, pl: &mut Plate, arm: &Armament, item: Obj, bp: Obj) {
        let path = format!("{}{}{}", arm.icon_prefix, bp.string(arm.icon), ICON_SUFFIX);
        pl.icon = self.images.get(&path);
        pl.icon_well = true;
        pl.icon_shift = arm.icon_shift;
        if pl.icon.is_none() {
            pl.icon = self.images.get(arm.icon_fallback);
            pl.icon_well = false;
            pl.icon_shift = 0.0;
        }
        pl.region = None;
        pl.pips = item.i32(arm.power);
        pl.pips_on = if pl.powered { pl.pips } else { 0 };
    }

    fn crew_plates(&mut self) -> [Plate; WHEEL_SLOTS] {
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
            pl.label = state::display_text(member.at(CREW_BLUEPRINT + CREWBP_NAME));
        }
        // Empty slots keep their places, so positions never shift.
        for pl in &mut plates[slot..] {
            pl.bound = true;
            pl.empty = true;
            pl.label = "Empty".to_string();
        }
        plates
    }

    fn system_plates(&mut self) -> [Plate; WHEEL_SLOTS] {
        let mut plates: [Plate; WHEEL_SLOTS] = Default::default();
        let boxes = state::gui().map(|g| g.at(GUI_SYS_CONTROL + SYSCTRL_BOXES));
        let systems: Vec<Obj> = boxes
            .map(|b| {
                (0..b.vec_len(std::mem::size_of::<usize>())).filter_map(|i| b.vec_obj(i)?.obj(SYSBOX_SYSTEM)).collect()
            })
            .unwrap_or_default();
        for (pl, &(kind, alt)) in plates.iter_mut().zip(SYSTEM_SLOTS.iter()) {
            pl.bound = true;
            pl.empty = true;
            pl.label = state::system_name(kind).to_string();
            pl.action = state::system_key(kind).map(|code| Action::Key { code, shift: false });
            let Some(sys) = systems.iter().find(|s| s.i32(SYSTEM_TYPE) == kind || Some(s.i32(SYSTEM_TYPE)) == alt)
            else {
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

    fn layout_plates(&mut self, w: usize) -> [Plate; WHEEL_SLOTS] {
        let mut plates: [Plate; WHEEL_SLOTS] = Default::default();
        let def = self.layout.wheels[w].clone();
        for (pl, pdef) in plates.iter_mut().zip(def.plates.iter()) {
            let Some(pdef) = pdef else {
                continue;
            };
            pl.bound = true;
            pl.available = true;
            pl.action = Some(pdef.action);
            pl.label = pdef.label.clone();
            if let Some(icon) = &pdef.icon {
                pl.icon = self.images.get(&icon.path);
                pl.region = icon.region;
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
                        pl.label = state::display_text(bp.at(BLUEPRINT_SHORT_TITLE));
                        self.armament_plate(pl, arm, item, bp);
                    }
                    _ => pl.label = "Empty".to_string(),
                }
            } else if def.kind == WheelKind::Power && code == 'v' as i32 {
                pl.powered = state::weapon_control().is_some_and(|c| c.flag(WEAPONCTRL_AUTOFIRE));
            } else if def.kind == WheelKind::Action {
                if let Some(sys) = state::ability_system(code) {
                    pl.available = state::has_system(sys);
                } else if code == 'p' as i32 {
                    pl.available = state::has_crystal_crew();
                }
            }
        }
        plates
    }

    /// Fills the open wheel's plates from the layout and the game: crew and systems
    /// wholesale, power and action wheels their availability, badges and icons.
    pub fn refresh_wheel(&mut self) {
        self.wheel.refresh_wait = WHEEL_REFRESH_FRAMES;
        let Some(w) = self.wheel.open.filter(|&w| w < self.layout.wheels.len()) else {
            return;
        };
        self.wheel.plates = match self.layout.wheels[w].kind {
            WheelKind::Crew => self.crew_plates(),
            WheelKind::Systems => self.system_plates(),
            _ => self.layout_plates(w),
        };
    }

    pub fn close_wheel(&mut self) {
        if self.wheel.open.take().is_none() {
            return;
        }
        if self.wheel.paused && state::game_paused() {
            tap_key(KEY_SPACE, false);
        }
        self.wheel.paused = false;
        self.wheel.sel = None;
    }

    /// Opens a wheel on its top plate, or the first pickable one clockwise, in place of the
    /// open one; a second press of the open wheel closes it.
    pub fn open_wheel(&mut self, index: usize, by: Source) {
        if self.wheel.open == Some(index) {
            self.close_wheel();
            return;
        }
        if index >= self.layout.wheels.len() || (self.wheel.open.is_none() && !self.wheel_screen()) {
            return;
        }
        self.wheel.open = Some(index);
        self.wheel.sel = None;
        self.wheel.opened_aiming = state::aiming();
        self.refresh_wheel();
        self.wheel.sel = (0..WHEEL_SLOTS).find(|&i| self.wheel.pickable(i));
        if self.cfg.autopause_wheel && !state::game_paused() {
            tap_key(KEY_SPACE, false);
            self.wheel.paused = true;
        }
        if self.debug {
            log!("{}: wheel {}", by.name(), self.layout.wheels[index].name);
        }
    }

    /// Performs the selected plate: keys and clicks run as taps; an action wheel closes
    /// after, the others refill. The abilities key is remembered for repeat_ability.
    pub fn act_on_plate(&mut self) {
        let (Some(w), Some(sel)) = (self.wheel.open, self.wheel.sel) else {
            return;
        };
        if !self.wheel.pickable(sel) {
            return;
        }
        let kind = self.layout.wheels[w].kind;
        match self.wheel.plates[sel].action {
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
            _ => {}
        }
        if self.debug {
            log!("wheel {}: {}", self.layout.wheels[w].name, self.wheel.plates[sel].label);
        }
        if kind == WheelKind::Action {
            self.close_wheel();
        } else if self.wheel.open.is_some() {
            self.refresh_wheel();
        }
    }

    /// Adds power to the selected item of a power or systems wheel, or takes it away for a
    /// negative dir; on the crew wheel, adds the member to the selection or drops them.
    pub fn adjust_plate(&mut self, dir: i32) {
        let (Some(w), Some(sel)) = (self.wheel.open, self.wheel.sel) else {
            return;
        };
        let kind = self.layout.wheels[w].kind;
        let plate = &self.wheel.plates[sel];
        let Some(Action::Key { code, .. }) = plate.action.filter(|_| plate.bound && plate.available) else {
            return;
        };
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
                tap_key(code, dir < 0);
                if self.debug {
                    log!(
                        "wheel {}: {} power {}",
                        self.layout.wheels[w].name,
                        plate.label,
                        if dir < 0 { "down" } else { "up" }
                    );
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

    #[test]
    fn plates_go_clockwise_from_the_top() {
        let o = (640.0, 360.0);
        let (x, y) = plate_center(o, 0);
        assert!((x - 640.0).abs() < 1e-3 && (y - (360.0 - WHEEL_RADIUS)).abs() < 1e-3);
        let (x, y) = plate_center(o, 2);
        assert!((x - (640.0 + WHEEL_RADIUS)).abs() < 1e-3 && (y - 360.0).abs() < 1e-3);
    }

    #[test]
    fn origin_stays_on_screen() {
        let reach = WHEEL_RADIUS + WHEEL_TILE / 2.0 + WHEEL_MARGIN;
        assert_eq!(wheel_origin(0.0, 0.0), (reach, reach));
    }
}
