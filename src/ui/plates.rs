//! Wheel plates built from the layout and the game: the power wheels' weapon and drone slots,
//! the systems and crew wheels, and layout plates with their built-in art and setting values.

use super::draw::*;
use super::glyph::{Glyph, HELP_GLYPH, LETTER_A, LETTER_B, LETTER_X, LETTER_Y, TEXT_GLYPH};
use super::wheel::Plate;
use crate::config::layout::{truncate_label, Action, Builtin, WheelDef, WheelKind, WHEEL_SLOTS};
use crate::config::settings::{
    is_switch, option_name, Tuning, AUTOPAUSE_ALL, CREW_MARKER_KEY, FLICK_THRESHOLD_KEY, FOCUS_BRACKETS_KEY,
    FOCUS_REPEAT_KEY, FRAME_FILTER_KEY, HUD_SHIFT_KEY, IPAD_BEACONS_KEY, OVERLAY_KEY, ROOM_CREW_PICK_KEY, RUMBLE_KEY,
    SHARPEN_KEY, SPEED_LEVEL_KEY, SWAP_AB_KEY, SWAP_XY_KEY, TAP_HOLD_KEY, TOUCH_KEY,
};
use crate::game::actions::{move_drone, move_weapon};
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;

/// Weapon art sits right of its texture's centre by this many texture pixels.
pub(super) const WEAPON_ICON_SHIFT: f32 = 3.0;
/// Plates that open these wheels show their glyphs.
const WHEEL_GLYPHS: [(&str, Glyph); 2] = [("view", Glyph::Display), ("controls", Glyph::Pad)];
/// The label of a slot with nothing in it.
pub(super) const EMPTY_LABEL: &str = "Empty";
pub(super) const ZOOM_OFF_BADGE: &str = "Whole frame";
pub(super) const AUTO_BADGE: &str = "Auto";
pub(super) const ON_BADGE: &str = "On";
/// A switch plate's one pip, lit while the switch is on.
pub(super) const SWITCH_PIPS: i32 = 1;
pub(super) const OFF_BADGE: &str = "Off";
/// The autopause plate's badge for each combination of the flags.
pub(super) const AUTOPAUSE_BADGES: [&str; AUTOPAUSE_ALL as usize + 1] = [OFF_BADGE, "Wheels", "Aim", "Both"];

/// Where a power wheel finds the item a slot key powers.
pub(super) struct Armament {
    pub(super) first_key: i32,
    pub(super) slots: i32,
    pub(super) control: fn() -> Option<Obj>,
    pub(super) blueprint: usize,
    pub(super) powered: usize,
    pub(super) power: usize,
    pub(super) icon: usize,
    pub(super) icon_prefix: &'static str,
    pub(super) icon_fallback: &'static str,
    pub(super) icon_shift: f32,
    /// Trades the items of two slots of the bar's control.
    pub(super) move_item: fn(Obj, usize, usize),
    /// Whether a volley tracks the bar's slots.
    pub(super) volley: bool,
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
        move_item: move_weapon,
        volley: true,
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
        move_item: move_drone,
        volley: false,
    },
];

/// Fixed systems plates in power-bar order; a slot takes the medbay or the clone bay.
#[rustfmt::skip]
const SYSTEM_SLOTS: [(i32, Option<i32>); WHEEL_SLOTS] = [
    (SYS_SHIELDS, None), (SYS_ENGINES, None), (SYS_MEDBAY, Some(SYS_CLONEBAY)), (SYS_OXYGEN, None),
    (SYS_TELEPORTER, None), (SYS_CLOAKING, None), (SYS_MIND, None), (SYS_HACKING, None),
];

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
pub(super) fn times(k: f32) -> String {
    format!("{}x", badge_number(k))
}

/// Whether a settings plate's action steps a setting by the D-pad, the wheel staying open.
pub(super) fn is_knob(action: Action) -> bool {
    match action {
        Action::Builtin(b) => matches!(b, Builtin::Zoom | Builtin::TextSize | Builtin::Autopause),
        Action::Set(i) => !is_switch(i),
        _ => false,
    }
}

/// Shows a setting on its plate: lit while on, a pip per step or the switch's one, and the
/// value as the badge.
pub(super) fn show_tuning(pl: &mut Plate, tuning: Tuning) {
    match tuning {
        Tuning::Switch(on) => {
            pl.powered = on;
            (pl.pips, pl.pips_on) = (SWITCH_PIPS, on as i32);
            pl.badge = if on { ON_BADGE } else { OFF_BADGE }.to_string();
        }
        Tuning::Steps { value, step, lo, hi, falling } => {
            // A pip per step from 0, or from a step past the end a step up leaves.
            let base = if falling {
                hi + step
            } else if lo > 0.0 {
                lo - step
            } else {
                0.0
            };
            let pips = |v: f32| ((v - base).abs() / step).round() as i32;
            pl.powered = value > 0.0;
            (pl.pips, pl.pips_on) = (pips(if falling { lo } else { hi }), pips(value));
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
        FOCUS_REPEAT_KEY => Some(Glyph::Repeat),
        TOUCH_KEY => Some(Glyph::Touch),
        ROOM_CREW_PICK_KEY => Some(Glyph::RoomCrew),
        TAP_HOLD_KEY => Some(Glyph::HoldTime),
        FLICK_THRESHOLD_KEY => Some(Glyph::Stick),
        SWAP_AB_KEY => Some(Glyph::Buttons(LETTER_A, LETTER_B)),
        SWAP_XY_KEY => Some(Glyph::Buttons(LETTER_X, LETTER_Y)),
        _ => None,
    }
}

pub(super) fn crew_plates() -> [Plate; WHEEL_SLOTS] {
    let mut plates: [Plate; WHEEL_SLOTS] = Default::default();
    let mut slot = 0;
    for (i, member) in state::boxed_crew().into_iter().filter(|&(i, _)| i < CREW_HOTKEYS) {
        if slot == WHEEL_SLOTS {
            break;
        }
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

pub(super) fn system_plates() -> [Plate; WHEEL_SLOTS] {
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
        pl.empty = false;
        show_power(pl, *sys);
        pl.icon = sys.obj(SYSTEM_ICON).map(Obj::ptr);
        pl.label = state::system_name(sys.i32(SYSTEM_TYPE)).to_string();
    }
    plates
}

/// Shows a system's power on its plate: pickable, lit while powered, a pip per bar, and the
/// power out of the bars as the badge.
fn show_power(pl: &mut Plate, sys: Obj) {
    pl.available = true;
    pl.powered = sys.i32(SYSTEM_POWER) > 0;
    (pl.pips, pl.pips_on) = (sys.i32(SYSTEM_POWER_MAX), sys.i32(SYSTEM_POWER));
    pl.badge = format!("{}/{}", pl.pips_on, pl.pips);
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
        Builtin::RepeatAbility => pl.glyph = Some(Glyph::Repeat),
        _ => {}
    }
}

/// The weapon slot a key arms, if it is a weapon's.
pub(super) fn weapon_slot(code: i32) -> Option<usize> {
    (KEY_FIRST_WEAPON..KEY_FIRST_WEAPON + WEAPON_SLOTS).contains(&code).then(|| (code - KEY_FIRST_WEAPON) as usize)
}

/// The bar and the slot a key of a wheel of `kind` powers; None off a power wheel.
pub(super) fn armament_of(kind: WheelKind, code: i32) -> Option<(&'static Armament, usize)> {
    if kind != WheelKind::Power {
        return None;
    }
    let arm = ARMAMENTS.iter().find(|a| (a.first_key..a.first_key + a.slots).contains(&code))?;
    Some((arm, (code - arm.first_key) as usize))
}

/// Whether an armament control holds a powered item.
fn any_powered(control: Obj) -> bool {
    (0..WEAPON_SLOTS as usize).any(|i| state::armament_item(control, i).is_some_and(|w| w.flag(WEAPON_POWERED)))
}

/// The plates of `def`, a wheel of `wheels`.
pub(super) fn layout_plates(def: &WheelDef, wheels: &[WheelDef], images: &mut Images) -> [Plate; WHEEL_SLOTS] {
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
            let name = wheels.get(w).map(|w| w.name.as_str());
            pl.glyph = WHEEL_GLYPHS.iter().find(|&&(wheel, _)| Some(wheel) == name).map(|&(_, glyph)| glyph);
        } else if let Action::Key { code: KEY_AUTOFIRE, .. } = pdef.action {
            pl.glyph = Some(Glyph::Autofire);
        }
        match pdef.action {
            Action::Builtin(Builtin::CenterSwap) => pl.available = state::enemy_ship().is_some(),
            Action::Builtin(Builtin::ArmAll) => pl.available = state::weapon_control().is_some_and(any_powered),
            _ => {}
        }
        let Action::Key { code, .. } = pdef.action else {
            continue;
        };
        if let Some((arm, slot)) = armament_of(def.kind, code) {
            let item = (arm.control)().and_then(|c| state::armament_item(c, slot));
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
        } else if code == KEY_AUTOFIRE {
            pl.powered = state::weapon_control().is_some_and(|c| c.flag(WEAPONCTRL_AUTOFIRE));
        } else if def.kind == WheelKind::Action && !state::powered_by(code).is_empty() {
            pl.available = false;
            if let Some(sys) = state::keyed_system(code) {
                show_power(pl, sys);
            }
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
