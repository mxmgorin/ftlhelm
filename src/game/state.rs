//! Readers over the game's objects: the CApp and its GUI, windows, crew, systems, weapons
//! and drones, and their display names.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use super::api::{api, Point, Ptr};
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
fn selected_crew() -> Option<Obj> {
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

pub fn crew_is_selected(member: Obj) -> bool {
    selected_crew().is_some_and(|v| v.vec_objs().any(|(_, m)| m == member))
}

pub fn game_paused() -> bool {
    gui().is_some_and(|g| g.flag(GUI_PAUSED))
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
    let choice = if main_menu_open() { main_menu()?.at(MAIN_MENU_CHOICE_BOX) } else { gui()?.at(GUI_CHOICE_BOX) };
    choice.flag(FOCUS_OPEN).then_some(choice)
}

pub fn choice_open() -> bool {
    open_choice().is_some()
}

/// The open tabbed screen (ship or store).
pub fn open_tabbed() -> Option<Obj> {
    let g = gui()?;
    [GUI_SHIP_SCREENS, GUI_STORE_SCREENS].into_iter().map(|off| g.at(off)).find(|w| w.flag(FOCUS_OPEN))
}

/// The game menu or the options screen when one is open, with its size: only its buttons
/// count.
pub fn modal_window(gui: Obj) -> Option<(Obj, usize)> {
    [(GUI_OPTIONS_BOX, OPTIONS_BOX_SIZE), (GUI_MENU_BOX, MENU_BOX_SIZE)]
        .into_iter()
        .map(|(off, size)| (gui.at(off), size))
        .find(|(w, _)| w.flag(FOCUS_OPEN))
}

/// The weapon or drone object in slot i of an armament control.
pub fn armament_item(control: Obj, i: usize) -> Option<Obj> {
    control.at(ARMAMENT_BOXES).vec_obj(i)?.obj(ARMAMENTBOX_ITEM)
}

pub fn weapon_control() -> Option<Obj> {
    Some(gui()?.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL))
}

pub fn drone_control() -> Option<Obj> {
    Some(gui()?.at(GUI_COMBAT_CONTROL + COMBAT_DRONE_CONTROL))
}

/// Whether a weapon or an ability waits for a target.
pub fn aiming() -> bool {
    let m = mouse();
    [MOUSE_AIMING, MOUSE_TELEPORTING, MOUSE_MIND, MOUSE_HACKING].into_iter().any(|off| m.i32(off) != 0)
}

/// Where the enemy ship's room rectangles start on screen.
pub fn enemy_origin(gui: Obj) -> Point {
    let combat = gui.at(GUI_COMBAT_CONTROL);
    let origin = combat.point(COMBAT_POSITION);
    let target = combat.point(COMBAT_TARGET_POSITION);
    Point { x: origin.x + target.x, y: origin.y + target.y }
}

/// The crew member at an F-key index into the ship's crew list.
pub fn crew_at(index: usize) -> Option<Obj> {
    player_manager()?.at(SHIPMANAGER_CREW).vec_obj(index)
}

pub fn crew_dead(member: Obj) -> bool {
    member.flag(CREW_DEAD)
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
        b'y' => SYS_ARTILLERY,
        _ => return None,
    })
}

/// Whether the player's crew has a living crystal member.
pub fn has_crystal_crew() -> bool {
    (0..).map_while(crew_at).any(|m| !crew_dead(m) && m.at(CREW_BLUEPRINT).string(CREWBP_RACE) == CRYSTAL_RACE)
}

pub fn has_system(kind: i32) -> bool {
    let Some(manager) = player_manager() else {
        return false;
    };
    // SAFETY: GetSystemRoom reads the ship manager it is given.
    unsafe { (api().system_room)(manager.ptr(), kind) >= 0 }
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
