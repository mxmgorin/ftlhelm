//! The game's types as its functions take them, and the entry points, globals and
//! joystick API resolved from the executable's symbol table.

use std::ffi::{c_char, c_int, c_uint, c_void};
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

/// A std::vector as the game reads it, over memory the caller keeps alive.
#[repr(C)]
pub struct CppVector<T> {
    pub begin: *const T,
    pub end: *const T,
    pub capacity: *const T,
}

impl<T> CppVector<T> {
    pub fn of(items: &[T]) -> CppVector<T> {
        let span = items.as_ptr_range();
        CppVector { begin: span.start, end: span.end, capacity: span.end }
    }
}

/// freetype::font_data: one entry of the game's font table.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontData {
    pub h: f32,
    /// The engine's font handle.
    pub font: i32,
    /// The size glyphs are drawn at.
    pub size: f32,
    pub baseline: f32,
    pub line_height: f32,
}

impl FontData {
    pub const EMPTY: FontData = FontData { h: 0.0, font: 0, size: 0.0, baseline: 0.0, line_height: 0.0 };

    /// The same font drawn `k` times larger, its metrics on whole pixels.
    pub fn scaled(self, k: f32) -> FontData {
        FontData {
            h: (self.h * k).round(),
            font: self.font,
            size: self.size * k,
            baseline: (self.baseline * k).round(),
            line_height: (self.line_height * k).round(),
        }
    }
}

/// The engine's ShaderSourceCallback: a primitive's type, attribute counts and render
/// state; returns a shader source on the engine's heap.
pub type ShaderSource =
    unsafe extern "C" fn(c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int) -> *mut c_char;
/// The engine's ShaderKeyCallback: the same arguments; returns the shader table key.
pub type ShaderKey =
    unsafe extern "C" fn(c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int, c_int) -> c_uint;

/// The head of the engine's ShaderInfo: one shader table entry.
#[repr(C)]
pub struct ShaderInfo {
    pub program: c_uint,
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
    graphics_init: usize = "sys_graphics_init",
    xi_query_device: usize = "p_XIQueryDevice",
    touchscreen_present: usize = "xi2_touchscreen_present",
    sil_joysticks: usize = "joysticks",
    sil_event_info: usize = "event_info",
    get_text: unsafe extern "C" fn(*mut Ptr, Ptr, *const c_void, *const c_void) = "_ZN11TextLibrary7GetTextERKSsS1_",
    text_library: usize = "_ZN7Globals7LibraryE",
    onloop: usize = "_ZN4CApp6OnLoopEv",
    mouse_render: usize = "_ZN12MouseControl8OnRenderEv",
    get_shift: usize = "_ZN6CEvent13GetShiftStateEv",
    touch_down: usize = "_ZN4CApp11OnTouchDownEiii",
    touch_move: usize = "_ZN4CApp11OnTouchMoveEiiiii",
    touch_up: usize = "_ZN4CApp9OnTouchUpEiiiii",
    touch_cancel: usize = "_ZN4CApp13OnTouchCancelEiiiii",
    mouse_move: unsafe extern "C" fn(Ptr, c_int, c_int, c_int, c_int, bool, bool, bool) = "_ZN4CApp11OnMouseMoveEiiiibbb",
    lbutton_down: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp13OnLButtonDownEii",
    lbutton_up: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp11OnLButtonUpEii",
    rbutton_down: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp13OnRButtonDownEii",
    rbutton_up: unsafe extern "C" fn(Ptr, c_int, c_int) = "_ZN4CApp11OnRButtonUpEii",
    key_down: unsafe extern "C" fn(Ptr, c_int) = "_ZN4CApp9OnKeyDownEi",
    key_up: unsafe extern "C" fn(Ptr, c_int) = "_ZN4CApp7OnKeyUpEi",
    selected_room: unsafe extern "C" fn(Ptr, c_int, c_int, bool) -> c_int = "_ZN4Ship17GetSelectedRoomIdEiib",
    draw_rect: unsafe extern "C" fn(f32, f32, f32, f32, GlColor) = "_ZN8CSurface11GL_DrawRectEffff8GL_Color",
    set_color: unsafe extern "C" fn(GlColor) = "_ZN8CSurface11GL_SetColorE8GL_Color",
    blit: unsafe extern "C" fn(Ptr, f32, f32, f32, f32, f32, GlColor, bool) = "_ZN8CSurface12GL_BlitImageEP10GL_Texturefffff8GL_Colorb",
    blit_partial: unsafe extern "C" fn(Ptr, f32, f32, f32, f32, f32, f32, f32, f32, f32, GlColor, bool) = "_ZN8CSurface19GL_BlitImagePartialEP10GL_Texturefffffffff8GL_Colorb",
    push_matrix: unsafe extern "C" fn() = "_ZN8CSurface13GL_PushMatrixEv",
    pop_matrix: unsafe extern "C" fn() = "_ZN8CSurface12GL_PopMatrixEv",
    translate: unsafe extern "C" fn(f32, f32, f32) = "_ZN8CSurface12GL_TranslateEfff",
    scale: unsafe extern "C" fn(f32, f32, f32) = "_ZN8CSurface8GL_ScaleEfff",
    rotate: unsafe extern "C" fn(f32, f32, f32, f32) = "_ZN8CSurface9GL_RotateEffff",
    pie_partial: unsafe extern "C" fn(c_int, c_int, f32, f32, f32, f32, GlColor) -> Ptr = "_ZN8CSurface28GL_CreatePiePartialPrimitiveEiiffff8GL_Color",
    multi_rect: unsafe extern "C" fn(*const CppVector<Rect>, GlColor) -> Ptr = "_ZN8CSurface27GL_CreateMultiRectPrimitiveERKSt6vectorIN7Globals4RectESaIS2_EE8GL_Color",
    render_primitive_color: unsafe extern "C" fn(Ptr, GlColor) = "_ZN8CSurface27GL_RenderPrimitiveWithColorEP12GL_Primitive8GL_Color",
    set_fixed_color: unsafe extern "C" fn(*const GlColor) = "graphics_set_fixed_color",
    destroy_primitive: unsafe extern "C" fn(Ptr) = "_ZN8CSurface19GL_DestroyPrimitiveEP12GL_Primitive",
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
    weapon_swap: unsafe extern "C" fn(Ptr, c_uint, c_uint) = "_ZN13WeaponControl13SwapArmamentsEjj",
    weapon_check: unsafe extern "C" fn(Ptr) = "_ZN13WeaponControl12CheckWeaponsEv",
    drone_swap: unsafe extern "C" fn(Ptr, c_uint, c_uint) = "_ZN12DroneControl13SwapArmamentsEjj",
    drone_check: unsafe extern "C" fn(Ptr) = "_ZN12DroneControl11CheckDronesEv",
    mouse_position: usize = "_ZN12MouseControl5MouseE",
    tutorial: usize = "_ZN15TutorialManager8TutorialE",
    score_keeper: usize = "_ZN11ScoreKeeper6KeeperE",
    font_data: usize = "_ZN15ResourceControl11GetFontDataEib",
    choice_render: usize = "_ZN9ChoiceBox8OnRenderEv",
    render_pause: usize = "_ZN10CommandGui11RenderPauseEv",
    measure_tooltip: usize = "_ZN12MouseControl14MeasureTooltipEi",
    choice_mouse_move: usize = "_ZN9ChoiceBox9MouseMoveEii",
    frame_draw: usize = "_ZN11WindowFrame4DrawEii",
    display_width: unsafe extern "C" fn() -> c_int = "graphics_display_width",
    display_height: unsafe extern "C" fn() -> c_int = "graphics_display_height",
    set_viewport: unsafe extern "C" fn(c_int, c_int, c_int, c_int) = "_ZN8CSurface14GL_SetViewPortEiiii",
    blit_frame: usize = "_ZN8CSurface18GL_BlitFrameBufferEPiffb",
    blit_pixel: usize = "_ZN8CSurface17GL_BlitPixelImageEP10GL_Texturefffff8GL_Colorb",
    render_tooltip: usize = "_ZN12MouseControl13RenderTooltipE5Pointb",
    viewport_width: unsafe extern "C" fn() -> c_int = "graphics_viewport_width",
    viewport_height: unsafe extern "C" fn() -> c_int = "graphics_viewport_height",
    default_vertex_source: usize = "generate_vertex_shader_source",
    default_fragment_source: ShaderSource = "generate_fragment_shader_source",
    default_shader_key: ShaderKey = "generate_shader_key",
    set_shader_generator: unsafe extern "C" fn(usize, ShaderSource, ShaderKey, c_int, c_int) -> c_int = "graphics_set_shader_generator",
    add_shader_uniform: unsafe extern "C" fn(*const c_char) -> c_int = "graphics_add_shader_uniform",
    set_shader_uniform_float: unsafe extern "C" fn(c_int, f32) = "graphics_set_shader_uniform_float",
    set_shader_uniform_vec2: unsafe extern "C" fn(c_int, *const [f32; 2]) = "graphics_set_shader_uniform_vec2",
    shader_lookup: unsafe extern "C" fn(c_uint, *mut c_int) -> *const ShaderInfo = "shader_table_lookup",
    mem_alloc: unsafe extern "C" fn(i64, c_int, c_int) -> *mut c_void = "mem_alloc",
    crew_box_ctor: usize = "_ZN7CrewBoxC1E5PointP10CrewMemberi",
    status_mouse_move: usize = "_ZN10ShipStatus11OnMouseMoveEii",
    crew_render: usize = "_ZN11CrewControl8OnRenderEv",
    clear_crew_boxes: unsafe extern "C" fn(Ptr) = "_ZN11CrewControl14ClearCrewBoxesEv",
    update_sub_box: unsafe extern "C" fn(Ptr) = "_ZN13SystemControl18UpdateSubSystemBoxEv",
    ship_info: unsafe extern "C" fn(c_int) -> Ptr = "_ZN9ShipGraph11GetShipInfoEi",
    create_image_primitive: unsafe extern "C" fn(Ptr, *const Ptr, c_int, c_int, c_int, GlColor, f32, bool) -> Ptr = "_ZN15ResourceControl20CreateImagePrimitiveERKSsiii8GL_Colorfb",
    button_init: unsafe extern "C" fn(Ptr, *const Ptr, *const Rect, *const Rect, bool) = "_ZN6Button6OnInitERKSsRKN7Globals4RectES5_b",
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
