//! A stand-in game for tests: zeroed memory blocks written at the offsets in `offsets`,
//! std::vectors over them, and a CApp with its GUI that the readers in `state` reach.
//!
//! The CApp pointer and the API are process globals, so `with_game` runs one test at a
//! time. Every API symbol resolves into one zeroed block that starts with the mouse state;
//! a call into the game lands in non-executable memory and faults.

use std::ffi::c_void;
use std::mem::{size_of, size_of_val};
use std::sync::{Mutex, OnceLock};

use super::api::{self, Api, Point, Rect, SYMBOLS};
use super::mem::Obj;
use super::offsets::*;
use super::state;

/// A std::vector's begin and end pointers.
const VECTOR_FIELDS: usize = VECTOR_END + size_of::<usize>();
/// Reaches the last mouse field the readers use.
const STATICS_SIZE: usize = MOUSE_HACKING + size_of::<i32>();
/// Reaches the last CApp field the readers use.
const APP_SIZE: usize = CAPP_Y_BAR + size_of::<i32>();
/// Reaches the equipment screen's last flag, the GUI field farthest in.
const GUI_SIZE: usize = GUI_EQUIP_SCREEN + EQUIP_OVER_AUG_CAPACITY + size_of::<u8>();
const MANAGER_SIZE: usize = SHIPMANAGER_SHIP + SHIP_AIRLOCKS + VECTOR_FIELDS;
const ROOM_SIZE: usize = ROOM_ID + size_of::<i32>();
const DOOR_SIZE: usize = DOOR_Y + size_of::<i32>();
/// Reaches the literal flag of the member's name, its field farthest in.
const CREW_SIZE: usize = CREW_BLUEPRINT + CREWBP_NAME + TEXTSTRING_LITERAL + size_of::<u8>();

/// Zeroed memory blocks that stay put until the arena drops.
#[derive(Default)]
pub struct Arena {
    blocks: Vec<*mut [u64]>,
}

impl Arena {
    /// A zeroed, pointer-aligned object of `size` bytes.
    pub fn alloc(&mut self, size: usize) -> Obj {
        let words = size.div_ceil(size_of::<u64>()).max(1);
        let block = Box::into_raw(vec![0u64; words].into_boxed_slice());
        self.blocks.push(block);
        // SAFETY: a live block the arena owns, zeroed as the readers expect of a new object.
        unsafe { Obj::from_ptr(block as *const c_void) }.expect("an allocation is non-null")
    }

    /// Lays out a std::vector of `items` at `off` in `owner`.
    pub fn vector<T: Copy>(&mut self, owner: Obj, off: usize, items: &[T]) {
        let data = self.alloc(size_of_val(items));
        for (i, &item) in items.iter().enumerate() {
            data.set(i * size_of::<T>(), item);
        }
        owner.set(off + VECTOR_BEGIN, data.addr());
        owner.set(off + VECTOR_END, data.addr() + size_of_val(items));
    }

    /// A NUL-terminated copy of `text`, as a std::string's data pointer leads to.
    pub fn c_string(&mut self, text: &str) -> Obj {
        let data = self.alloc(text.len() + 1);
        for (i, b) in text.bytes().enumerate() {
            data.set(i, b);
        }
        data
    }

    /// Lays out a std::vector of pointers at `off` in `owner`; None is a null element.
    pub fn ptr_vector(&mut self, owner: Obj, off: usize, items: &[Option<Obj>]) {
        let addrs: Vec<usize> = items.iter().map(|o| o.map_or(0, Obj::addr)).collect();
        self.vector(owner, off, &addrs);
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        for &block in &self.blocks {
            // SAFETY: each block came from Box::into_raw and is freed once.
            drop(unsafe { Box::from_raw(block) });
        }
    }
}

/// The block every API symbol resolves to.
static STATICS: OnceLock<usize> = OnceLock::new();
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn install_api() -> usize {
    let words = STATICS_SIZE.div_ceil(size_of::<u64>());
    let block = Box::leak(vec![0u64; words].into_boxed_slice()).as_mut_ptr() as usize;
    // SAFETY: the data symbols read zeroed memory; the function pointers are never called.
    api::install(unsafe { Api::from_addresses(&vec![block; SYMBOLS.len()]) });
    block
}

/// Clears the CApp pointer when a game test ends, failed or not.
struct AppReset;

impl Drop for AppReset {
    fn drop(&mut self) {
        state::set_app(std::ptr::null_mut());
    }
}

/// The stand-in game a test builds on.
pub struct Game {
    pub arena: Arena,
    pub app: Obj,
    pub gui: Obj,
    /// MouseControl::Mouse.
    pub mouse: Obj,
}

/// Runs `f` on a fresh stand-in game: a zeroed CApp whose GUI pointer leads to a zeroed
/// GUI, and a cleared mouse.
pub fn with_game<R>(f: impl FnOnce(&mut Game) -> R) -> R {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let statics = *STATICS.get_or_init(install_api);
    // SAFETY: the leaked block of STATICS_SIZE bytes.
    unsafe { std::ptr::write_bytes(statics as *mut u8, 0, STATICS_SIZE) };
    let _reset = AppReset;
    let mut arena = Arena::default();
    let app = arena.alloc(APP_SIZE);
    let gui = arena.alloc(GUI_SIZE);
    app.set(CAPP_GUI, gui.addr());
    state::set_app(app.ptr());
    // SAFETY: the leaked block of STATICS_SIZE bytes.
    let mouse = unsafe { Obj::from_addr(statics) }.expect("a leaked block is non-null");
    let mut game = Game { arena, app, gui, mouse };
    f(&mut game)
}

impl Game {
    /// A CompleteShip at `slot` of `owner`; returns its ShipManager.
    fn complete_ship(&mut self, owner: Obj, slot: usize) -> Obj {
        let complete = self.arena.alloc(COMPLETE_SHIP_MANAGER + size_of::<usize>());
        let manager = self.arena.alloc(MANAGER_SIZE);
        owner.set(slot, complete.addr());
        complete.set(COMPLETE_SHIP_MANAGER, manager.addr());
        manager
    }

    /// The player's ShipManager, reached through the GUI.
    pub fn player_manager(&mut self) -> Obj {
        self.complete_ship(self.gui, GUI_SHIP_COMPLETE)
    }

    /// The enemy's ShipManager, reached through the combat control's target.
    pub fn enemy_manager(&mut self) -> Obj {
        self.complete_ship(self.gui.at(GUI_COMBAT_CONTROL), COMBAT_CURRENT_TARGET)
    }

    /// Points the std::string at `off` in `owner` at a copy of `text`.
    pub fn string(&mut self, owner: Obj, off: usize, text: &str) {
        let data = self.arena.c_string(text);
        owner.set(off, data.addr());
    }

    /// Makes the TextString at `off` in `owner` the literal `text`.
    pub fn literal(&mut self, owner: Obj, off: usize, text: &str) {
        let text_string = owner.at(off);
        self.string(text_string, TEXTSTRING_TEXT, text);
        text_string.set(TEXTSTRING_LITERAL, 1u8);
    }

    /// A CrewMember with a literal name, of a race, aboard a ship.
    pub fn crew(&mut self, name: &str, race: &str, ship: i32, dead: bool) -> Obj {
        let member = self.arena.alloc(CREW_SIZE);
        self.literal(member, CREW_BLUEPRINT + CREWBP_NAME, name);
        self.string(member, CREW_BLUEPRINT + CREWBP_RACE, race);
        member.set(CREW_SHIP, ship);
        member.set(CREW_DEAD, dead as u8);
        member
    }

    /// A Room with its id and its rectangle relative to the ship.
    pub fn room(&mut self, id: i32, rect: Rect) -> Obj {
        let room = self.arena.alloc(ROOM_SIZE);
        room.set(ROOM_ID, id);
        room.set(ROOM_RECT, rect);
        room
    }

    /// A Door between two rooms, at a point relative to the ship.
    pub fn door(&mut self, rooms: (i32, i32), at: Point, open: bool) -> Obj {
        let door = self.arena.alloc(DOOR_SIZE);
        door.set(DOOR_ROOM1, rooms.0);
        door.set(DOOR_ROOM2, rooms.1);
        door.set(DOOR_X, at.x);
        door.set(DOOR_Y, at.y);
        door.set(DOOR_OPEN, open as u8);
        door
    }
}
