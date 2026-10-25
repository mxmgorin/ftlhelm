//! A stand-in game for tests: zeroed memory blocks written at the offsets in `offsets`,
//! std::vectors over them, and a CApp with its GUI that the readers in `state` reach.
//!
//! The CApp pointer and the API are process globals, so `with_game` runs one test at a
//! time. Every data symbol resolves into one zeroed block that starts with the mouse state.
//! The game functions tests reach are typed stubs: input handlers record a `Call`, strings
//! are malloc'd copies `live_strings` counts, the joystick reports a `FakePad`, the display
//! is SCREEN. Any other call lands in non-executable memory and faults.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::mem::{size_of, size_of_val};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::actions::MouseButton;
use super::api::{self, Api, Point, Ptr, Rect, SYMBOLS};
use super::mem::Obj;
use super::offsets::*;
use super::state;

/// The layout the stand-in game takes: the first supported build's.
const STAND_IN: &Layout = &BUILDS[0].layout;
/// A std::vector's begin and end pointers.
const VECTOR_FIELDS: usize = VECTOR_END + size_of::<usize>();
/// Reaches the last mouse field the readers use.
const STATICS_SIZE: usize = MOUSE_HACKING + size_of::<i32>();
/// Reaches the last CApp field the readers use.
const APP_SIZE: usize = STAND_IN.capp_y_bar + size_of::<i32>();
/// Reaches the equipment screen's last flag, the GUI field farthest in.
const GUI_SIZE: usize = STAND_IN.gui_equip_screen + EQUIP_OVER_AUG_CAPACITY + size_of::<u8>();
const MANAGER_SIZE: usize = SHIPMANAGER_SHIP + SHIP_AIRLOCKS + VECTOR_FIELDS;
const ROOM_SIZE: usize = ROOM_ID + size_of::<i32>();
const DOOR_SIZE: usize = DOOR_Y + size_of::<i32>();
/// Reaches the literal flag of the member's name, its field farthest in.
const CREW_SIZE: usize = CREW_BLUEPRINT + CREWBP_NAME + TEXTSTRING_LITERAL + size_of::<u8>();

/// The display size the stand-in engine reports.
pub const SCREEN: (c_int, c_int) = (640, 480);

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

/// A call the game's input handlers took, positions in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Call {
    KeyDown(i32),
    KeyUp(i32),
    /// A mouse button pressed (true) or released.
    Button(MouseButton, bool, i32, i32),
    MouseMove(i32, i32),
    /// CrewControl::ClearSelectedCrew on the control at this address.
    ClearCrew(usize),
}

/// The pad the joystick functions report: one connected pad at `index`, or none.
#[derive(Clone, Default)]
pub struct FakePad {
    pub index: Option<i32>,
    /// The raw button behind each SIL button; none past the list's end.
    pub mapping: Vec<i32>,
    /// Raw buttons held down.
    pub held: Vec<i32>,
    /// Both sticks, then the triggers as one more stick.
    pub sticks: [[f32; 2]; 3],
    pub dpad: [i32; 2],
}

/// The block every data symbol resolves to.
static STATICS: OnceLock<usize> = OnceLock::new();
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
static CALLS: Mutex<Vec<Call>> = Mutex::new(Vec::new());
static PAD: Mutex<FakePad> =
    Mutex::new(FakePad { index: None, mapping: Vec::new(), held: Vec::new(), sticks: [[0.0; 2]; 3], dpad: [0; 2] });
static LIVE_STRINGS: AtomicIsize = AtomicIsize::new(0);

fn record(call: Call) {
    CALLS.lock().unwrap_or_else(|e| e.into_inner()).push(call);
}

fn pad() -> MutexGuard<'static, FakePad> {
    PAD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Takes the calls recorded since the last take.
pub fn calls() -> Vec<Call> {
    std::mem::take(&mut CALLS.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn set_pad(state: FakePad) {
    *pad() = state;
}

/// The game strings built and not yet destroyed.
pub fn live_strings() -> isize {
    LIVE_STRINGS.load(Ordering::Relaxed)
}

extern "C" fn key_down(_app: Ptr, code: c_int) {
    record(Call::KeyDown(code));
}

extern "C" fn key_up(_app: Ptr, code: c_int) {
    record(Call::KeyUp(code));
}

extern "C" fn lbutton_down(_app: Ptr, x: c_int, y: c_int) {
    record(Call::Button(MouseButton::Left, true, x, y));
}

extern "C" fn lbutton_up(_app: Ptr, x: c_int, y: c_int) {
    record(Call::Button(MouseButton::Left, false, x, y));
}

extern "C" fn rbutton_down(_app: Ptr, x: c_int, y: c_int) {
    record(Call::Button(MouseButton::Right, true, x, y));
}

extern "C" fn rbutton_up(_app: Ptr, x: c_int, y: c_int) {
    record(Call::Button(MouseButton::Right, false, x, y));
}

#[allow(clippy::too_many_arguments)]
extern "C" fn mouse_move(_app: Ptr, x: c_int, y: c_int, _dx: c_int, _dy: c_int, _l: bool, _m: bool, _r: bool) {
    record(Call::MouseMove(x, y));
}

extern "C" fn clear_crew(control: Ptr) {
    record(Call::ClearCrew(control as usize));
}

/// Stores a malloc'd copy of `chars` at `out` and counts it live.
/// # Safety
/// `chars` must be NUL-terminated and `out` writable.
unsafe fn new_string(out: *mut Ptr, chars: *const c_char) {
    out.write(libc::strdup(chars) as Ptr);
    LIVE_STRINGS.fetch_add(1, Ordering::Relaxed);
}

extern "C" fn string_ctor(out: *mut Ptr, chars: *const c_char, _alloc: *const c_void) {
    // SAFETY: the caller passes NUL-terminated text and a place for the string.
    unsafe { new_string(out, chars) }
}

extern "C" fn string_dtor(string: *mut Ptr) {
    // SAFETY: the string came from new_string.
    unsafe { libc::free(string.read()) };
    LIVE_STRINGS.fetch_sub(1, Ordering::Relaxed);
}

/// Translates a TextString's key as the key in brackets.
extern "C" fn get_text(out: *mut Ptr, _library: Ptr, text: *const c_void, _language: *const c_void) {
    // SAFETY: the caller passes a TextString of the stand-in game.
    let key = unsafe { Obj::from_ptr(text) }.map(|t| t.string(TEXTSTRING_TEXT)).unwrap_or_default();
    let translated = CString::new(format!("[{key}]")).expect("a key holds no NUL byte");
    // SAFETY: as for string_ctor.
    unsafe { new_string(out, translated.as_ptr()) }
}

/// Measures one pixel per byte.
extern "C" fn measure_width(_font: c_int, string: *const Ptr) -> c_int {
    // SAFETY: the caller passes a string string_ctor built.
    unsafe { CStr::from_ptr(string.read() as *const c_char) }.to_bytes().len() as c_int
}

extern "C" fn display_width() -> c_int {
    SCREEN.0
}

extern "C" fn display_height() -> c_int {
    SCREEN.1
}

extern "C" fn joy_count() -> c_int {
    pad().index.map_or(0, |i| i + 1)
}

extern "C" fn joy_connected(dev: c_int) -> c_int {
    (pad().index == Some(dev)) as c_int
}

extern "C" fn joy_num_sticks(_dev: c_int) -> c_int {
    pad().sticks.len() as c_int
}

extern "C" fn joy_button_mapping(_dev: c_int, button: c_int) -> c_int {
    pad().mapping.get(button as usize).copied().unwrap_or(-1)
}

extern "C" fn joy_button_state(_dev: c_int, raw: c_int) -> c_int {
    pad().held.contains(&raw) as c_int
}

extern "C" fn joy_read_stick(_dev: c_int, stick: c_int, x: *mut f32, y: *mut f32) {
    let [sx, sy] = pad().sticks[stick as usize];
    // SAFETY: the caller passes places for both axes.
    unsafe {
        x.write(sx);
        y.write(sy);
    }
}

extern "C" fn joy_read_dpad(_dev: c_int, x: *mut c_int, y: *mut c_int) {
    let [dx, dy] = pad().dpad;
    // SAFETY: the caller passes places for both axes.
    unsafe {
        x.write(dx);
        y.write(dy);
    }
}

fn install_api() -> usize {
    install_layout(STAND_IN);
    let words = STATICS_SIZE.div_ceil(size_of::<u64>());
    let block = Box::leak(vec![0u64; words].into_boxed_slice()).as_mut_ptr() as usize;
    // SAFETY: the data symbols read zeroed memory; the stubs below replace each function a
    // test reaches, and the rest are never called.
    let mut stand_in = unsafe { Api::from_addresses(&vec![block; SYMBOLS.len()]) };
    stand_in.key_down = key_down;
    stand_in.key_up = key_up;
    stand_in.lbutton_down = lbutton_down;
    stand_in.lbutton_up = lbutton_up;
    stand_in.rbutton_down = rbutton_down;
    stand_in.rbutton_up = rbutton_up;
    stand_in.mouse_move = mouse_move;
    stand_in.clear_crew = clear_crew;
    stand_in.string_ctor = string_ctor;
    stand_in.string_dtor = string_dtor;
    stand_in.get_text = get_text;
    stand_in.measure_width = measure_width;
    stand_in.display_width = display_width;
    stand_in.display_height = display_height;
    stand_in.joy_count = joy_count;
    stand_in.joy_connected = joy_connected;
    stand_in.joy_num_sticks = joy_num_sticks;
    stand_in.joy_button_mapping = joy_button_mapping;
    stand_in.joy_button_state = joy_button_state;
    stand_in.joy_read_stick = joy_read_stick;
    stand_in.joy_read_dpad = joy_read_dpad;
    api::install(stand_in);
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
    calls();
    set_pad(FakePad::default());
    LIVE_STRINGS.store(0, Ordering::Relaxed);
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
