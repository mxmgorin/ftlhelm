//! Load-time setup and the hooks: CApp::OnLoop runs the layer once a frame,
//! MouseControl::OnRender draws its overlay, CEvent::GetShiftState reports its Shift, and
//! GenericButton::MouseMove reports the buttons the game hit-tests.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::hook::{hook, mapped_executable, redirect_step_cap, resolve_symbols};
use super::layer::with_layer;
use super::util::{log, now_s};
use crate::game::actions::shift_held;
use crate::game::api::{self, api, Api, Ptr, SYMBOLS};
use crate::game::mem::Obj;
use crate::game::state;
use crate::game::zones::record_button;
use crate::input::pad::active_pad;

const DEBUG_ENV: &str = "FTLPAD_DEBUG";
const HEARTBEAT_S: f64 = 10.0;
const MAX_DT: f64 = 0.05;

/// Used when no layout file is named.
const DEFAULT_LAYOUT: &str =
    "[controls]\na = space\nback = esc\nstart = j\nl2 = mouse_right\nr2 = mouse_left\nright_analog = mouse_movement\n";

/// push %rbp; push %rbx; mov %rdi,%rbx
const ONLOOP_PROLOGUE: [u8; 5] = [0x55, 0x53, 0x48, 0x89, 0xfb];
/// push %r15; mov %rdi,%r15
const MOUSE_RENDER_PROLOGUE: [u8; 5] = [0x41, 0x57, 0x49, 0x89, 0xff];
/// sub $0x8,%rsp; mov $KEY_LEFTSHIFT,%edi
const GET_SHIFT_PROLOGUE: [u8; 9] = [0x48, 0x83, 0xec, 0x08, 0xbf, 0x4b, 0x00, 0x00, 0x00];
/// mov 0x10(%rdi),%eax; movzbl 0x49(%rdi),%r8d
const BUTTON_MOVE_PROLOGUE: [u8; 8] = [0x8b, 0x47, 0x10, 0x44, 0x0f, 0xb6, 0x47, 0x49];

/// Entries that run the hooked functions' original code.
static ONLOOP_ORIG: AtomicUsize = AtomicUsize::new(0);
static MOUSE_RENDER_ORIG: AtomicUsize = AtomicUsize::new(0);
static GET_SHIFT_ORIG: AtomicUsize = AtomicUsize::new(0);
static BUTTON_MOVE_ORIG: AtomicUsize = AtomicUsize::new(0);

/// The original of a hooked `extern "C"` function of type F.
/// # Safety
/// `slot` must hold an entry hook() returned for a function of type F.
unsafe fn orig<F: Copy>(slot: &AtomicUsize) -> F {
    let addr = slot.load(Ordering::Relaxed);
    std::mem::transmute_copy::<usize, F>(&addr)
}

/// Frame timing kept between OnLoop calls.
struct Clock {
    last_t: f64,
    last_beat: f64,
    frames: u64,
}

static CLOCK: std::sync::Mutex<Clock> = std::sync::Mutex::new(Clock { last_t: 0.0, last_beat: 0.0, frames: 0 });

extern "C" fn onloop_hook(app: Ptr) {
    state::set_app(app);
    let t = now_s();
    let (dt, frames, beat) = {
        let mut c = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
        c.frames += 1;
        let dt = if c.last_t > 0.0 { (t - c.last_t).min(MAX_DT) as f32 } else { 0.0 };
        c.last_t = t;
        let beat = t - c.last_beat >= HEARTBEAT_S;
        if beat {
            c.last_beat = t;
        }
        (dt, c.frames, beat)
    };
    if frames == 1 {
        log!("first OnLoop");
    }
    with_layer(|l| {
        l.poll_files(t);
        l.run_queued_clicks();
        l.sync_cursor();
        l.build_zones();
        if l.cfg.focus_first {
            l.focus_first_on_open();
        }
        l.poll_commands();
        let dev = active_pad();
        l.pad.index = dev;
        let pad = dev.map(|d| l.read_pad(d)).unwrap_or_default();
        l.apply_pad(pad, t, dt);
        if l.debug && beat {
            log!(
                "{frames} frames, pad {}, cursor {},{}, depth {}",
                dev.unwrap_or(-1),
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
        // SAFETY: the original MouseControl::OnRender on its own object.
        unsafe { orig::<extern "C" fn(Ptr)>(&MOUSE_RENDER_ORIG)(this) };
    }
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
    let Some(addrs) = resolve_symbols(SYMBOLS) else {
        log!("unknown build, inactive");
        return;
    };
    // SAFETY: every symbol resolved in this FTL build, in SYMBOLS order.
    api::install(unsafe { Api::from_addresses(&addrs) });
    let a = api();
    if !mapped_executable(a.onloop) {
        log!("game not mapped in this process, inactive");
        return;
    }
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
    let step_cap = redirect_step_cap(a.fps_loop);
    let on = |b: bool| if b { "on" } else { "off" };
    log!(
        "active: overlay {}, shift {}, buttons {}, step cap {}",
        on(overlay),
        on(shift),
        on(buttons),
        on(step_cap.is_some())
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
