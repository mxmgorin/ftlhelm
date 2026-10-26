//! The frame shown smaller than it is drawn, each screen pixel the sharpened average of the
//! frame area it covers.
//!
//! The engine draws every primitive with a shader it generates through three callbacks: one
//! keys the primitive's features, two write the vertex and fragment sources for a key not
//! yet in its table. The layer installs its own key and fragment callbacks over the
//! engine's. While a shrunk frame blit draws, the key gains FILTER_KEY and the fragment
//! source is the area filter; every other draw gets the engine's own key and source.
//!
//! Per axis, the filter pairs up the texels under a screen pixel's footprint and reads each
//! pair with one bilinear tap placed so the hardware applies the pair's box weights; the
//! pairs' weighted sum is the exact box average. An unsharp mask then pushes that average
//! SHARPEN times its difference away from a blur: four bilinear taps half a screen pixel out
//! along the diagonals. The engine prepends its GLSL header and frees the source after
//! compiling it, so the source is in its dialect (`in`, `color_out`, `texp`) and copied into
//! its heap.
//!
//! The generator is installed at the first shrunk blit. Where the engine refuses it, or the
//! filter fails to build, blits keep the game's own filter.
//!
//! The frame blit is one textured quad whose texture is frame-sized: the whole draw takes
//! the filter's key, and `size` maps its texcoords to texels.

use std::ffi::{c_char, c_int, c_uint, CStr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use crate::core::util::log;
use crate::game::api::api;

/// The key the engine's generator gives a primitive it cannot draw.
const INVALID_KEY: c_uint = !0;
/// Marks the filter's key; the engine's own keys stay below it.
const FILTER_KEY: c_uint = 1 << 31;
/// A table entry's program once its shaders failed to build.
const FAILED_PROGRAM: c_uint = !0;
/// The shader table size and growth the engine sets up for its own generator.
const TABLE_SIZE: c_int = 0xf00;
const TABLE_GROWS: c_int = 0;
/// The unsharp mask's amount.
const SHARPEN: f32 = 0.5;
/// The engine allocator's default alignment and no flags.
const ALLOC_DEFAULT_ALIGN: c_int = 0;
const ALLOC_NO_FLAGS: c_int = 0;

// Two pairs per axis hold a footprint of up to three texels; a wider one averages the four
// texels from its start.
const FRAGMENT_SOURCE: &CStr = c"
uniform lowp sampler2D tex;
uniform highp vec2 size;
uniform highp vec2 span;
uniform highp float sharpen;
in texp vec2 texcoord_varying;

highp vec2 pair(highp float lo, highp float hi, highp float first) {
    highp float w0 = clamp(min(hi, first + 1.0) - max(lo, first), 0.0, 1.0);
    highp float w1 = clamp(min(hi, first + 2.0) - max(lo, first + 1.0), 0.0, 1.0);
    highp float w = w0 + w1;
    return vec2(first + 0.5 + (w > 0.0 ? w1 / w : 0.0), w);
}

void main() {
    highp vec2 inv = 1.0 / size;
    highp vec2 center = texcoord_varying * size;
    highp vec2 lo = center - 0.5 * span;
    highp vec2 hi = center + 0.5 * span;
    highp vec2 first = floor(lo);
    highp vec2 x0 = pair(lo.x, hi.x, first.x);
    highp vec2 x1 = pair(lo.x, hi.x, first.x + 2.0);
    highp vec2 y0 = pair(lo.y, hi.y, first.y);
    highp vec2 y1 = pair(lo.y, hi.y, first.y + 2.0);
    highp vec4 area = (texture2D(tex, vec2(x0.x, y0.x) * inv) * (x0.y * y0.y)
        + texture2D(tex, vec2(x1.x, y0.x) * inv) * (x1.y * y0.y)
        + texture2D(tex, vec2(x0.x, y1.x) * inv) * (x0.y * y1.y)
        + texture2D(tex, vec2(x1.x, y1.x) * inv) * (x1.y * y1.y))
        / ((x0.y + x1.y) * (y0.y + y1.y));
    highp vec2 d = 0.5 * span;
    highp vec4 blur = 0.25 * (texture2D(tex, (center - d) * inv)
        + texture2D(tex, (center + vec2(d.x, -d.y)) * inv)
        + texture2D(tex, (center + vec2(-d.x, d.y)) * inv)
        + texture2D(tex, (center + d) * inv));
    color_out = clamp(area + sharpen * (area - blur), 0.0, 1.0);
}
";

/// Whether the draw in progress is a shrunk frame blit.
static FILTERING: AtomicBool = AtomicBool::new(false);
/// The last key handed out for the filter.
static LAST_FILTER_KEY: AtomicU32 = AtomicU32::new(INVALID_KEY);

enum State {
    Uninstalled,
    /// The generator is in; the filter's uniform ids, and whether its build is confirmed.
    Installed {
        size: c_int,
        span: c_int,
        confirmed: bool,
    },
    Failed,
}

static STATE: Mutex<State> = Mutex::new(State::Uninstalled);

/// The key for a primitive the engine keyed as `key`: marked as the filter's while a shrunk
/// frame blit draws.
fn filter_key(key: c_uint, filtering: bool) -> c_uint {
    if filtering && key != INVALID_KEY {
        key | FILTER_KEY
    } else {
        key
    }
}

// The engine's callback signature: a primitive's type, its attribute counts and render state.
#[allow(clippy::too_many_arguments)]
extern "C" fn shader_key(
    a0: c_int,
    a1: c_int,
    a2: c_int,
    a3: c_int,
    a4: c_int,
    a5: c_int,
    a6: c_int,
    a7: c_int,
    a8: c_int,
    a9: c_int,
) -> c_uint {
    // SAFETY: the engine's own key callback with the arguments the engine passed.
    let key = unsafe { (api().default_shader_key)(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9) };
    let key = filter_key(key, FILTERING.load(Ordering::Relaxed));
    if key & FILTER_KEY != 0 {
        LAST_FILTER_KEY.store(key, Ordering::Relaxed);
    }
    key
}

// The engine's callback signature: a primitive's type, its attribute counts and render state.
#[allow(clippy::too_many_arguments)]
extern "C" fn fragment_source(
    a0: c_int,
    a1: c_int,
    a2: c_int,
    a3: c_int,
    a4: c_int,
    a5: c_int,
    a6: c_int,
    a7: c_int,
    a8: c_int,
    a9: c_int,
) -> *mut c_char {
    let a = api();
    if !FILTERING.load(Ordering::Relaxed) {
        // SAFETY: the engine's own source callback with the arguments the engine passed.
        return unsafe { (a.default_fragment_source)(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9) };
    }
    let len = FRAGMENT_SOURCE.count_bytes() + 1;
    // SAFETY: the engine's allocator, which the engine frees the source with; the copy
    // includes the terminating NUL.
    unsafe {
        let buf = (a.mem_alloc)(len as i64, ALLOC_DEFAULT_ALIGN, ALLOC_NO_FLAGS) as *mut c_char;
        if !buf.is_null() {
            std::ptr::copy_nonoverlapping(FRAGMENT_SOURCE.as_ptr(), buf, len);
        }
        buf
    }
}

/// Installs the generator and the filter's uniforms; Failed, logged, when the engine
/// refuses them.
fn install() -> State {
    let a = api();
    // SAFETY: the engine's generator and uniform setup with callbacks of its signatures and
    // NUL-terminated names; it rebuilds its shaders on their next use.
    unsafe {
        let set =
            (a.set_shader_generator)(a.default_vertex_source, fragment_source, shader_key, TABLE_SIZE, TABLE_GROWS);
        let size = (a.add_shader_uniform)(c"size".as_ptr());
        let span = (a.add_shader_uniform)(c"span".as_ptr());
        let sharpen = (a.add_shader_uniform)(c"sharpen".as_ptr());
        if set == 0 || size == 0 || span == 0 || sharpen == 0 {
            log!("frame filter: not installed, game's own");
            return State::Failed;
        }
        (a.set_shader_uniform_float)(sharpen, SHARPEN);
        State::Installed { size, span, confirmed: false }
    }
}

/// Whether the engine built the filter's shaders for the last filtered draw; None while no
/// draw has asked for the filter's key.
fn filter_built() -> Option<bool> {
    let key = LAST_FILTER_KEY.load(Ordering::Relaxed);
    if key == INVALID_KEY {
        return None;
    }
    let mut evicted = 0;
    // SAFETY: the engine's table lookup of a key it has drawn with.
    let info = unsafe { (api().shader_lookup)(key, &mut evicted) };
    // SAFETY: a non-null lookup result is a live table entry.
    Some(!info.is_null() && unsafe { (*info).program } != FAILED_PROGRAM)
}

/// Runs `blit`, the game's blit of a `frame_w` x `frame_h` frame, with each screen pixel
/// averaging the `span` frame pixels per axis it covers; plainly where the filter is
/// unavailable. `blit` must filter the frame smoothly.
pub fn blit(frame_w: f32, frame_h: f32, span: (f32, f32), blit: impl FnOnce()) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let State::Uninstalled = *state {
        *state = install();
    }
    let State::Installed { size, span: span_id, confirmed } = *state else {
        drop(state);
        return blit();
    };
    let a = api();
    // SAFETY: the engine's uniform setters with ids it handed out.
    unsafe {
        (a.set_shader_uniform_vec2)(size, &[frame_w, frame_h]);
        (a.set_shader_uniform_vec2)(span_id, &[span.0, span.1]);
    }
    FILTERING.store(true, Ordering::Relaxed);
    blit();
    FILTERING.store(false, Ordering::Relaxed);
    if let Some(built) = (!confirmed).then(filter_built).flatten() {
        log!("frame filter: {}", if built { "area average" } else { "failed to build, game's own" });
        *state = if built { State::Installed { size, span: span_id, confirmed: true } } else { State::Failed };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_shrunk_blit_gets_the_filter_key() {
        assert_eq!(filter_key(7, false), 7);
        assert_eq!(filter_key(7, true), 7 | FILTER_KEY);
        assert_eq!(filter_key(INVALID_KEY, true), INVALID_KEY);
    }
}
