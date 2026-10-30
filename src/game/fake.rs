//! A stand-in game for tests: zeroed memory blocks written at the offsets in `offsets`,
//! std::vectors over them, and a CApp with its GUI that the readers in `state` reach.
//!
//! The CApp pointer and the API are process globals, so `with_game` runs one test at a
//! time. Every data symbol resolves into one zeroed block that starts with the mouse state.
//! The game functions tests reach are typed stubs: input handlers record a `Call`, strings
//! are malloc'd copies `live_strings` counts, the joystick reports `FakePad`s, also written
//! into SIL's pad and device tables, the display is SCREEN. Any other call lands in
//! non-executable memory and faults.
//!
//! The renderer's primitives are distinct addresses, each build recorded.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::mem::{size_of, size_of_val};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::actions::MouseButton;
use super::api::{self, Api, CppVector, GlColor, Point, Ptr, Rect, SYMBOLS};
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
/// Reaches the hull image's x, the Ship field farthest in.
const MANAGER_SIZE: usize = SHIPMANAGER_SHIP + SHIP_IMAGE + IMAGE_X + size_of::<i32>();
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
    /// TabbedWindow::SetTab on the window at this address, to this tab.
    SetTab(usize, u32),
    /// CrewControl::ClearCrewBoxes on the control at this address.
    ClearCrewBoxes(usize),
    /// SystemControl::UpdateSubSystemBox on the control at this address.
    UpdateSubBox(usize),
    /// ResourceControl::CreateImagePrimitive at this offset from the drawing point.
    CreatePrimitive(i32, i32),
    /// CSurface::GL_DestroyPrimitive on the primitive at this address.
    DestroyPrimitive(usize),
    /// A multi-rect primitive built at this address.
    BuildPrimitive(usize),
    /// Button::OnInit on the button at this address, with this hit box size.
    ButtonInit(usize, i32, i32),
    /// freetype::easy_print in this font at this point.
    PrintLeft(i32, f32, f32),
    /// SwapArmaments on the armament control at this address, of these two slots.
    Swap(usize, u32, u32),
}

/// The size of every image ResourceControl::GetImageId finds.
pub const TEXTURE_SIZE: (i32, i32) = (40, 50);

/// A pad the joystick functions report: connected at `index`, or not at all.
#[derive(Clone, Default)]
pub struct FakePad {
    pub index: Option<i32>,
    /// The device's name, vendor and product.
    pub name: &'static str,
    pub id: (u16, u16),
    /// The raw button behind each SIL button; none past the list's end.
    pub mapping: Vec<i32>,
    /// Raw buttons held down.
    pub held: Vec<i32>,
    /// Both sticks, then the triggers as one more stick.
    pub sticks: [[f32; 2]; 3],
    pub dpad: [i32; 2],
}

/// The blocks the data symbols resolve to: every symbol's, then SIL's pad and device tables.
static STATICS: OnceLock<Statics> = OnceLock::new();
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
static CALLS: Mutex<Vec<Call>> = Mutex::new(Vec::new());
static PADS: Mutex<Vec<FakePad>> = Mutex::new(Vec::new());
static LIVE_STRINGS: AtomicIsize = AtomicIsize::new(0);
/// The ShipGraph ShipGraph::GetShipInfo returns for any ship.
static SHIP_GRAPH: AtomicUsize = AtomicUsize::new(0);
/// The texture of every image, TEXTURE_SIZE large.
static TEXTURE: OnceLock<usize> = OnceLock::new();
/// The last primitive handed out; each is a distinct non-null address.
static PRIMITIVES: AtomicUsize = AtomicUsize::new(0);
/// The player ship's systems by room: each one's address, its room its index.
static SYSTEMS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

#[derive(Clone, Copy)]
struct Statics {
    shared: usize,
    joysticks: usize,
    devices: usize,
}

const JOYSTICKS_SIZE: usize = SIL_TABLE_LEN * SILJOY_SIZE;
const DEVICES_SIZE: usize = SIL_TABLE_LEN * EVDEV_INFO_SIZE;

fn record(call: Call) {
    CALLS.lock().unwrap_or_else(|e| e.into_inner()).push(call);
}

fn pads() -> MutexGuard<'static, Vec<FakePad>> {
    PADS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` on the pad at `dev`, or on an unplugged one.
fn with_pad<R>(dev: c_int, f: impl FnOnce(&FakePad) -> R) -> R {
    let pads = pads();
    f(pads.iter().find(|p| p.index == Some(dev)).unwrap_or(&FakePad::default()))
}

/// A zeroed block of `size` bytes that lives as long as the process.
fn leak_block(size: usize) -> usize {
    Box::leak(vec![0u64; size.div_ceil(size_of::<u64>())].into_boxed_slice()).as_mut_ptr() as usize
}

/// Takes the calls recorded since the last take.
pub fn calls() -> Vec<Call> {
    std::mem::take(&mut CALLS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Makes `graph` the ShipGraph of every ship.
pub fn set_ship_graph(graph: Obj) {
    SHIP_GRAPH.store(graph.addr(), Ordering::Relaxed);
}

pub fn set_pad(state: FakePad) {
    set_pads(vec![state]);
}

/// Connects `state`'s pads, each in its slot of SIL's tables, as the only ones.
pub fn set_pads(state: Vec<FakePad>) {
    let statics = *STATICS.get().expect("pads are set inside with_game");
    // SAFETY: the leaked tables, SIL_TABLE_LEN entries each.
    let (joysticks, devices) = unsafe { (Obj::from_addr(statics.joysticks), Obj::from_addr(statics.devices)) };
    let (joysticks, devices) = (joysticks.expect("a leaked block"), devices.expect("a leaked block"));
    for pad in &state {
        let Some(dev) = pad.index else {
            continue;
        };
        let joy = joysticks.at(dev as usize * SILJOY_SIZE);
        joy.set(SILJOY_EVENT_DEV, dev);
        joy.set(SILJOY_VENDOR, pad.id.0);
        joy.set(SILJOY_PRODUCT, pad.id.1);
        let buttons = pad.mapping.iter().chain(&pad.held).max().map_or(0, |&b| b + 1);
        joy.set(SILJOY_NUM_BUTTONS, buttons);
        let device = devices.at(dev as usize * EVDEV_INFO_SIZE);
        for (i, b) in pad.name.bytes().chain([0]).enumerate() {
            device.set(EVDEV_NAME + i, b);
        }
    }
    *pads() = state;
}

/// The game strings built and not yet destroyed.
pub fn live_strings() -> isize {
    LIVE_STRINGS.load(Ordering::Relaxed)
}

extern "C" fn key_down(_app: Ptr, code: c_int) {
    record(Call::KeyDown(code));
    select_weapon(code);
}

/// As the game's weapon hotkeys: a slot's key without Shift arms its powered weapon.
fn select_weapon(code: c_int) {
    let slot = code - KEY_FIRST_WEAPON;
    if !(0..WEAPON_SLOTS).contains(&slot) || super::actions::shift_held() {
        return;
    }
    let Some(control) = state::weapon_control() else {
        return;
    };
    if let Some(weapon) = state::armament_item(control, slot as usize).filter(|w| w.flag(WEAPON_POWERED)) {
        control.set(WEAPONCTRL_ARMED, weapon.addr());
    }
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

extern "C" fn set_tab(tabbed: Ptr, tab: u32) {
    record(Call::SetTab(tabbed as usize, tab));
}

/// As the game's SwapArmaments with the boxes checked after: the two boxes trade items.
extern "C" fn swap_armaments(control: Ptr, a: c_uint, b: c_uint) {
    record(Call::Swap(control as usize, a, b));
    // SAFETY: a control the test built.
    let boxes = unsafe { Obj::from_addr(control as usize) }.expect("a built control").at(ARMAMENT_BOXES);
    if let (Some(x), Some(y)) = (boxes.vec_obj(a as usize), boxes.vec_obj(b as usize)) {
        let (item_x, item_y) = (x.get::<usize>(ARMAMENTBOX_ITEM), y.get::<usize>(ARMAMENTBOX_ITEM));
        x.set(ARMAMENTBOX_ITEM, item_y);
        y.set(ARMAMENTBOX_ITEM, item_x);
    }
}

extern "C" fn check_armaments(_control: Ptr) {}

extern "C" fn clear_crew(control: Ptr) {
    record(Call::ClearCrew(control as usize));
}

extern "C" fn clear_crew_boxes(control: Ptr) {
    record(Call::ClearCrewBoxes(control as usize));
}

extern "C" fn update_sub_box(control: Ptr) {
    record(Call::UpdateSubBox(control as usize));
}

fn systems() -> MutexGuard<'static, Vec<usize>> {
    SYSTEMS.lock().unwrap_or_else(|e| e.into_inner())
}

extern "C" fn system_room(_manager: Ptr, kind: c_int) -> c_int {
    // SAFETY: the systems are live arena objects of the running test.
    let kind_of = |&addr: &usize| unsafe { Obj::from_addr(addr) }.map(|s| s.i32(SYSTEM_TYPE));
    systems().iter().position(|a| kind_of(a) == Some(kind)).map_or(-1, |room| room as c_int)
}

extern "C" fn system_in_room(_manager: Ptr, room: c_int) -> Ptr {
    let addr = usize::try_from(room).ok().and_then(|r| systems().get(r).copied()).unwrap_or(0);
    addr as Ptr
}

extern "C" fn ship_info(_id: c_int) -> Ptr {
    SHIP_GRAPH.load(Ordering::Relaxed) as Ptr
}

extern "C" fn image_id(_resources: Ptr, _name: *const Ptr) -> Ptr {
    let texture = *TEXTURE.get_or_init(|| {
        let block = leak_block(TEXTURE_H + size_of::<i32>());
        // SAFETY: the leaked block just made, large enough for both fields.
        let texture = unsafe { Obj::from_addr(block) }.expect("a leaked block is non-null");
        texture.set(TEXTURE_W, TEXTURE_SIZE.0);
        texture.set(TEXTURE_H, TEXTURE_SIZE.1);
        block
    });
    texture as Ptr
}

#[allow(clippy::too_many_arguments)]
extern "C" fn create_image_primitive(
    _resources: Ptr,
    _name: *const Ptr,
    x: c_int,
    y: c_int,
    _turn: c_int,
    _color: GlColor,
    _alpha: f32,
    _mirror: bool,
) -> Ptr {
    record(Call::CreatePrimitive(x, y));
    (PRIMITIVES.fetch_add(1, Ordering::Relaxed) + 1) as Ptr
}

extern "C" fn destroy_primitive(primitive: Ptr) {
    record(Call::DestroyPrimitive(primitive as usize));
}

fn build_primitive() -> Ptr {
    let p = PRIMITIVES.fetch_add(1, Ordering::Relaxed) + 1;
    record(Call::BuildPrimitive(p));
    p as Ptr
}

extern "C" fn multi_rect(_rects: *const CppVector<Rect>, _color: GlColor) -> Ptr {
    build_primitive()
}

extern "C" fn set_color(_color: GlColor) {}

extern "C" fn print_left(font: c_int, x: f32, y: f32, _text: *const Ptr) {
    record(Call::PrintLeft(font, x, y));
}

extern "C" fn button_init(button: Ptr, _name: *const Ptr, hit: *const Rect, _image: *const Rect, _mirror: bool) {
    // SAFETY: the caller passes a valid hit box.
    let hit = unsafe { *hit };
    record(Call::ButtonInit(button as usize, hit.w, hit.h));
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
    pads().iter().filter_map(|p| p.index).max().map_or(0, |i| i + 1)
}

extern "C" fn joy_connected(dev: c_int) -> c_int {
    pads().iter().any(|p| p.index == Some(dev)) as c_int
}

extern "C" fn joy_num_sticks(dev: c_int) -> c_int {
    with_pad(dev, |p| p.sticks.len() as c_int)
}

extern "C" fn joy_button_mapping(dev: c_int, button: c_int) -> c_int {
    with_pad(dev, |p| p.mapping.get(button as usize).copied().unwrap_or(-1))
}

extern "C" fn joy_button_state(dev: c_int, raw: c_int) -> c_int {
    with_pad(dev, |p| p.held.contains(&raw) as c_int)
}

extern "C" fn joy_read_stick(dev: c_int, stick: c_int, x: *mut f32, y: *mut f32) {
    let [sx, sy] = with_pad(dev, |p| p.sticks[stick as usize]);
    // SAFETY: the caller passes places for both axes.
    unsafe {
        x.write(sx);
        y.write(sy);
    }
}

extern "C" fn joy_read_dpad(dev: c_int, x: *mut c_int, y: *mut c_int) {
    let [dx, dy] = with_pad(dev, |p| p.dpad);
    // SAFETY: the caller passes places for both axes.
    unsafe {
        x.write(dx);
        y.write(dy);
    }
}

fn install_api() -> Statics {
    install_layout(STAND_IN);
    let statics = Statics {
        shared: leak_block(STATICS_SIZE),
        joysticks: leak_block(JOYSTICKS_SIZE),
        devices: leak_block(DEVICES_SIZE),
    };
    // SAFETY: the data symbols read zeroed memory; the stubs below replace each function a
    // test reaches, and the rest are never called.
    let mut stand_in = unsafe { Api::from_addresses(&vec![statics.shared; SYMBOLS.len()]) };
    stand_in.sil_joysticks = statics.joysticks;
    stand_in.sil_event_info = statics.devices;
    stand_in.key_down = key_down;
    stand_in.key_up = key_up;
    stand_in.lbutton_down = lbutton_down;
    stand_in.lbutton_up = lbutton_up;
    stand_in.rbutton_down = rbutton_down;
    stand_in.rbutton_up = rbutton_up;
    stand_in.mouse_move = mouse_move;
    stand_in.set_tab = set_tab;
    stand_in.clear_crew = clear_crew;
    stand_in.weapon_swap = swap_armaments;
    stand_in.weapon_check = check_armaments;
    stand_in.drone_swap = swap_armaments;
    stand_in.drone_check = check_armaments;
    stand_in.clear_crew_boxes = clear_crew_boxes;
    stand_in.update_sub_box = update_sub_box;
    stand_in.ship_info = ship_info;
    stand_in.system_room = system_room;
    stand_in.system_in_room = system_in_room;
    stand_in.image_id = image_id;
    stand_in.create_image_primitive = create_image_primitive;
    stand_in.destroy_primitive = destroy_primitive;
    stand_in.multi_rect = multi_rect;
    stand_in.button_init = button_init;
    stand_in.set_color = set_color;
    stand_in.print_left = print_left;
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
    statics
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
    // SAFETY: the leaked blocks of these sizes.
    unsafe {
        std::ptr::write_bytes(statics.shared as *mut u8, 0, STATICS_SIZE);
        std::ptr::write_bytes(statics.joysticks as *mut u8, 0, JOYSTICKS_SIZE);
        std::ptr::write_bytes(statics.devices as *mut u8, 0, DEVICES_SIZE);
    }
    calls();
    set_pads(Vec::new());
    LIVE_STRINGS.store(0, Ordering::Relaxed);
    SHIP_GRAPH.store(0, Ordering::Relaxed);
    systems().clear();
    let _reset = AppReset;
    let mut arena = Arena::default();
    let app = arena.alloc(APP_SIZE);
    let gui = arena.alloc(GUI_SIZE);
    app.set(CAPP_GUI, gui.addr());
    state::set_app(app.ptr());
    // SAFETY: the leaked block of STATICS_SIZE bytes.
    let mouse = unsafe { Obj::from_addr(statics.shared) }.expect("a leaked block is non-null");
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

    /// A ShipSystem of `kind` with its power out of `max` bars, in the player ship's next room.
    pub fn system(&mut self, kind: i32, power: i32, max: i32) -> Obj {
        let sys = self.arena.alloc(SYSTEM_ICON + size_of::<usize>());
        sys.set(SYSTEM_TYPE, kind);
        sys.set(SYSTEM_POWER, power);
        sys.set(SYSTEM_POWER_MAX, max);
        systems().push(sys.addr());
        sys
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
