//! Load-time setup and the hooks:
//! - CApp::OnLoop runs the layer once a frame, and installs the crash report on the first;
//! - MouseControl::OnRender draws the overlay around the game's pointer;
//!   CSurface::GL_BlitPixelImage draws the plain pointer larger on a small screen, or not
//!   while touch hides it, and MouseControl::RenderTooltip moves its tooltip below it;
//! - CEvent::GetShiftState adds the layer's Shift;
//! - CApp::OnTouchDown, OnTouchMove, OnTouchUp and OnTouchCancel queue touches, and the
//!   engine looks for a touchscreen among all input devices;
//! - GenericButton::MouseMove records the buttons the game hit-tests;
//! - ChoiceBox::OnRender, WindowFrame::Draw and ChoiceBox::MouseMove draw and hit-test
//!   event boxes scaled;
//! - MouseControl::MeasureTooltip and ResourceControl::GetFontData scale tooltip fonts;
//! - CSurface::GL_BlitFrameBuffer shows the zoomed view and averages a shrunk frame;
//! - the PLT calls to snd_pcm_open and snd_pcm_recover restart a failed sound stream.

use std::ffi::{c_int, c_void};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use super::crash::{self, Phase};
use super::hook::{
    accept_buttons_only_pads, build_id, hook, mapped_executable, query_all_touch_devices, redirect_step_cap,
    resolve_symbols,
};
use super::layer::with_layer;
use super::util::{line_buffer_stdout, log, now_s, open_log_file};
use crate::config::layout::DEFAULT_LAYOUT;
use crate::game::actions::shift_held;
use crate::game::api::{self, api, Api, FontData, GlColor, Point, Ptr, SYMBOLS};
use crate::game::mem::Obj;
use crate::game::offsets::{build_by_id, install_layout, MOUSE_INVALID_POINTER, MOUSE_VALID_POINTER};
use crate::game::zones::record_button;
use crate::game::{sound, state};
use crate::input::touch::{self, TouchPhase};
use crate::ui::{shrink, text, zoom};

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
static MEASURE_TOOLTIP_ORIG: AtomicUsize = AtomicUsize::new(0);
static CHOICE_MOUSE_MOVE_ORIG: AtomicUsize = AtomicUsize::new(0);
static FRAME_DRAW_ORIG: AtomicUsize = AtomicUsize::new(0);
static BLIT_FRAME_ORIG: AtomicUsize = AtomicUsize::new(0);
static BLIT_PIXEL_ORIG: AtomicUsize = AtomicUsize::new(0);
static RENDER_TOOLTIP_ORIG: AtomicUsize = AtomicUsize::new(0);
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
        text::update(l.cfg.text_scale, state::app(), l.zoom.target);
        shrink::configure(l.cfg.frame_filter, l.cfg.sharpen);
        l.run_queued_clicks();
        l.sync_cursor();
        l.build_zones();
        if l.cfg.focus_first {
            l.focus_first_on_open();
        }
        l.poll_commands();
        l.apply_touch(t);
        let pad = l.poll_pad(frame.dt);
        l.apply_pad(pad, t, frame.dt);
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
        (l.draw_overlay(), l.touch_in_use(), l.pointer_scale())
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

fn install(name: &str, func: usize, repl: usize, prologue: &[u8], slot: &AtomicUsize) -> bool {
    match hook(name, func, repl, prologue) {
        Some(entry) => {
            slot.store(entry, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Runs at load: resolves the game, places the hooks and reads the files. Stays inactive in
/// a process that is not the game.
pub extern "C" fn init() {
    let debug = std::env::var_os(DEBUG_ENV).is_some();
    let addrs = resolve_symbols(SYMBOLS);
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
    let missing: Vec<_> = SYMBOLS.iter().zip(&addrs).filter(|(_, &a)| a == 0).map(|(n, _)| *n).collect();
    if !missing.is_empty() {
        for name in missing {
            log!("symbol missing: {name}");
        }
        log!("unknown build, inactive");
        return;
    }
    let id = build_id().unwrap_or_default();
    let Some(build) = build_by_id(&id) else {
        let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
        log!("build id {hex}: unknown build, inactive");
        return;
    };
    crash::log_panics();
    install_layout(&build.layout);
    // SAFETY: every symbol resolved in this FTL build, in SYMBOLS order.
    api::install(unsafe { Api::from_addresses(&addrs) });
    let a = api();
    if !mapped_executable(a.onloop) {
        log!("game not mapped in this process, inactive");
        return;
    }
    line_buffer_stdout();
    // The engine drops joystick events while the X window lacks focus.
    // SAFETY: the engine's joystick switch.
    unsafe { (a.joy_enable_unfocused)(1) };
    if !install("OnLoop", a.onloop, onloop_hook as *const () as usize, &ONLOOP_PROLOGUE, &ONLOOP_ORIG) {
        log!("inactive");
        return;
    }
    let overlay = install(
        "MouseControl::OnRender",
        a.mouse_render,
        mouse_render_hook as *const () as usize,
        &MOUSE_RENDER_PROLOGUE,
        &MOUSE_RENDER_ORIG,
    );
    let shift = install(
        "GetShiftState",
        a.get_shift,
        get_shift_hook as *const () as usize,
        &GET_SHIFT_PROLOGUE,
        &GET_SHIFT_ORIG,
    );
    let buttons = install(
        "GenericButton::MouseMove",
        a.button_move,
        button_move_hook as *const () as usize,
        &BUTTON_MOVE_PROLOGUE,
        &BUTTON_MOVE_ORIG,
    );
    let touch_hooks = [
        ("CApp::OnTouchDown", a.touch_down, touch_down_hook as *const () as usize),
        ("CApp::OnTouchMove", a.touch_move, touch_move_hook as *const () as usize),
        ("CApp::OnTouchUp", a.touch_up, touch_up_hook as *const () as usize),
        ("CApp::OnTouchCancel", a.touch_cancel, touch_cancel_hook as *const () as usize),
    ];
    let touch = touch_hooks.map(|(name, func, repl)| hook(name, func, repl, &TOUCH_STUB_PROLOGUE).is_some());
    let touch = touch.iter().all(|&on| on);
    let touch_scan = touch && query_all_touch_devices(a.graphics_init, a.xi_query_device);
    // The pointer hook goes in first: a box drawn scaled must also hit-test scaled.
    let event_text = install(
        "ChoiceBox::MouseMove",
        a.choice_mouse_move,
        choice_mouse_move_hook as *const () as usize,
        &CHOICE_MOUSE_MOVE_PROLOGUE,
        &CHOICE_MOUSE_MOVE_ORIG,
    ) && install(
        "ChoiceBox::OnRender",
        a.choice_render,
        choice_render_hook as *const () as usize,
        &CHOICE_RENDER_PROLOGUE,
        &CHOICE_RENDER_ORIG,
    ) && install(
        "WindowFrame::Draw",
        a.frame_draw,
        frame_draw_hook as *const () as usize,
        &FRAME_DRAW_PROLOGUE,
        &FRAME_DRAW_ORIG,
    );
    // The measure hook goes in first: a tooltip drawn scaled must be measured scaled.
    let tooltip_text = overlay
        && install(
            "MouseControl::MeasureTooltip",
            a.measure_tooltip,
            measure_tooltip_hook as *const () as usize,
            &MEASURE_TOOLTIP_PROLOGUE,
            &MEASURE_TOOLTIP_ORIG,
        )
        && install(
            "GetFontData",
            a.font_data,
            font_data_hook as *const () as usize,
            &FONT_DATA_PROLOGUE,
            &FONT_DATA_ORIG,
        );
    let zoom = install(
        "CSurface::GL_BlitFrameBuffer",
        a.blit_frame,
        blit_frame_hook as *const () as usize,
        &BLIT_FRAME_PROLOGUE,
        &BLIT_FRAME_ORIG,
    );
    let pointer = install(
        "CSurface::GL_BlitPixelImage",
        a.blit_pixel,
        blit_pixel_hook as *const () as usize,
        &BLIT_PIXEL_PROLOGUE,
        &BLIT_PIXEL_ORIG,
    );
    let pointer_tooltip = pointer
        && install(
            "MouseControl::RenderTooltip",
            a.render_tooltip,
            render_tooltip_hook as *const () as usize,
            &RENDER_TOOLTIP_PROLOGUE,
            &RENDER_TOOLTIP_ORIG,
        );
    POINTER_TOOLTIP.store(pointer_tooltip, Ordering::Relaxed);
    let step_cap = redirect_step_cap(a.fps_loop);
    let pad_scan = accept_buttons_only_pads(a.scan_joystick);
    let sound_recovery = sound::install();
    let on = |b: bool| if b { "on" } else { "off" };
    log!(
        "active on {}: overlay {}, shift {}, buttons {}, touch {}, touch scan {}, pointer {}, \
         pointer tooltip {}, event text {}, tooltip text {}, zoom {}, step cap {}, pad scan {}, \
         sound recovery {}",
        build.name,
        on(overlay),
        on(shift),
        on(buttons),
        on(touch),
        on(touch_scan),
        on(pointer),
        on(pointer_tooltip),
        on(event_text),
        on(tooltip_text),
        on(zoom),
        on(step_cap.is_some()),
        on(pad_scan),
        on(sound_recovery)
    );
    with_layer(|l| {
        l.debug = debug;
        l.touch.hooked = touch;
        l.actions.step_cap = step_cap;
        // SAFETY: the float redirect_step_cap placed, holding the game's cap.
        l.actions.step_cap_normal = step_cap.map_or(0.0, |c| unsafe { (c as *const f32).read() });
        l.load_layout(DEFAULT_LAYOUT, "built-in default");
        l.poll_files(now_s());
        l.pad.load_sdl_map();
        l.cmd = crate::debug::commands::Commands::open();
    });
}
