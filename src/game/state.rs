//! Readers over the game's objects: the CApp and its GUI, windows, crew, systems, weapons
//! and drones, and their display names.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use super::api::{api, Point, Ptr, Rect};
use super::mem::Obj;
use super::offsets::*;
use super::string::GameString;

/// The game's CApp, or null.
static APP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn set_app(app: Ptr) {
    APP.store(app, Ordering::Relaxed);
}

pub fn app() -> Option<Obj> {
    // SAFETY: APP only ever holds the CApp the game passed to its OnLoop.
    unsafe { Obj::from_ptr(APP.load(Ordering::Relaxed)) }
}

/// The CApp for calls into the game; null before the first OnLoop.
pub fn app_ptr() -> Ptr {
    APP.load(Ordering::Relaxed)
}

pub fn gui() -> Option<Obj> {
    app()?.obj(CAPP_GUI)
}

fn main_menu() -> Option<Obj> {
    Some(app()?.at(CAPP_MAIN_MENU))
}

pub fn main_menu_open() -> bool {
    main_menu().is_some_and(|m| m.flag(MAIN_MENU_OPEN))
}

/// MouseControl::Mouse, the game's own cursor state.
pub fn mouse() -> Obj {
    // SAFETY: a resolved global of the game.
    unsafe { Obj::from_addr(api().mouse_position) }.expect("a resolved symbol is non-null")
}

pub fn mouse_point() -> Point {
    mouse().point(0)
}

pub fn player_complete() -> Option<Obj> {
    gui()?.obj(GUI_SHIP_COMPLETE)
}

fn enemy_complete() -> Option<Obj> {
    gui()?.at(GUI_COMBAT_CONTROL).obj(COMBAT_CURRENT_TARGET)
}

/// The Ship object of a CompleteShip.
fn ship_of(complete: Option<Obj>) -> Option<Obj> {
    Some(complete?.obj(COMPLETE_SHIP_MANAGER)?.at(SHIPMANAGER_SHIP))
}

pub fn player_ship() -> Option<Obj> {
    ship_of(player_complete())
}

/// The Ship of the enemy the combat control targets.
pub fn enemy_ship() -> Option<Obj> {
    ship_of(enemy_complete())
}

/// Where the player ship's room rectangles start on screen.
pub fn player_origin(gui: Obj) -> Point {
    gui.point(GUI_SHIP_POSITION)
}

pub fn player_manager() -> Option<Obj> {
    player_complete()?.obj(COMPLETE_SHIP_MANAGER)
}

/// CrewControl's vector of selected crew.
pub fn selected_crew() -> Option<Obj> {
    Some(gui()?.at(GUI_CREW_CONTROL + CREW_SELECTED))
}

pub fn selected_count() -> usize {
    selected_crew().map_or(0, Obj::ptr_count)
}

pub fn any_crew_selected() -> bool {
    selected_count() > 0
}

pub fn first_selected_crew() -> Option<Obj> {
    selected_crew()?.vec_obj(0)
}

/// Whether a crew member is under the pointer, as the game resolved it a frame after the
/// pointer moved.
pub fn crew_under_pointer() -> bool {
    gui().is_some_and(|g| g.at(GUI_CREW_CONTROL + CREW_POTENTIAL).ptr_count() > 0)
}

pub fn crew_is_selected(member: Obj) -> bool {
    selected_crew().is_some_and(|v| v.vec_objs().any(|(_, m)| m == member))
}

pub fn game_paused() -> bool {
    gui().is_some_and(|g| g.flag(layout().gui_paused))
}

/// Whether a menu or an event holds the game.
pub fn menu_paused() -> bool {
    gui().is_some_and(|g| g.flag(layout().gui_menu_paused))
}

pub fn window_open(window: Option<Obj>) -> bool {
    window.is_some_and(|w| w.flag(FOCUS_OPEN))
}

/// Whether the GUI window at `off` is open.
pub fn gui_window_open(off: usize) -> bool {
    window_open(gui().map(|g| g.at(off)))
}

pub fn star_map() -> Option<Obj> {
    gui()?.obj(GUI_STAR_MAP)
}

pub fn star_map_open() -> bool {
    window_open(star_map())
}

/// The open dialog with choices: the main menu's while the menu is up, else the event box.
pub fn open_choice() -> Option<Obj> {
    let choice =
        if main_menu_open() { main_menu()?.at(layout().main_menu_choice_box) } else { gui()?.at(GUI_CHOICE_BOX) };
    choice.flag(FOCUS_OPEN).then_some(choice)
}

pub fn choice_open() -> bool {
    open_choice().is_some()
}

/// The open tabbed screen (ship or store).
pub fn open_tabbed() -> Option<Obj> {
    let g = gui()?;
    [layout().gui_ship_screens, layout().gui_store_screens]
        .into_iter()
        .map(|off| g.at(off))
        .find(|w| w.flag(FOCUS_OPEN))
}

/// The open store screen's buy window, the Store.
pub fn open_store() -> Option<Obj> {
    let screens = gui()?.at(layout().gui_store_screens);
    if !screens.flag(FOCUS_OPEN) {
        return None;
    }
    // Only an open store screen's window is sure to be live.
    screens.at(TABBED_WINDOWS).vec_obj(STORE_BUY_TAB)
}

/// How many pages the store's buy tab shows its sections on.
pub fn store_pages(store: Obj) -> u32 {
    1 + (store.i32(STORE_SECTION_COUNT).max(0) as usize > STORE_PAGE_SECTIONS) as u32
}

/// The buttons of the store page not shown, which the game hit-tests all the same; none
/// with the store closed.
pub fn hidden_store_buttons() -> Vec<usize> {
    let Some(store) = open_store() else {
        return Vec::new();
    };
    let page2 = store.flag(STORE_SHOW_PAGE2);
    let section_boxes = store.i32(STORE_SECTION_COUNT).max(0) as usize * STORE_SECTION_BOXES;
    store
        .at(STORE_BOXES)
        .vec_objs()
        .filter(|&(i, _)| i < section_boxes && (i >= STORE_PAGE_SECTIONS * STORE_SECTION_BOXES) != page2)
        .map(|(_, b)| b.addr() + STOREBOX_BUTTON)
        .collect()
}

/// Whether the ship view is up with nothing over it: no menu, ship or store screen, dialog
/// or map.
pub fn ship_view() -> bool {
    let Some(gui) = gui() else {
        return false;
    };
    player_complete().is_some()
        && !main_menu_open()
        && open_tabbed().is_none()
        && modal_window(gui).is_none()
        && !choice_open()
        && !star_map_open()
}

/// The extent of the topmost window over the frame: the options, the game menu, a store or
/// ship screen, or the map; None with no window, a dialog, or a screen over the whole frame.
pub fn window_rect() -> Option<Rect> {
    let gui = gui()?;
    if main_menu_open() {
        return None;
    }
    let options = gui.at(GUI_OPTIONS_BOX);
    if options.flag(FOCUS_OPEN) {
        return (!options.flag(OPTIONS_CUSTOMIZE_CONTROLS)).then_some(OPTIONS_RECT);
    }
    let menu = gui.at(GUI_MENU_BOX);
    if menu.flag(FOCUS_OPEN) {
        let whole = menu.flag(MENU_SHOW_CONTROLS) || menu.at(MENU_CONFIRM).flag(FOCUS_OPEN);
        return (!whole).then_some(MENU_RECT);
    }
    if choice_open() {
        return None;
    }
    let store = gui.at(layout().gui_store_screens);
    if store.flag(FOCUS_OPEN) {
        let sell = store.i32(TABBED_CURRENT_TAB) == STORE_SELL_TAB;
        return Some(if sell { STORE_SELL_RECT } else { SHIP_SCREENS_RECT });
    }
    if gui.at(layout().gui_ship_screens).flag(FOCUS_OPEN) {
        return Some(SHIP_SCREENS_RECT);
    }
    star_map_open().then_some(STAR_MAP_RECT)
}

/// The game menu while it takes the clicks: open, with no options, confirmation or controls
/// over it.
pub fn game_menu_alone() -> Option<Obj> {
    let gui = gui()?;
    let menu = gui.at(GUI_MENU_BOX);
    let covered = gui.at(GUI_OPTIONS_BOX).flag(FOCUS_OPEN)
        || menu.flag(MENU_SHOW_CONTROLS)
        || menu.at(MENU_CONFIRM).flag(FOCUS_OPEN);
    (menu.flag(FOCUS_OPEN) && !covered && !main_menu_open()).then_some(menu)
}

/// The game menu or the options screen, when one is open.
pub fn modal_window(gui: Obj) -> Option<Obj> {
    [GUI_OPTIONS_BOX, GUI_MENU_BOX].into_iter().map(|off| gui.at(off)).find(|w| w.flag(FOCUS_OPEN))
}

/// The hangar, while the main menu shows it.
pub fn hangar() -> Option<Obj> {
    let hangar = main_menu().filter(|m| m.flag(MAIN_MENU_OPEN))?.at(MAIN_MENU_HANGAR);
    hangar.flag(HANGAR_OPEN).then_some(hangar)
}

pub fn ship_list_open() -> bool {
    hangar().is_some_and(|h| h.at(HANGAR_SHIP_LIST).flag(SHIP_LIST_OPEN))
}

/// The open options screen: the main menu's while the menu is up, else the game's.
pub fn open_options() -> Option<Obj> {
    let options = if main_menu_open() { main_menu()?.at(MAIN_MENU_OPTIONS) } else { gui()?.at(GUI_OPTIONS_BOX) };
    options.flag(FOCUS_OPEN).then_some(options)
}

/// The choice box an options screen hit-tests: its restart notice while that is open, else
/// its own list; None while its language picker, wipe confirmation or hotkey screen is up.
pub fn options_choices(options: Obj) -> Option<Obj> {
    let l = layout();
    if [l.options_language, l.options_wipe_dialog].into_iter().any(|off| options.at(off).flag(FOCUS_OPEN)) {
        return None;
    }
    let notice = options.at(l.options_restart_dialog);
    if notice.flag(FOCUS_OPEN) {
        return Some(notice);
    }
    (!options.flag(OPTIONS_CUSTOMIZE_CONTROLS)).then_some(options)
}

/// The weapon or drone object in slot i of an armament control.
pub fn armament_item(control: Obj, i: usize) -> Option<Obj> {
    control.at(ARMAMENT_BOXES).vec_obj(i)?.obj(ARMAMENTBOX_ITEM)
}

pub fn weapon_control() -> Option<Obj> {
    Some(gui()?.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL))
}

/// The slot of the weapon waiting for a target in a weapon control, None when none is.
pub fn armed_slot(control: Obj) -> Option<usize> {
    let armed = control.obj(WEAPONCTRL_ARMED)?;
    (0..WEAPON_SLOTS as usize).find(|&i| armament_item(control, i) == Some(armed))
}

/// Whether a weapon holds a target.
pub fn weapon_aimed(weapon: Obj) -> bool {
    weapon.at(WEAPON_TARGETS).vec_len(POINTF_SIZE) > 0
}

/// Whether a weapon is a beam.
pub fn weapon_is_beam(weapon: Obj) -> bool {
    weapon.obj(WEAPON_BLUEPRINT).is_some_and(|bp| bp.string(WEAPONBP_TYPE_NAME) == BEAM_TYPE_NAME)
}

pub fn drone_control() -> Option<Obj> {
    Some(gui()?.at(GUI_COMBAT_CONTROL + COMBAT_DRONE_CONTROL))
}

/// Whether a weapon or an ability waits for a target.
pub fn aiming() -> bool {
    let m = mouse();
    [MOUSE_AIMING, MOUSE_TELEPORTING, MOUSE_MIND, MOUSE_HACKING].into_iter().any(|off| m.i32(off) != 0)
}

/// Whether the game found something that takes clicks under the pointer at its last OnLoop.
pub fn pointer_valid() -> bool {
    mouse().flag(MOUSE_VALID)
}

/// Where the enemy ship's room rectangles start on screen.
pub fn enemy_origin(gui: Obj) -> Point {
    let combat = gui.at(GUI_COMBAT_CONTROL);
    let origin = combat.point(COMBAT_POSITION);
    let target = combat.point(COMBAT_TARGET_POSITION);
    Point { x: origin.x + target.x, y: origin.y + target.y }
}

/// The player's crew with the indices of their boxes, which the crew hotkeys follow.
pub fn boxed_crew() -> Vec<(usize, Obj)> {
    let Some(gui) = gui() else {
        return Vec::new();
    };
    gui.at(GUI_CREW_CONTROL + CREW_BOXES).vec_objs().filter_map(|(i, b)| Some((i, b.obj(CREWBOX_CREW)?))).collect()
}

pub fn crew_dead(member: Obj) -> bool {
    member.flag(CREW_DEAD)
}

/// The game's current language code, as its loc- folders name it.
pub fn language() -> String {
    // SAFETY: the text library is a game global.
    unsafe { Obj::from_addr(api().text_library) }.map_or_else(String::new, |l| l.string(TEXTLIB_LANGUAGE))
}

/// Whether the game's current language is `code`.
pub fn language_is(code: &str) -> bool {
    // SAFETY: the text library is a game global.
    unsafe { Obj::from_addr(api().text_library) }.map_or(code.is_empty(), |l| l.string_is(TEXTLIB_LANGUAGE, code))
}

/// A TextString's display text: literal ones as they are, keys through the game's text
/// library in its current language.
pub fn display_text(text: Obj) -> String {
    if text.flag(TEXTSTRING_LITERAL) {
        return text.string(TEXTSTRING_TEXT);
    }
    let a = api();
    let mut result = GameString::returned();
    // SAFETY: GetText constructs its result in place; GameString releases it.
    unsafe {
        (a.get_text)(
            result.as_mut_ptr(),
            a.text_library as Ptr,
            text.ptr(),
            (a.text_library + TEXTLIB_LANGUAGE) as *const c_void,
        );
    }
    result.text()
}

pub fn system_key(kind: i32) -> Option<i32> {
    let key = match kind {
        SYS_SHIELDS => 'a',
        SYS_ENGINES => 's',
        SYS_OXYGEN => 'f',
        SYS_MEDBAY | SYS_CLONEBAY => 'd',
        SYS_TELEPORTER => 'g',
        SYS_CLOAKING => 'h',
        SYS_MIND => 'k',
        SYS_HACKING => 'l',
        SYS_ARTILLERY => 'y',
        _ => return None,
    };
    Some(key as i32)
}

pub fn system_name(kind: i32) -> &'static str {
    match kind {
        SYS_SHIELDS => "Shields",
        SYS_ENGINES => "Engines",
        SYS_OXYGEN => "Oxygen",
        SYS_MEDBAY => "Medbay",
        SYS_CLONEBAY => "Clone bay",
        SYS_TELEPORTER => "Teleporter",
        SYS_CLOAKING => "Cloaking",
        SYS_MIND => "Mind control",
        SYS_HACKING => "Hacking",
        SYS_ARTILLERY => "Artillery",
        SYS_WEAPONS => "Weapons",
        SYS_DRONES => "Drones",
        SYS_PILOT => "Piloting",
        SYS_SENSORS => "Sensors",
        SYS_DOORS => "Doors",
        SYS_BATTERY => "Battery",
        _ => "?",
    }
}

/// The system type an ability key needs.
pub fn ability_system(key: i32) -> Option<i32> {
    Some(match u8::try_from(key).ok()? {
        b'c' => SYS_CLOAKING,
        b'b' => SYS_BATTERY,
        b'n' => SYS_HACKING,
        b'm' => SYS_MIND,
        b't' | b'r' => SYS_TELEPORTER,
        _ => return None,
    })
}

/// The system kinds whose power `key` adds, Shift taking it.
pub fn powered_by(key: i32) -> Vec<i32> {
    (SYS_SHIELDS..=SYS_HACKING).filter(|&kind| system_key(kind) == Some(key)).collect()
}

/// The player ship's system whose power `key` adds.
pub fn keyed_system(key: i32) -> Option<Obj> {
    powered_by(key).into_iter().find_map(player_system)
}

/// Whether the player's crew has a living crystal member.
pub fn has_crystal_crew() -> bool {
    boxed_crew().into_iter().any(|(_, m)| !crew_dead(m) && m.at(CREW_BLUEPRINT).string(CREWBP_RACE) == CRYSTAL_RACE)
}

pub fn has_system(kind: i32) -> bool {
    let Some(manager) = player_manager() else {
        return false;
    };
    // SAFETY: GetSystemRoom reads the ship manager it is given.
    unsafe { (api().system_room)(manager.ptr(), kind) >= 0 }
}

/// The player ship's system of `kind`.
pub fn player_system(kind: i32) -> Option<Obj> {
    let manager = player_manager()?;
    // SAFETY: GetSystemRoom reads the ship manager it is given.
    let room = unsafe { (api().system_room)(manager.ptr(), kind) };
    if room < 0 {
        return None;
    }
    system_in_room(room)
}

/// The system in a room of the player ship.
pub fn system_in_room(room: i32) -> Option<Obj> {
    let manager = player_manager()?;
    // SAFETY: GetSystemInRoom returns a ShipSystem of the manager, or null.
    unsafe { Obj::from_ptr((api().system_in_room)(manager.ptr(), room)) }
}

/// CFPS::FPSControl, the game's frame timer.
pub fn fps_control() -> Obj {
    // SAFETY: a resolved global of the game.
    unsafe { Obj::from_addr(api().fps_control) }.expect("a resolved symbol is non-null")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{live_strings, with_game};

    #[test]
    fn the_topmost_window_gives_the_rect() {
        with_game(|g| {
            assert_eq!(window_rect(), None);
            let ships = g.gui.at(layout().gui_ship_screens);
            ships.set(FOCUS_OPEN, 1u8);
            assert_eq!(window_rect(), Some(SHIP_SCREENS_RECT));
            ships.set(FOCUS_OPEN, 0u8);
            let store = g.gui.at(layout().gui_store_screens);
            store.set(FOCUS_OPEN, 1u8);
            store.set(TABBED_CURRENT_TAB, STORE_SELL_TAB);
            assert_eq!(window_rect(), Some(STORE_SELL_RECT));
            // A dialog over the store shows the whole frame; the game menu over the dialog is fitted.
            g.gui.at(GUI_CHOICE_BOX).set(FOCUS_OPEN, 1u8);
            assert_eq!(window_rect(), None);
            let menu = g.gui.at(GUI_MENU_BOX);
            menu.set(FOCUS_OPEN, 1u8);
            assert_eq!(window_rect(), Some(MENU_RECT));
            menu.set(MENU_SHOW_CONTROLS, 1u8);
            assert_eq!(window_rect(), None);
            g.gui.at(GUI_OPTIONS_BOX).set(FOCUS_OPEN, 1u8);
            assert_eq!(window_rect(), Some(OPTIONS_RECT));
        });
    }

    #[test]
    fn display_text_reads_literals_and_translates_keys() {
        with_game(|g| {
            let text = g.arena.alloc(TEXTSTRING_LITERAL + std::mem::size_of::<u8>());
            g.string(text, TEXTSTRING_TEXT, "weapon_laser");
            assert_eq!(display_text(text), "[weapon_laser]");
            text.set(TEXTSTRING_LITERAL, 1u8);
            assert_eq!(display_text(text), "weapon_laser");
            assert_eq!(live_strings(), 0);
        });
    }

    #[test]
    fn the_selection_reads_crew_control() {
        with_game(|g| {
            let [a, b, c] = [(); 3].map(|_| g.arena.alloc(std::mem::size_of::<usize>()));
            assert_eq!((selected_count(), first_selected_crew()), (0, None));
            g.arena.ptr_vector(g.gui.at(GUI_CREW_CONTROL + CREW_SELECTED), 0, &[Some(a), Some(b)]);
            assert_eq!((selected_count(), first_selected_crew()), (2, Some(a)));
            assert!(any_crew_selected() && crew_is_selected(b) && !crew_is_selected(c));
        });
    }

    #[test]
    fn ships_and_their_screen_origins() {
        with_game(|g| {
            let player_at = Point { x: 100, y: 50 };
            g.gui.set(GUI_SHIP_POSITION, player_at);
            let combat = g.gui.at(GUI_COMBAT_CONTROL);
            combat.set(COMBAT_POSITION, Point { x: 600, y: 0 });
            combat.set(COMBAT_TARGET_POSITION, Point { x: 100, y: 60 });
            let player = g.player_manager().at(SHIPMANAGER_SHIP);
            assert_eq!(enemy_ship(), None);
            let enemy = g.enemy_manager().at(SHIPMANAGER_SHIP);
            assert_eq!(player_ship(), Some(player));
            assert_eq!(enemy_ship(), Some(enemy));
            assert_eq!(player_origin(g.gui), player_at);
            assert_eq!(enemy_origin(g.gui), Point { x: 700, y: 60 });
        });
    }
}
