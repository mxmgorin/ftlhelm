//! The game's types as its functions take them, and the entry points, globals and
//! joystick API resolved from the executable's symbol table.

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

pub type Ptr = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl GlColor {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        GlColor { r, g, b, a }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x as f32 + self.w as f32 / 2.0, self.y as f32 + self.h as f32 / 2.0)
    }
}

macro_rules! game_api {
    ($($field:ident: $ty:ty = $sym:literal,)*) => {
        /// The resolved entry points and globals. Hooked functions and data are kept as
        /// plain addresses.
        pub struct Api {
            $(pub $field: $ty,)*
        }

        /// Symbol names in field order.
        pub const SYMBOLS: &[&str] = &[$($sym,)*];

        impl Api {
            /// Builds the API from one address per symbol, in SYMBOLS order.
            /// # Safety
            /// Each address must be the named symbol of the FTL build the types describe.
            pub unsafe fn from_addresses(addrs: &[usize]) -> Api {
                let mut it = addrs.iter();
                Api {
                    $($field: std::mem::transmute_copy::<usize, $ty>(
                        it.next().expect("one address per symbol, checked by the resolver")),)*
                }
            }
        }
    };
}

game_api! {
    joy_count: unsafe extern "C" fn() -> c_int = "input_joystick_count",
    joy_connected: unsafe extern "C" fn(c_int) -> c_int = "input_joystick_connected",
    joy_num_sticks: unsafe extern "C" fn(c_int) -> c_int = "input_joystick_num_sticks",
    joy_button_mapping: unsafe extern "C" fn(c_int, c_int) -> c_int = "input_joystick_button_mapping",
    joy_button_state: unsafe extern "C" fn(c_int, c_int) -> c_int = "input_joystick_button_state",
    joy_read_stick: unsafe extern "C" fn(c_int, c_int, *mut f32, *mut f32) = "input_joystick_read_stick",
    joy_read_dpad: unsafe extern "C" fn(c_int, *mut c_int, *mut c_int) = "input_joystick_read_dpad",
    joy_enable_unfocused: unsafe extern "C" fn(c_int) = "input_enable_unfocused_joystick",
    joy_rumble: unsafe extern "C" fn(c_int, f32, f32, f32) = "input_joystick_rumble",
    fps_control: usize = "_ZN4CFPS10FPSControlE",
    fps_loop: usize = "_ZN4CFPS6OnLoopEv",
    scan_joystick: usize = "scan_joystick",
    get_text: unsafe extern "C" fn(*mut Ptr, Ptr, *const c_void, *const c_void) = "_ZN11TextLibrary7GetTextERKSsS1_",
    text_library: usize = "_ZN7Globals7LibraryE",
    onloop: usize = "_ZN4CApp6OnLoopEv",
    mouse_render: usize = "_ZN12MouseControl8OnRenderEv",
    get_shift: usize = "_ZN6CEvent13GetShiftStateEv",
    mouse_move: unsafe extern "C" fn(Ptr, c_int, c_int, c_int, c_int, bool, bool, bool) = "_ZN4CApp11OnMouseMoveEiiiibbb",
    lbutton_down: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp13OnLButtonDownEii",
    lbutton_up: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp11OnLButtonUpEii",
    rbutton_down: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp13OnRButtonDownEii",
    rbutton_up: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp11OnRButtonUpEii",
    key_down: unsafe extern "C" fn(Ptr, c_int) = "_ZN4CApp9OnKeyDownEi",
    key_up: unsafe extern "C" fn(Ptr, c_int) = "_ZN4CApp7OnKeyUpEi",
    request_exit: unsafe extern "C" fn(Ptr) = "_ZN4CApp13OnRequestExitEv",
    draw_rect_outline: unsafe extern "C" fn(c_int, c_int, c_int, c_int, GlColor, f32) = "_ZN8CSurface18GL_DrawRectOutlineEiiii8GL_Colorf",
    selected_room: unsafe extern "C" fn(Ptr, c_int, c_int, bool) -> c_int = "_ZN4Ship17GetSelectedRoomIdEiib",
    draw_rect: unsafe extern "C" fn(f32, f32, f32, f32, GlColor) = "_ZN8CSurface11GL_DrawRectEffff8GL_Color",
    set_color: unsafe extern "C" fn(GlColor) = "_ZN8CSurface11GL_SetColorE8GL_Color",
    blit: unsafe extern "C" fn(Ptr, f32, f32, f32, f32, f32, GlColor, bool) = "_ZN8CSurface12GL_BlitImageEP10GL_Texturefffff8GL_Colorb",
    blit_partial: unsafe extern "C" fn(Ptr, f32, f32, f32, f32, f32, f32, f32, f32, f32, GlColor, bool) = "_ZN8CSurface19GL_BlitImagePartialEP10GL_Texturefffffffff8GL_Colorb",
    push_matrix: unsafe extern "C" fn() = "_ZN8CSurface13GL_PushMatrixEv",
    pop_matrix: unsafe extern "C" fn() = "_ZN8CSurface12GL_PopMatrixEv",
    translate: unsafe extern "C" fn(f32, f32, f32) = "_ZN8CSurface12GL_TranslateEfff",
    scale: unsafe extern "C" fn(f32, f32, f32) = "_ZN8CSurface8GL_ScaleEfff",
    image_id: unsafe extern "C" fn(Ptr, *const Ptr) -> Ptr = "_ZN15ResourceControl10GetImageIdERKSs",
    resources: usize = "_ZN15ResourceControl15GlobalResourcesE",
    crew_icon: unsafe extern "C" fn(Ptr, bool) = "_ZN13CrewAnimation10RenderIconEb",
    string_ctor: unsafe extern "C" fn(*mut Ptr, *const c_char, *const c_void) = "_ZNSsC1EPKcRKSaIcE",
    string_dtor: unsafe extern "C" fn(*mut Ptr) = "_ZNSsD1Ev",
    print_center: unsafe extern "C" fn(c_int, f32, f32, *const Ptr) = "_ZN8freetype16easy_printCenterEiffRKSs",
    print_left: unsafe extern "C" fn(c_int, f32, f32, *const Ptr) = "_ZN8freetype10easy_printEiffRKSs",
    measure_width: unsafe extern "C" fn(c_int, *const Ptr) -> c_int = "_ZN8freetype17easy_measureWidthEiRKSs",
    button_move: usize = "_ZN13GenericButton9MouseMoveEiib",
    system_room: unsafe extern "C" fn(Ptr, c_int) -> c_int = "_ZN11ShipManager13GetSystemRoomEi",
    system_in_room: unsafe extern "C" fn(Ptr, c_int) -> Ptr = "_ZN11ShipManager15GetSystemInRoomEi",
    set_tab: unsafe extern "C" fn(Ptr, u32) = "_ZN12TabbedWindow6SetTabEj",
    clear_crew: unsafe extern "C" fn(Ptr) = "_ZN11CrewControl17ClearSelectedCrewEv",
    deselect_crew: unsafe extern "C" fn(Ptr, Ptr) = "_ZN11CrewControl12DeselectCrewEP10CrewMember",
    mouse_position: usize = "_ZN12MouseControl5MouseE",
}

static API: OnceLock<Api> = OnceLock::new();

/// Installs the resolved API; the first call wins.
pub fn install(api: Api) {
    let _ = API.set(api);
}

/// The resolved API. Only code that runs after a successful init reaches it.
pub fn api() -> &'static Api {
    API.get().expect("hooks run only after the API is installed")
}
