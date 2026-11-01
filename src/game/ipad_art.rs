//! FTL's iPad art for two sets of images the PC game shares with it, larger and drawn for
//! touch: the map's beacon icons and the crew station buttons, each set on its own switch.
//!
//! A hook on ResourceControl::GetImageId hands the game the iPad twin of an image while its
//! set is on; the layer's own lookups bypass it. The game builds both sets once at start, so
//! the layer rebuilds them when a switch changes: the star map's beacon primitives, centred
//! for the image's size, and the two station buttons, whose backgrounds the crew control then
//! draws anew.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::api::{api, GlColor, Ptr, Rect};
use crate::game::mem::Obj;
use crate::game::offsets::*;
use crate::game::state;
use crate::game::string::GameString;

/// Where the iPad twins sit, before the PC image's path.
const IPAD_DIR: &str = "ipad/";
/// The beacon icons: each image and the StarMap primitive the game builds from it.
const BEACONS: [(&str, usize); 4] = [
    ("map/map_icon_diamond_yellow.png", STARMAP_UNEXPLORED),
    ("map/map_icon_diamond_blue.png", STARMAP_EXPLORED),
    ("map/map_icon_triangle_red.png", STARMAP_WARNING),
    ("map/map_icon_triangle_yellow.png", STARMAP_YELLOW_WARNING),
];
/// Every station button image, each state's and the background's, starts with this.
const STATION_PREFIX: &str = "statusUI/button_station_";
/// The station buttons: each one's field and image name, which Button::OnInit ends with a
/// state's suffix.
const STATION_BUTTONS: [(usize, &str); 2] =
    [(CREW_SAVE_STATIONS, "statusUI/button_station_assign"), (CREW_RETURN_STATIONS, "statusUI/button_station_return")];
/// The state the game sizes a station button by.
const BUTTON_ON_SUFFIX: &str = "_on.png";
/// The beacon primitives as StarMap builds them: untinted, opaque, unturned.
const WHITE: GlColor = GlColor::new(1.0, 1.0, 1.0, 1.0);
const OPAQUE: f32 = 1.0;
const UNTURNED: i32 = 0;

/// A set of images swapped as one.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Set {
    Beacons,
    Stations,
}

/// Each set's switch as the hook reads it, in Set order.
static ON: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
static HOOKED: AtomicBool = AtomicBool::new(false);
/// Set while the layer looks up images of its own.
static BYPASS: AtomicBool = AtomicBool::new(false);

/// Records whether the GetImageId hook is in; without it nothing is swapped or rebuilt.
pub fn set_hooked(on: bool) {
    HOOKED.store(on, Ordering::Relaxed);
}

/// Puts the module's process-wide state back as at load, for the next test.
#[cfg(test)]
pub fn reset_statics() {
    ON.iter().for_each(|on| on.store(false, Ordering::Relaxed));
    HOOKED.store(false, Ordering::Relaxed);
    BYPASS.store(false, Ordering::Relaxed);
}

/// Runs `f` with the game's own images: no swap applies to lookups inside it.
pub fn own_lookup<R>(f: impl FnOnce() -> R) -> R {
    BYPASS.store(true, Ordering::Relaxed);
    let r = f();
    BYPASS.store(false, Ordering::Relaxed);
    r
}

/// The set image `name` belongs to.
fn set_of(name: &[u8]) -> Option<Set> {
    if BEACONS.iter().any(|(beacon, _)| beacon.as_bytes() == name) {
        Some(Set::Beacons)
    } else {
        name.starts_with(STATION_PREFIX.as_bytes()).then_some(Set::Stations)
    }
}

/// The image the game gets in place of `name`: its iPad twin while its set is on.
pub fn swapped(name: &CStr) -> Option<String> {
    if BYPASS.load(Ordering::Relaxed) {
        return None;
    }
    let set = set_of(name.to_bytes())?;
    ON[set as usize].load(Ordering::Relaxed).then(|| format!("{IPAD_DIR}{}", name.to_string_lossy()))
}

/// The size of image `name` as the game has it, swaps applied; None when it has none.
fn image_size(name: &GameString) -> Option<(i32, i32)> {
    let a = api();
    // SAFETY: ResourceControl::GetImageId on the global resources and a std::string.
    let texture = unsafe { Obj::from_ptr((a.image_id)(a.resources as Ptr, name.as_ptr())) }?;
    Some((texture.i32(TEXTURE_W), texture.i32(TEXTURE_H)))
}

/// Builds the star map's beacon primitives again, each centred on its beacon.
fn rebuild_beacons(map: Obj) {
    let a = api();
    for (image, field) in BEACONS {
        let name = GameString::new(image);
        let Some((w, h)) = image_size(&name) else {
            continue;
        };
        // SAFETY: ResourceControl::CreateImagePrimitive on the global resources and a std::string.
        let built = unsafe {
            (a.create_image_primitive)(
                a.resources as Ptr,
                name.as_ptr(),
                -w / 2,
                -h / 2,
                UNTURNED,
                WHITE,
                OPAQUE,
                false,
            )
        };
        if let Some(old) = map.obj(field) {
            // SAFETY: a primitive the star map built and holds only in this field.
            unsafe { (a.destroy_primitive)(old.ptr()) };
        }
        map.set(field, built as usize);
    }
}

/// Initialises the station buttons again, sized by their images as the game sizes them, and
/// has their backgrounds drawn anew.
fn rebuild_station_buttons(crew: Obj) {
    let a = api();
    for (field, image) in STATION_BUTTONS {
        let Some((w, h)) = image_size(&GameString::new(&format!("{image}{BUTTON_ON_SUFFIX}"))) else {
            continue;
        };
        let size = Rect { x: 0, y: 0, w, h };
        let name = GameString::new(image);
        // SAFETY: Button::OnInit on the crew control's own button, which frees what it held.
        unsafe { (a.button_init)(crew.at(field).ptr(), name.as_ptr(), &size, &size, false) };
    }
    crew.set(CREW_STATIONS_LAST_Y, i32::MIN);
}

/// The sets as the layer last built them: iPad or not, the beacons with the star map they
/// went to; None before it built them.
#[derive(Default)]
pub struct IpadArt {
    beacons: Option<(bool, usize)>,
    stations: Option<bool>,
}

impl Layer {
    /// Swaps each set to the iPad's or back as its switch says, rebuilding what the game
    /// built from it.
    pub fn update_ipad_art(&mut self) {
        if !HOOKED.load(Ordering::Relaxed) {
            return;
        }
        let (beacons, stations) = (self.cfg.ipad_beacons, self.cfg.ipad_station_buttons);
        ON[Set::Beacons as usize].store(beacons, Ordering::Relaxed);
        ON[Set::Stations as usize].store(stations, Ordering::Relaxed);
        if let Some(map) = state::star_map().filter(|m| self.ipad_art.beacons != Some((beacons, m.addr()))) {
            rebuild_beacons(map);
            self.ipad_art.beacons = Some((beacons, map.addr()));
            log!("ipad beacons {}", beacons as i32);
        }
        if let Some(gui) = state::gui().filter(|_| self.ipad_art.stations != Some(stations)) {
            rebuild_station_buttons(gui.at(GUI_CREW_CONTROL));
            self.ipad_art.stations = Some(stations);
            log!("ipad station buttons {}", stations as i32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::Settings;
    use crate::game::fake::{self, with_game, Call};

    #[test]
    fn a_set_swaps_only_while_on_and_never_for_the_layer() {
        // The switches are process-wide; the stand-in game runs one test at a time.
        with_game(|_| {
            let name = c"map/map_icon_triangle_red.png";
            ON[Set::Beacons as usize].store(false, Ordering::Relaxed);
            assert_eq!(swapped(name), None);
            ON[Set::Beacons as usize].store(true, Ordering::Relaxed);
            assert_eq!(swapped(name).as_deref(), Some("ipad/map/map_icon_triangle_red.png"));
            assert_eq!(own_lookup(|| swapped(name)), None);
            assert_eq!(swapped(c"map/map_icon_diamond_shadow.png"), None);
            assert_eq!(set_of(b"statusUI/button_station_base.png"), Some(Set::Stations));
            assert_eq!(set_of(b"statusUI/top_hull.png"), None);
        });
    }

    #[test]
    fn a_switch_rebuilds_its_set_once() {
        with_game(|g| {
            let map = g.arena.alloc(STARMAP_YELLOW_WARNING + size_of::<usize>());
            g.gui.set(GUI_STAR_MAP, map.addr());
            set_hooked(true);
            let cfg = Settings { ipad_beacons: true, ipad_station_buttons: true, ..Default::default() };
            let mut l = Layer { cfg, ..Default::default() };
            l.update_ipad_art();
            let (w, h) = fake::TEXTURE_SIZE;
            let calls = fake::calls();
            let built = calls.iter().filter(|c| **c == Call::CreatePrimitive(-w / 2, -h / 2)).count();
            assert_eq!(built, BEACONS.len());
            let crew = g.gui.at(GUI_CREW_CONTROL);
            for (field, _) in STATION_BUTTONS {
                assert!(calls.contains(&Call::ButtonInit(crew.at(field).addr(), w, h)));
            }
            assert_eq!(crew.i32(CREW_STATIONS_LAST_Y), i32::MIN);
            l.update_ipad_art();
            assert!(fake::calls().is_empty());
            l.cfg.ipad_beacons = false;
            l.update_ipad_art();
            let calls = fake::calls();
            assert_eq!(calls.iter().filter(|c| matches!(c, Call::DestroyPrimitive(_))).count(), BEACONS.len());
            assert!(!calls.iter().any(|c| matches!(c, Call::ButtonInit(..))));
        });
    }
}
