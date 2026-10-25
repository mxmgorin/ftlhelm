//! Load-time setup and the hooks:
//! - CApp::OnLoop runs the layer once a frame;
//! - MouseControl::OnRender draws the overlay;
//! - CEvent::GetShiftState adds the layer's Shift;
//! - GenericButton::MouseMove records the buttons the game hit-tests;
//! - ChoiceBox::OnRender, WindowFrame::Draw and ChoiceBox::MouseMove draw and hit-test
//!   event boxes scaled;
//! - MouseControl::MeasureTooltip and ResourceControl::GetFontData scale tooltip fonts;
//! - the PLT calls to snd_pcm_open and snd_pcm_recover restart a failed sound stream.

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::hook::{accept_buttons_only_pads, build_id, hook, mapped_executable, redirect_step_cap, resolve_symbols};
use super::layer::with_layer;
use super::util::{line_buffer_stdout, log, now_s};
use crate::game::actions::shift_held;
use crate::game::api::{self, api, Api, FontData, Point, Ptr, SYMBOLS};
use crate::game::mem::Obj;
use crate::game::zones::record_button;
use crate::game::{sound, state};
use crate::ui::text;

const DEBUG_ENV: &str = "FTLPAD_DEBUG";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Git revision of the build.
const REV: &str = env!("FTLPAD_REV");

/// Used when no layout file is named.
const DEFAULT_LAYOUT: &str =
    "[controls]\na = space\nback = esc\nstart = j\nl2 = mouse_right\nr2 = mouse_left\nright_analog = mouse_movement\n";

/// GNU build ID of the FTL.amd64 whose layout `offsets` describes.
const BUILD_ID: [u8; 20] = [
    0x93, 0x1e, 0x2d, 0xdf, 0x71, 0x29, 0x32, 0x97, 0x75, 0x91, 0x05, 0x48, 0x6b, 0x73, 0x58, 0x1b, 0x97, 0xbe, 0xb8,
    0xcf,
];

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
/// sub $0x8,%rsp; mov $KEY_LEFTSHIFT,%edi
const GET_SHIFT_PROLOGUE: [u8; 9] = [0x48, 0x83, 0xec, 0x08, 0xbf, 0x4b, 0x00, 0x00, 0x00];
/// mov 0x10(%rdi),%eax; movzbl 0x49(%rdi),%r8d
const BUTTON_MOVE_PROLOGUE: [u8; 8] = [0x8b, 0x47, 0x10, 0x44, 0x0f, 0xb6, 0x47, 0x49];

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
        let frame = l.clock.tick(t);
        if frame.count == 1 {
            log!("first OnLoop");
            sound::report();
        }
        l.poll_files(t);
        text::update(l.cfg.text_scale, state::app());
        l.run_queued_clicks();
        l.sync_cursor();
        l.build_zones();
        if l.cfg.focus_first {
            l.focus_first_on_open();
        }
        l.poll_commands();
        let pad = l.poll_pad();
        l.apply_pad(pad, t, frame.dt);
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
    let draw_pointer = with_layer(|l| l.draw_overlay()).unwrap_or(true);
    if draw_pointer {
        let _tooltip = text::TooltipScope::enter();
        // SAFETY: the original MouseControl::OnRender on its own object.
        unsafe { orig::<extern "C" fn(Ptr)>(&MOUSE_RENDER_ORIG)(this) };
    }
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

extern "C" fn get_shift_hook() -> bool {
    // SAFETY: the original CEvent::GetShiftState.
    shift_held() || unsafe { orig::<extern "C" fn() -> bool>(&GET_SHIFT_ORIG)() }
}

extern "C" fn button_move_hook(this: Ptr, x: i32, y: i32, silent: bool) {
    // SAFETY: the game passes its GenericButton as `this`.
    if let Some(button) = unsafe { Obj::from_ptr(this as *const c_void) } {
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
        // Not the game: a launcher script or a tool it runs, which stays silent.
        if debug {
            log!("not FTL, inactive");
        }
        return;
    }
    log!("ftlpad {VERSION} ({REV})");
    let missing: Vec<_> = SYMBOLS.iter().zip(&addrs).filter(|(_, &a)| a == 0).map(|(n, _)| *n).collect();
    if !missing.is_empty() {
        for name in missing {
            log!("symbol missing: {name}");
        }
        log!("unknown build, inactive");
        return;
    }
    let id = build_id().unwrap_or_default();
    if id != BUILD_ID {
        let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
        log!("build id {hex}: unknown build, inactive");
        return;
    }
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
    let step_cap = redirect_step_cap(a.fps_loop);
    let pad_scan = accept_buttons_only_pads(a.scan_joystick);
    let sound_recovery = sound::install();
    let on = |b: bool| if b { "on" } else { "off" };
    log!(
        "active: overlay {}, shift {}, buttons {}, event text {}, tooltip text {}, step cap {}, pad scan {}, \
         sound recovery {}",
        on(overlay),
        on(shift),
        on(buttons),
        on(event_text),
        on(tooltip_text),
        on(step_cap.is_some()),
        on(pad_scan),
        on(sound_recovery)
    );
    with_layer(|l| {
        l.debug = debug;
        l.actions.step_cap = step_cap;
        // SAFETY: the float redirect_step_cap placed, holding the game's cap.
        l.actions.step_cap_normal = step_cap.map_or(0.0, |c| unsafe { (c as *const f32).read() });
        l.load_layout(DEFAULT_LAYOUT, "built-in default");
        l.poll_files(now_s());
        l.pad.load_sdl_map();
        l.cmd = crate::debug::commands::Commands::open();
    });
}
