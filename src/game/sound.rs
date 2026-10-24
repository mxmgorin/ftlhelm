//! Restarts the game's sound stream when ALSA cannot recover it.
//!
//! The engine's playback thread calls snd_pcm_recover after an underrun and goes silent for
//! good when that fails. Its PLT slot is pointed here: on failure the device is opened again
//! under the name the game opened it with, set up by the game's configure_pcm at the mixer's
//! rate, and swapped into pcm_handle. The name is recorded through the snd_pcm_open slot, as
//! a plugin may rename the stream. The old stream is closed only once the new one works.
//! After start-up every user of pcm_handle, the recovery included, runs on the playback
//! thread.
//!
//! Logged: each open's device and plugin chain, the first frame's rate and sizes, the first
//! failure's full chain dump. ALSA is reached through dlsym: the library links only what
//! box64 provides.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::core::hook::{plt_slot, redirect_slot, resolve_symbols, slot_target};
use crate::core::util::{log, now_s};

const RECOVER: &str = "snd_pcm_recover";
const OPEN: &str = "snd_pcm_open";
/// The game's PCM handle, its stream setup, and the rate and sizes that setup chose.
const SYMBOLS: &[&str] = &["pcm_handle", "configure_pcm", "hw_sound_rate", "hw_buffer_size", "hw_period_size"];
/// A dump line that is not a plugin's header.
const DUMP_SETUP_HEADER: &str = "Its setup is:";
const SND_PCM_STREAM_PLAYBACK: c_int = 0;
/// configure_pcm's second argument: keep the rate the mixer already runs at.
const KEEP_RATE: c_int = 1;
/// Least time between two reopen attempts.
const RETRY_S: f64 = 1.0;

type Pcm = *mut c_void;
type Output = *mut c_void;

struct Alsa {
    recover: unsafe extern "C" fn(Pcm, c_int, c_int) -> c_int,
    open: unsafe extern "C" fn(*mut Pcm, *const c_char, c_int, c_int) -> c_int,
    close: unsafe extern "C" fn(Pcm) -> c_int,
    strerror: unsafe extern "C" fn(c_int) -> *const c_char,
    dump: unsafe extern "C" fn(Pcm, Output) -> c_int,
    output_buffer_open: unsafe extern "C" fn(*mut Output) -> c_int,
    output_buffer_string: unsafe extern "C" fn(Output, *mut *mut c_char) -> usize,
    output_close: unsafe extern "C" fn(Output) -> c_int,
}

struct Sound {
    alsa: Alsa,
    pcm_handle: *mut Pcm,
    configure: unsafe extern "C" fn(c_int, c_int) -> c_int,
    sound_rate: *const c_int,
    buffer_size: *const c_int,
    period_size: *const c_int,
    recover_slot: usize,
    open_slot: usize,
}

// SAFETY: the pointers name the game's globals, alive for the process; only the playback
// thread dereferences them.
unsafe impl Send for Sound {}
unsafe impl Sync for Sound {}

static SOUND: OnceLock<Sound> = OnceLock::new();
static LAST_TRY: Mutex<f64> = Mutex::new(f64::NEG_INFINITY);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static DUMPED: AtomicBool = AtomicBool::new(false);
static REOPENS: AtomicUsize = AtomicUsize::new(0);
/// The device name the game opened its playback stream with.
static DEVICE: Mutex<Option<CString>> = Mutex::new(None);

/// Looks up an exported function; None when no loaded library has it.
fn lookup<F: Copy>(name: &CStr) -> Option<F> {
    // SAFETY: dlsym with a NUL-terminated name has no other preconditions.
    let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    // SAFETY: the caller names F as the C type of the function exported under `name`.
    (!p.is_null()).then(|| unsafe { std::mem::transmute_copy::<*mut c_void, F>(&p) })
}

impl Alsa {
    fn resolve() -> Option<Alsa> {
        Some(Alsa {
            recover: lookup(c"snd_pcm_recover")?,
            open: lookup(c"snd_pcm_open")?,
            close: lookup(c"snd_pcm_close")?,
            strerror: lookup(c"snd_strerror")?,
            dump: lookup(c"snd_pcm_dump")?,
            output_buffer_open: lookup(c"snd_output_buffer_open")?,
            output_buffer_string: lookup(c"snd_output_buffer_string")?,
            output_close: lookup(c"snd_output_close")?,
        })
    }

    fn error(&self, err: c_int) -> String {
        // SAFETY: snd_strerror returns a static string for any code.
        unsafe { CStr::from_ptr((self.strerror)(err)) }.to_string_lossy().into_owned()
    }

    /// Returns snd_pcm_dump's text for the PCM: its plugin chain and their settings.
    fn dump_text(&self, pcm: Pcm) -> String {
        let mut out: Output = null_mut();
        let mut dump = String::new();
        // SAFETY: a fresh buffer output, written by snd_pcm_dump and closed here.
        unsafe {
            if (self.output_buffer_open)(&mut out) < 0 {
                return dump;
            }
            (self.dump)(pcm, out);
            let mut text: *mut c_char = null_mut();
            let len = (self.output_buffer_string)(out, &mut text);
            if !text.is_null() {
                dump = String::from_utf8_lossy(std::slice::from_raw_parts(text as *const u8, len)).into_owned();
            }
            (self.output_close)(out);
        }
        dump
    }

    /// Logs the PCM's whole dump, one log line per dump line.
    fn dump(&self, pcm: Pcm) {
        for line in self.dump_text(pcm).lines() {
            log!("sound: {line}");
        }
    }

    /// Returns the PCM's plugin chain: the dump's plugin headers, outermost first.
    fn chain(&self, pcm: Pcm) -> String {
        let dump = self.dump_text(pcm);
        let headers: Vec<&str> = dump
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with(char::is_whitespace) && *l != DUMP_SETUP_HEADER)
            .collect();
        headers.join(" -> ")
    }
}

impl Sound {
    /// Opens the game's device again and swaps it in; returns why it could not.
    fn reopen(&self) -> Result<(), String> {
        // SAFETY: pcm_handle is the game's global; only this thread uses it after start-up.
        let old = unsafe { *self.pcm_handle };
        if !DUMPED.swap(true, Ordering::Relaxed) {
            self.alsa.dump(old);
        }
        let name = DEVICE.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or("device name not recorded")?;
        let mut new: Pcm = null_mut();
        // SAFETY: opens a playback PCM into a local handle.
        let err = unsafe { (self.alsa.open)(&mut new, name.as_ptr(), SND_PCM_STREAM_PLAYBACK, 0) };
        if err < 0 {
            return Err(format!("open {}: {}", name.to_string_lossy(), self.alsa.error(err)));
        }
        // SAFETY: configure_pcm sets up whatever pcm_handle holds; the old stream stays
        // open until the new one is set up, and comes back if it is not.
        unsafe {
            *self.pcm_handle = new;
            if (self.configure)(*self.buffer_size, KEEP_RATE) == 0 {
                *self.pcm_handle = old;
                (self.alsa.close)(new);
                return Err("configure_pcm failed".to_string());
            }
            (self.alsa.close)(old);
        }
        Ok(())
    }
}

/// Stands in for snd_pcm_recover: recovers as ALSA does, and reopens the stream when that
/// fails; returns 0 once the stream plays again, else ALSA's error.
extern "C" fn recover_hook(pcm: Pcm, err: c_int, silent: c_int) -> c_int {
    let s = SOUND.get().expect("the slot is redirected only after SOUND is set");
    // SAFETY: the real snd_pcm_recover with the game's own arguments.
    let res = unsafe { (s.alsa.recover)(pcm, err, silent) };
    if res >= 0 {
        return res;
    }
    {
        let now = now_s();
        let mut last = LAST_TRY.lock().unwrap_or_else(|e| e.into_inner());
        if now - *last < RETRY_S {
            return res;
        }
        *last = now;
    }
    match s.reopen() {
        Ok(()) => {
            let n = REOPENS.fetch_add(1, Ordering::Relaxed) + 1;
            log!("sound: recover failed ({}), stream reopened, {n} so far", s.alsa.error(res));
            0
        }
        Err(why) => {
            log!("sound: recover failed ({}), reopen failed: {why}", s.alsa.error(res));
            res
        }
    }
}

/// Stands in for snd_pcm_open: opens as ALSA does; returns ALSA's result. Records the
/// device name of a playback stream that opened.
extern "C" fn open_hook(pcm: *mut Pcm, name: *const c_char, stream: c_int, mode: c_int) -> c_int {
    let s = SOUND.get().expect("the slot is redirected only after SOUND is set");
    // SAFETY: the real snd_pcm_open with the game's own arguments.
    let res = unsafe { (s.alsa.open)(pcm, name, stream, mode) };
    if res >= 0 && stream == SND_PCM_STREAM_PLAYBACK && !name.is_null() {
        // SAFETY: the game passes a NUL-terminated device name.
        let device = unsafe { CStr::from_ptr(name) }.to_owned();
        // SAFETY: ALSA just stored the opened PCM through `pcm`.
        let chain = s.alsa.chain(unsafe { *pcm });
        log!("sound: opened {}: {chain}", device.to_string_lossy());
        *DEVICE.lock().unwrap_or_else(|e| e.into_inner()) = Some(device);
    }
    res
}

/// Points the game's snd_pcm_open and snd_pcm_recover calls at open_hook and
/// recover_hook; returns false with the reason logged when ALSA, a game symbol or a PLT
/// slot is missing.
pub fn install() -> bool {
    let Some(alsa) = Alsa::resolve() else {
        log!("sound: ALSA functions not found");
        return false;
    };
    let addrs = resolve_symbols(SYMBOLS);
    if let Some(i) = addrs.iter().position(|&a| a == 0) {
        log!("sound: symbol missing: {}", SYMBOLS[i]);
        return false;
    }
    let [pcm_handle, configure, sound_rate, buffer_size, period_size]: [usize; SYMBOLS.len()] =
        addrs.try_into().expect("resolve_symbols gives one address per symbol");
    let (Some(recover_slot), Some(open_slot)) = (plt_slot(RECOVER), plt_slot(OPEN)) else {
        log!("sound: no PLT slot for {RECOVER} or {OPEN}");
        return false;
    };
    let sound = Sound {
        alsa,
        pcm_handle: pcm_handle as *mut Pcm,
        // SAFETY: configure_pcm is the game's int configure_pcm(int buffer_size, int force_rate).
        configure: unsafe {
            std::mem::transmute_copy::<usize, unsafe extern "C" fn(c_int, c_int) -> c_int>(&configure)
        },
        sound_rate: sound_rate as *const c_int,
        buffer_size: buffer_size as *const c_int,
        period_size: period_size as *const c_int,
        recover_slot,
        open_slot,
    };
    if SOUND.set(sound).is_err() {
        return false;
    }
    // The open hook goes first: a recovery needs the name it records.
    if redirect_slot(open_slot, open_hook as *const () as usize).is_none()
        || redirect_slot(recover_slot, recover_hook as *const () as usize).is_none()
    {
        log!("sound: PLT slot not writable");
        return false;
    }
    ACTIVE.store(true, Ordering::Relaxed);
    true
}

/// Logs the stream's rate and sizes, and any redirected PLT slot the loader has since
/// rewritten.
pub fn report() {
    let Some(s) = SOUND.get() else {
        return;
    };
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    // SAFETY: the game's ints, set by configure_pcm; a torn read only skews this line.
    let (rate, buffer, period) = unsafe { (*s.sound_rate, *s.buffer_size, *s.period_size) };
    log!("sound: {rate} Hz, buffer {buffer} frames, period {period} frames");
    for (slot, hook, name) in [
        (s.recover_slot, recover_hook as *const () as usize, RECOVER),
        (s.open_slot, open_hook as *const () as usize, OPEN),
    ] {
        if slot_target(slot) != hook {
            log!("sound: {name} slot rewritten by the loader, recovery off");
        }
    }
}
