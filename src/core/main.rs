//! Load-time setup and the hooks:
//! - CApp::OnLoop runs the layer once a frame, and installs the crash report on the first;
//! - MouseControl::OnRender draws the overlay around the game's pointer;
//!   CSurface::GL_BlitPixelImage draws the plain pointer larger on a small screen, or not
//!   while touch or focus brackets mark its point, and MouseControl::RenderTooltip moves
//!   its tooltip below it;
//! - CEvent::GetShiftState adds the layer's Shift;
//! - CApp::OnTouchDown, OnTouchMove, OnTouchUp and OnTouchCancel queue touches, and the
//!   engine looks for a touchscreen among all input devices;
//! - GenericButton::MouseMove records the buttons the game hit-tests;
//! - ChoiceBox::OnRender, WindowFrame::Draw and ChoiceBox::MouseMove draw and hit-test
//!   event boxes scaled;
//! - MouseControl::MeasureTooltip and ResourceControl::GetFontData scale tooltip fonts;
//! - CSurface::GL_BlitFrameBuffer shows the zoomed view and averages a shrunk frame;
//! - CrewBox::CrewBox, ShipStatus::OnMouseMove and CrewControl::OnRender build the crew
//!   boxes, hit-test the status panel and print the crew message with the HUD's left shift;
//! - ResourceControl::GetImageId hands the game the iPad art of the sets switched on;
//! - the PLT calls to snd_pcm_open and snd_pcm_recover restart a failed sound stream.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use super::crash::{self, Phase};
use super::hook::{accept_buttons_only_pads, hook, mapped_executable, query_all_touch_devices, redirect_step_cap, Exe};
use super::layer::with_layer;
use super::util::{line_buffer_stdout, log, now_s, open_log_file};
use crate::config::layout::DEFAULT_LAYOUT;
use crate::config::settings::layout_path;
use crate::game::actions::shift_held;
use crate::game::api::{self, api, Api, FontData, GlColor, Point, Ptr, SYMBOLS};
use crate::game::mem::Obj;
use crate::game::offsets::{build_by_id, install_layout, MOUSE_INVALID_POINTER, MOUSE_VALID_POINTER};
use crate::game::string::GameString;
use crate::game::zones::record_button;
use crate::game::{hud, ipad_art, sound, state};
use crate::input::touch::{self, TouchPhase};
use crate::ui::{badge, shrink, text, zoom};

const DEBUG_ENV: &str = "FTLHELM_DEBUG";
const LOG_ENV: &str = "FTLHELM_LOG";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Git revision of the build.
const REV: &str = env!("FTLHELM_REV");

/// push %rbp; push %rbx; mov %rdi,%rbx
const ONLOOP_PROLOGUE: [u8; 5] = [0x55, 0x53, 0x48, 0x89, 0xfb];
/// push %r15; mov %rdi,%r15
const MOUSE_RENDER_PROLOGUE: [u8; 5] = [0x41, 0x57, 0x49, 0x89, 0xff];
/// push %r15; mov %rdi,%r15
const CHOICE_RENDER_PROLOGUE: [u8; 5] = [0x41, 0x57, 0x49, 0x89, 0xff];
/// push %r12; push %rbp; push %rbx; mov %rdi,%rbx
const RENDER_PAUSE_PROLOGUE: [u8; 7] = [0x41, 0x54, 0x55, 0x53, 0x48, 0x89, 0xfb];
/// push %r12; lea -0x6(%rsi),%eax
const FONT_DATA_PROLOGUE: [u8; 5] = [0x41, 0x54, 0x8d, 0x46, 0xfa];
/// push %r13; xorps %xmm1,%xmm1
const MEASURE_TOOLTIP_PROLOGUE: [u8; 5] = [0x41, 0x55, 0x0f, 0x57, 0xc9];
/// push %r13; mov %edx,%r13d
const CHOICE_MOUSE_MOVE_PROLOGUE: [u8; 5] = [0x41, 0x55, 0x41, 0x89, 0xd5];
/// push %r12; mov %edx,%r12d
const FRAME_DRAW_PROLOGUE: [u8; 5] = [0x41, 0x54, 0x41, 0x89, 0xd4];
/// push %r12; mov %esi,%r12d
const BLIT_FRAME_PROLOGUE: [u8; 5] = [0x41, 0x54, 0x41, 0x89, 0xf4];
/// sub $0x8,%rsp; mov $KEY_LEFTSHIFT,%edi
const GET_SHIFT_PROLOGUE: [u8; 9] = [0x48, 0x83, 0xec, 0x08, 0xbf, 0x4b, 0x00, 0x00, 0x00];
/// mov 0x10(%rdi),%eax; movzbl 0x49(%rdi),%r8d
const BUTTON_MOVE_PROLOGUE: [u8; 8] = [0x8b, 0x47, 0x10, 0x44, 0x0f, 0xb6, 0x47, 0x49];
/// push %r12; push %rbp; mov %rdi,%rbp
const BLIT_PIXEL_PROLOGUE: [u8; 6] = [0x41, 0x54, 0x55, 0x48, 0x89, 0xfd];
/// push %r15; mov %rsi,%r15
const RENDER_TOOLTIP_PROLOGUE: [u8; 5] = [0x41, 0x57, 0x49, 0x89, 0xf7];
/// push %r15; mov %rsi,%rax
const CREW_BOX_PROLOGUE: [u8; 5] = [0x41, 0x57, 0x48, 0x89, 0xf0];
/// lea -0x6(%rsi),%eax; push %rbx; mov %rdi,%rbx
const STATUS_MOUSE_PROLOGUE: [u8; 7] = [0x8d, 0x46, 0xfa, 0x53, 0x48, 0x89, 0xfb];
/// push %r15; push %r14; lea 0xf0(%rdi),%r14
const CREW_RENDER_PROLOGUE: [u8; 11] = [0x41, 0x57, 0x41, 0x56, 0x4c, 0x8d, 0xb7, 0xf0, 0x00, 0x00, 0x00];
/// push %r12; push %rbp; mov %rsi,%rbp
const IMAGE_ID_PROLOGUE: [u8; 6] = [0x41, 0x54, 0x55, 0x48, 0x89, 0xf5];
/// The game puts the pointer's tooltip this far below the pointer's tip.
const POINTER_TOOLTIP_DROP: i32 = 22;
/// repz ret, then the padding nop's first bytes: the empty handler is shorter than a jump.
/// The copy returns before the cut nop, and its entry is never called.
const TOUCH_STUB_PROLOGUE: [u8; 5] = [0xf3, 0xc3, 0x66, 0x66, 0x66];

/// Entries that run the hooked functions' original code.
static ONLOOP_ORIG: AtomicUsize = AtomicUsize::new(0);
static MOUSE_RENDER_ORIG: AtomicUsize = AtomicUsize::new(0);
static GET_SHIFT_ORIG: AtomicUsize = AtomicUsize::new(0);
static BUTTON_MOVE_ORIG: AtomicUsize = AtomicUsize::new(0);
static FONT_DATA_ORIG: AtomicUsize = AtomicUsize::new(0);
static CHOICE_RENDER_ORIG: AtomicUsize = AtomicUsize::new(0);
static RENDER_PAUSE_ORIG: AtomicUsize = AtomicUsize::new(0);
static MEASURE_TOOLTIP_ORIG: AtomicUsize = AtomicUsize::new(0);
static CHOICE_MOUSE_MOVE_ORIG: AtomicUsize = AtomicUsize::new(0);
static FRAME_DRAW_ORIG: AtomicUsize = AtomicUsize::new(0);
static BLIT_FRAME_ORIG: AtomicUsize = AtomicUsize::new(0);
static BLIT_PIXEL_ORIG: AtomicUsize = AtomicUsize::new(0);
static RENDER_TOOLTIP_ORIG: AtomicUsize = AtomicUsize::new(0);
static CREW_BOX_ORIG: AtomicUsize = AtomicUsize::new(0);
static STATUS_MOUSE_ORIG: AtomicUsize = AtomicUsize::new(0);
static CREW_RENDER_ORIG: AtomicUsize = AtomicUsize::new(0);
static IMAGE_ID_ORIG: AtomicUsize = AtomicUsize::new(0);
/// The plain pointer's textures while the game draws its pointer, else 0.
static POINTERS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
static POINTER_HIDDEN: AtomicBool = AtomicBool::new(false);
/// How many times its size the plain pointer is drawn, as f32 bits.
static POINTER_SCALE: AtomicU32 = AtomicU32::new(0);
/// Whether MouseControl::RenderTooltip is hooked; the pointer grows only then.
static POINTER_TOOLTIP: AtomicBool = AtomicBool::new(false);

/// The original of a hooked `extern "C"` function of type F.
/// # Safety
/// `slot` must hold an entry hook() returned for a function of type F.
unsafe fn orig<F: Copy>(slot: &AtomicUsize) -> F {
    let addr = slot.load(Ordering::Relaxed);
    std::mem::transmute_copy::<usize, F>(&addr)
}

extern "C" fn onloop_hook(app: Ptr) {
    state::set_app(app);
    let t = now_s();
    with_layer(|l| {
        let _phase = crash::enter(Phase::OnLoop);
        let frame = l.clock.tick(t);
        if frame.count == 1 {
            log!("first OnLoop");
            sound::report();
            if l.touch.hooked {
                touch::report();
            }
            crash::install(api().onloop);
        }
        l.poll_files(t);
        l.update_hud();
        l.update_ipad_art();
        l.update_banner_art();
        text::keep_boxes_to(l.zoom.span);
        text::update(l.cfg.text_scale, state::app(), l.zoom.target);
        shrink::configure(l.cfg.frame_filter, l.cfg.sharpen);
        l.run_queued_clicks();
        l.run_volley();
        l.sync_cursor();
        l.build_zones();
        if l.cfg.focus_first {
            l.focus_first_on_open();
        }
        l.poll_commands();
        l.apply_touch(t);
        let pad = l.poll_pad(frame.dt);
        l.apply_pad(pad, t, frame.dt);
        l.run_volley();
        l.update_zoom(frame.dt);
        if l.debug && frame.beat {
            log!(
                "{} frames, pad {}, cursor {},{}, depth {}",
                frame.count,
                l.pad.index.unwrap_or(-1),
                l.cursor.last_x,
                l.cursor.last_y,
                l.bind.depth()
            );
        }
    });
    // SAFETY: the original CApp::OnLoop, called with the CApp it was given.
    unsafe { orig::<extern "C" fn(Ptr)>(&ONLOOP_ORIG)(app) };
}

extern "C" fn mouse_render_hook(this: Ptr) {
    let (draw_pointer, hide_arrow, scale) = with_layer(|l| {
        let _phase = crash::enter(Phase::Overlay);
        (l.draw_overlay(), l.pointer_hidden(), l.pointer_scale())
    })
    .unwrap_or((true, false, 1.0));
    if draw_pointer {
        // SAFETY: the game passes its MouseControl as `this`.
        let mouse = unsafe { Obj::from_ptr(this as *const c_void) };
        for (slot, off) in POINTERS.iter().zip([MOUSE_VALID_POINTER, MOUSE_INVALID_POINTER]) {
            slot.store(mouse.map_or(0, |m| m.get::<usize>(off)), Ordering::Relaxed);
        }
        POINTER_HIDDEN.store(hide_arrow, Ordering::Relaxed);
        let scale = if POINTER_TOOLTIP.load(Ordering::Relaxed) { scale } else { 1.0 };
        POINTER_SCALE.store(scale.to_bits(), Ordering::Relaxed);
        let _tooltip = text::TooltipScope::enter();
        // SAFETY: the original MouseControl::OnRender on its own object.
        unsafe { orig::<extern "C" fn(Ptr)>(&MOUSE_RENDER_ORIG)(this) };
        for slot in &POINTERS {
            slot.store(0, Ordering::Relaxed);
        }
    }
    with_layer(|l| {
        let _phase = crash::enter(Phase::Overlay);
        l.draw_overlay_top();
    });
}

extern "C" fn font_data_hook(this: Ptr, id: c_int, flag: bool) -> *const FontData {
    // SAFETY: the original ResourceControl::GetFontData with its own arguments.
    let found = unsafe { orig::<extern "C" fn(Ptr, c_int, bool) -> *const FontData>(&FONT_DATA_ORIG)(this, id, flag) };
    text::font_for(id, found)
}

extern "C" fn render_pause_hook(this: Ptr) {
    if with_layer(|l| l.banner_hides_pause()).unwrap_or(false) {
        return;
    }
    // SAFETY: the original CommandGui::RenderPause on its own object.
    unsafe { orig::<extern "C" fn(Ptr)>(&RENDER_PAUSE_ORIG)(this) };
}

extern "C" fn choice_render_hook(this: Ptr) {
    let _event = text::BoxRender::enter(this as usize);
    // SAFETY: the original ChoiceBox::OnRender on its own object.
    unsafe { orig::<extern "C" fn(Ptr)>(&CHOICE_RENDER_ORIG)(this) };
}

extern "C" fn frame_draw_hook(this: Ptr, x: c_int, y: c_int) {
    text::frame_drawn(this as usize, x, y);
    // SAFETY: the original WindowFrame::Draw with its own arguments.
    unsafe { orig::<extern "C" fn(Ptr, c_int, c_int)>(&FRAME_DRAW_ORIG)(this, x, y) };
}

extern "C" fn choice_mouse_move_hook(this: Ptr, x: c_int, y: c_int) {
    let (x, y) = text::box_zoom(this as usize).map_or((x, y), |z| z.to_layout(x, y));
    // SAFETY: the original ChoiceBox::MouseMove on its own object, at a position in its layout.
    unsafe { orig::<extern "C" fn(Ptr, c_int, c_int)>(&CHOICE_MOUSE_MOVE_ORIG)(this, x, y) };
}

extern "C" fn measure_tooltip_hook(this: Ptr, width: c_int) -> Point {
    // SAFETY: the original MouseControl::MeasureTooltip with its own arguments.
    text::measure_tooltip(|| unsafe { orig::<extern "C" fn(Ptr, c_int) -> Point>(&MEASURE_TOOLTIP_ORIG)(this, width) })
}

extern "C" fn blit_pixel_hook(tex: Ptr, x: f32, y: f32, w: f32, h: f32, turn: f32, color: GlColor, mirror: bool) {
    let pointer = !tex.is_null() && POINTERS.iter().any(|p| p.load(Ordering::Relaxed) == tex as usize);
    if pointer && POINTER_HIDDEN.load(Ordering::Relaxed) {
        return;
    }
    // The plain pointer's tip is its image's corner, so it stays put as the image grows.
    let k = if pointer { f32::from_bits(POINTER_SCALE.load(Ordering::Relaxed)) } else { 1.0 };
    // SAFETY: the original CSurface::GL_BlitPixelImage with its own arguments.
    unsafe {
        orig::<extern "C" fn(Ptr, f32, f32, f32, f32, f32, GlColor, bool)>(&BLIT_PIXEL_ORIG)(
            tex,
            x,
            y,
            w * k,
            h * k,
            turn,
            color,
            mirror,
        )
    };
}

extern "C" fn render_tooltip_hook(this: Ptr, at: Point, fixed: bool) {
    let k = f32::from_bits(POINTER_SCALE.load(Ordering::Relaxed));
    let drawn = POINTERS[0].load(Ordering::Relaxed) != 0 && !POINTER_HIDDEN.load(Ordering::Relaxed);
    let tip = state::mouse_point();
    let below_tip = at == Point { x: tip.x, y: tip.y + POINTER_TOOLTIP_DROP };
    let at = if drawn && below_tip && k > 1.0 {
        Point { y: tip.y + (POINTER_TOOLTIP_DROP as f32 * k).round() as i32, ..at }
    } else {
        at
    };
    // SAFETY: the original MouseControl::RenderTooltip on its own object.
    unsafe { orig::<extern "C" fn(Ptr, Point, bool)>(&RENDER_TOOLTIP_ORIG)(this, at, fixed) };
}

extern "C" fn crew_box_hook(this: Ptr, at: Point, crew: Ptr, number: c_int) {
    let at = Point { x: at.x + hud::left_shift(), ..at };
    // SAFETY: the original CrewBox constructor on its own object.
    unsafe { orig::<extern "C" fn(Ptr, Point, Ptr, c_int)>(&CREW_BOX_ORIG)(this, at, crew, number) };
}

extern "C" fn status_mouse_hook(this: Ptr, x: c_int, y: c_int) {
    // SAFETY: the original ShipStatus::OnMouseMove on its own object, at a position in its layout.
    unsafe { orig::<extern "C" fn(Ptr, c_int, c_int)>(&STATUS_MOUSE_ORIG)(this, x - hud::left_shift(), y) };
}

extern "C" fn crew_render_hook(this: Ptr) {
    // SAFETY: the original CrewControl::OnRender on its own object.
    let render = || unsafe { orig::<extern "C" fn(Ptr)>(&CREW_RENDER_ORIG)(this) };
    // SAFETY: the game passes its CrewControl as `this`.
    match unsafe { Obj::from_ptr(this as *const c_void) } {
        Some(crew) => hud::render_crew_control(crew, render),
        None => render(),
    }
}

extern "C" fn image_id_hook(resources: Ptr, name: *const Ptr) -> Ptr {
    // SAFETY: the original ResourceControl::GetImageId on its own object and a std::string.
    let lookup =
        |name: *const Ptr| unsafe { orig::<extern "C" fn(Ptr, *const Ptr) -> Ptr>(&IMAGE_ID_ORIG)(resources, name) };
    // SAFETY: the game passes a constructed std::string, whose characters end with a NUL.
    let swap = ipad_art::swapped(unsafe { CStr::from_ptr(*name as *const c_char) });
    let texture = swap.map_or(std::ptr::null_mut(), |ipad| lookup(GameString::new(&ipad).as_ptr()));
    if texture.is_null() {
        lookup(name)
    } else {
        texture
    }
}

extern "C" fn blit_frame_hook(fb: Ptr, w: f32, h: f32, smooth: bool) {
    zoom::blit_frame(w, h, smooth, |smooth| {
        // SAFETY: the original CSurface::GL_BlitFrameBuffer with its own frame buffer and size.
        unsafe { orig::<extern "C" fn(Ptr, f32, f32, bool)>(&BLIT_FRAME_ORIG)(fb, w, h, smooth) }
    });
}

extern "C" fn get_shift_hook() -> bool {
    // SAFETY: the original CEvent::GetShiftState.
    shift_held() || unsafe { orig::<extern "C" fn() -> bool>(&GET_SHIFT_ORIG)() }
}

extern "C" fn touch_down_hook(_app: Ptr, id: c_int, x: c_int, y: c_int) {
    with_layer(|l| l.queue_touch(TouchPhase::Down, id, x, y));
}

extern "C" fn touch_move_hook(_app: Ptr, id: c_int, x: c_int, y: c_int, _x0: c_int, _y0: c_int) {
    with_layer(|l| l.queue_touch(TouchPhase::Move, id, x, y));
}

extern "C" fn touch_up_hook(_app: Ptr, id: c_int, x: c_int, y: c_int, _x0: c_int, _y0: c_int) {
    with_layer(|l| l.queue_touch(TouchPhase::Up, id, x, y));
}

extern "C" fn touch_cancel_hook(_app: Ptr, id: c_int, x: c_int, y: c_int, _x0: c_int, _y0: c_int) {
    with_layer(|l| l.queue_touch(TouchPhase::Cancel, id, x, y));
}

extern "C" fn button_move_hook(this: Ptr, x: i32, y: i32, silent: bool) {
    // SAFETY: the game passes its GenericButton as `this`.
    if let Some(button) = unsafe { Obj::from_ptr(this as *const c_void) } {
        let _phase = crash::enter(Phase::ButtonMove);
        record_button(button);
    }
    // SAFETY: the original GenericButton::MouseMove with its own arguments.
    unsafe { orig::<extern "C" fn(Ptr, i32, i32, bool)>(&BUTTON_MOVE_ORIG)(this, x, y, silent) };
}

/// What a run of hooks gives the layer; a feature works only with all of its hooks in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Feature {
    Overlay,
    Shift,
    Buttons,
    Touch,
    Pointer,
    PointerTooltip,
    EventText,
    TooltipText,
    Zoom,
    HudLeft,
    PauseBanner,
    IpadArt,
}

const FEATURES: usize = Feature::IpadArt as usize + 1;
/// Each feature's name in the log, in Feature order.
const FEATURE_NAMES: [&str; FEATURES] = [
    "overlay",
    "shift",
    "buttons",
    "touch",
    "pointer",
    "pointer tooltip",
    "event text",
    "tooltip text",
    "zoom",
    "hud left",
    "pause banner",
    "ipad art",
];
/// Features that work only on top of another: each one, then the one it needs.
const NEEDS: [(Feature, Feature); 3] = [
    (Feature::TooltipText, Feature::Overlay),
    (Feature::PointerTooltip, Feature::Pointer),
    (Feature::PauseBanner, Feature::Overlay),
];

/// A function the layer hooks: its feature and name, its address in the API, the
/// replacement, the prologue it must start with, and the slot for the entry that runs the
/// original; none for a stub whose original never runs.
struct Hook {
    feature: Feature,
    name: &'static str,
    func: fn(&Api) -> usize,
    repl: *const (),
    prologue: &'static [u8],
    orig: Option<&'static AtomicUsize>,
}

/// The hooks placed after CApp::OnLoop, in order; a feature's are tried until one fails.
const HOOKS: &[Hook] = &[
    Hook {
        feature: Feature::Overlay,
        name: "MouseControl::OnRender",
        func: |a| a.mouse_render,
        repl: mouse_render_hook as *const (),
        prologue: &MOUSE_RENDER_PROLOGUE,
        orig: Some(&MOUSE_RENDER_ORIG),
    },
    Hook {
        feature: Feature::Shift,
        name: "GetShiftState",
        func: |a| a.get_shift,
        repl: get_shift_hook as *const (),
        prologue: &GET_SHIFT_PROLOGUE,
        orig: Some(&GET_SHIFT_ORIG),
    },
    Hook {
        feature: Feature::Buttons,
        name: "GenericButton::MouseMove",
        func: |a| a.button_move,
        repl: button_move_hook as *const (),
        prologue: &BUTTON_MOVE_PROLOGUE,
        orig: Some(&BUTTON_MOVE_ORIG),
    },
    Hook {
        feature: Feature::Touch,
        name: "CApp::OnTouchDown",
        func: |a| a.touch_down,
        repl: touch_down_hook as *const (),
        prologue: &TOUCH_STUB_PROLOGUE,
        orig: None,
    },
    Hook {
        feature: Feature::Touch,
        name: "CApp::OnTouchMove",
        func: |a| a.touch_move,
        repl: touch_move_hook as *const (),
        prologue: &TOUCH_STUB_PROLOGUE,
        orig: None,
    },
    Hook {
        feature: Feature::Touch,
        name: "CApp::OnTouchUp",
        func: |a| a.touch_up,
        repl: touch_up_hook as *const (),
        prologue: &TOUCH_STUB_PROLOGUE,
        orig: None,
    },
    Hook {
        feature: Feature::Touch,
        name: "CApp::OnTouchCancel",
        func: |a| a.touch_cancel,
        repl: touch_cancel_hook as *const (),
        prologue: &TOUCH_STUB_PROLOGUE,
        orig: None,
    },
    // The pointer hook goes in first: a box drawn scaled must also hit-test scaled.
    Hook {
        feature: Feature::EventText,
        name: "ChoiceBox::MouseMove",
        func: |a| a.choice_mouse_move,
        repl: choice_mouse_move_hook as *const (),
        prologue: &CHOICE_MOUSE_MOVE_PROLOGUE,
        orig: Some(&CHOICE_MOUSE_MOVE_ORIG),
    },
    Hook {
        feature: Feature::EventText,
        name: "ChoiceBox::OnRender",
        func: |a| a.choice_render,
        repl: choice_render_hook as *const (),
        prologue: &CHOICE_RENDER_PROLOGUE,
        orig: Some(&CHOICE_RENDER_ORIG),
    },
    Hook {
        feature: Feature::EventText,
        name: "WindowFrame::Draw",
        func: |a| a.frame_draw,
        repl: frame_draw_hook as *const (),
        prologue: &FRAME_DRAW_PROLOGUE,
        orig: Some(&FRAME_DRAW_ORIG),
    },
    // The measure hook goes in first: a tooltip drawn scaled must be measured scaled.
    Hook {
        feature: Feature::TooltipText,
        name: "MouseControl::MeasureTooltip",
        func: |a| a.measure_tooltip,
        repl: measure_tooltip_hook as *const (),
        prologue: &MEASURE_TOOLTIP_PROLOGUE,
        orig: Some(&MEASURE_TOOLTIP_ORIG),
    },
    Hook {
        feature: Feature::TooltipText,
        name: "GetFontData",
        func: |a| a.font_data,
        repl: font_data_hook as *const (),
        prologue: &FONT_DATA_PROLOGUE,
        orig: Some(&FONT_DATA_ORIG),
    },
    Hook {
        feature: Feature::Zoom,
        name: "CSurface::GL_BlitFrameBuffer",
        func: |a| a.blit_frame,
        repl: blit_frame_hook as *const (),
        prologue: &BLIT_FRAME_PROLOGUE,
        orig: Some(&BLIT_FRAME_ORIG),
    },
    Hook {
        feature: Feature::Pointer,
        name: "CSurface::GL_BlitPixelImage",
        func: |a| a.blit_pixel,
        repl: blit_pixel_hook as *const (),
        prologue: &BLIT_PIXEL_PROLOGUE,
        orig: Some(&BLIT_PIXEL_ORIG),
    },
    Hook {
        feature: Feature::PointerTooltip,
        name: "MouseControl::RenderTooltip",
        func: |a| a.render_tooltip,
        repl: render_tooltip_hook as *const (),
        prologue: &RENDER_TOOLTIP_PROLOGUE,
        orig: Some(&RENDER_TOOLTIP_ORIG),
    },
    Hook {
        feature: Feature::HudLeft,
        name: "CrewBox::CrewBox",
        func: |a| a.crew_box_ctor,
        repl: crew_box_hook as *const (),
        prologue: &CREW_BOX_PROLOGUE,
        orig: Some(&CREW_BOX_ORIG),
    },
    Hook {
        feature: Feature::HudLeft,
        name: "ShipStatus::OnMouseMove",
        func: |a| a.status_mouse_move,
        repl: status_mouse_hook as *const (),
        prologue: &STATUS_MOUSE_PROLOGUE,
        orig: Some(&STATUS_MOUSE_ORIG),
    },
    Hook {
        feature: Feature::HudLeft,
        name: "CrewControl::OnRender",
        func: |a| a.crew_render,
        repl: crew_render_hook as *const (),
        prologue: &CREW_RENDER_PROLOGUE,
        orig: Some(&CREW_RENDER_ORIG),
    },
    Hook {
        feature: Feature::PauseBanner,
        name: "CommandGui::RenderPause",
        func: |a| a.render_pause,
        repl: render_pause_hook as *const (),
        prologue: &RENDER_PAUSE_PROLOGUE,
        orig: Some(&RENDER_PAUSE_ORIG),
    },
    Hook {
        feature: Feature::IpadArt,
        name: "ResourceControl::GetImageId",
        func: |a| a.image_id as usize,
        repl: image_id_hook as *const (),
        prologue: &IMAGE_ID_PROLOGUE,
        orig: Some(&IMAGE_ID_ORIG),
    },
];

/// Places `h` on the game; false when it cannot be.
fn place(h: &Hook, a: &Api) -> bool {
    let Some(entry) = hook(h.name, (h.func)(a), h.repl as usize, h.prologue) else {
        return false;
    };
    if let Some(slot) = h.orig {
        slot.store(entry, Ordering::Relaxed);
    }
    true
}

/// Places HOOKS on the game; returns which features are in, in Feature order.
fn place_hooks(a: &Api) -> [bool; FEATURES] {
    let mut on = [true; FEATURES];
    for h in HOOKS {
        let f = h.feature as usize;
        let base_in = NEEDS.iter().all(|&(top, base)| top != h.feature || on[base as usize]);
        on[f] = on[f] && base_in && place(h, a);
    }
    on
}

/// Runs at load: resolves the game, places the hooks and reads the files. Stays inactive in
/// a process that is not the game.
pub extern "C" fn init() {
    let debug = std::env::var_os(DEBUG_ENV).is_some();
    let Some(exe) = Exe::open() else {
        log!("cannot open the executable");
        return;
    };
    // One pass over the symbol table for the game's API and the sound recovery.
    let wanted: Vec<&str> = SYMBOLS.iter().chain(sound::SYMBOLS).copied().collect();
    let found = exe.symbols(&wanted);
    let (addrs, sound_addrs) = found.split_at(SYMBOLS.len());
    if addrs.iter().all(|&a| a == 0) {
        // Not the game but a launcher or a tool it runs: stay silent.
        if debug {
            log!("not FTL, inactive");
        }
        return;
    }
    if let Some(path) = std::env::var_os(LOG_ENV) {
        open_log_file(Path::new(&path));
    }
    log!("ftlhelm {VERSION} ({REV})");
    let missing: Vec<_> = SYMBOLS.iter().zip(addrs).filter(|(_, &a)| a == 0).map(|(n, _)| *n).collect();
    if !missing.is_empty() {
        for name in missing {
            log!("symbol missing: {name}");
        }
        log!("unknown build, inactive");
        return;
    }
    let id = exe.build_id().unwrap_or_default();
    let Some(build) = build_by_id(&id) else {
        let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
        log!("build id {hex}: unknown build, inactive");
        return;
    };
    crash::log_panics();
    install_layout(&build.layout);
    // SAFETY: every symbol resolved in this FTL build, in SYMBOLS order.
    api::install(unsafe { Api::from_addresses(addrs) });
    let a = api();
    if !mapped_executable(a.onloop) {
        log!("game not mapped in this process, inactive");
        return;
    }
    line_buffer_stdout();
    // The engine drops joystick events while the X window lacks focus.
    // SAFETY: the engine's joystick switch.
    unsafe { (a.joy_enable_unfocused)(1) };
    let Some(onloop) = hook("OnLoop", a.onloop, onloop_hook as *const () as usize, &ONLOOP_PROLOGUE) else {
        log!("inactive");
        return;
    };
    ONLOOP_ORIG.store(onloop, Ordering::Relaxed);
    let on = place_hooks(a);
    let feature = |f: Feature| on[f as usize];
    let touch = feature(Feature::Touch);
    let touch_scan = touch && query_all_touch_devices(a.graphics_init, a.xi_query_device);
    POINTER_TOOLTIP.store(feature(Feature::PointerTooltip), Ordering::Relaxed);
    hud::set_left_hooked(feature(Feature::HudLeft));
    badge::set_pause_hooked(feature(Feature::PauseBanner));
    ipad_art::set_hooked(feature(Feature::IpadArt));
    let step_cap = redirect_step_cap(a.fps_loop);
    let pad_scan = accept_buttons_only_pads(a.scan_joystick);
    let sound_recovery = sound::install(&exe, sound_addrs);
    let mut parts: Vec<(&str, bool)> = FEATURE_NAMES.into_iter().zip(on).collect();
    parts.insert(Feature::Touch as usize + 1, ("touch scan", touch_scan));
    parts.extend([("step cap", step_cap.is_some()), ("pad scan", pad_scan), ("sound recovery", sound_recovery)]);
    let parts: Vec<String> =
        parts.iter().map(|(name, on)| format!("{name} {}", if *on { "on" } else { "off" })).collect();
    log!("active on {}: {}", build.name, parts.join(", "));
    with_layer(|l| {
        l.debug = debug;
        l.touch.hooked = touch;
        l.actions.step_cap = step_cap;
        // SAFETY: the float redirect_step_cap placed, holding the game's cap.
        l.actions.step_cap_normal = step_cap.map_or(0.0, |c| unsafe { (c as *const f32).read() });
        l.files.path = layout_path();
        l.load_layout(DEFAULT_LAYOUT, "built-in default");
        l.poll_files(now_s());
        l.pad.load_sdl_map();
        l.cmd = crate::debug::commands::Commands::open();
    });
}
