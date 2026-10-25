//! Larger event and tooltip text where the game's frame is shrunk to fit the screen.
//!
//! Event boxes lay text out for a fixed frame, so a box is drawn whole under a transform
//! set as it draws its frame: scaled about the frame's centre and kept on screen. The box's
//! hit-tests and the zones built from its choices pass through the same transform.
//!
//! Tooltips size to their text: while one is measured or drawn, body font lookups return a
//! scaled copy of the game's entry; the font table is never written. A tooltip too tall at
//! that scale is measured again smaller until it fits, and the draw keeps the last measured
//! scale. Other text keeps the game's sizes: fixed HUD frames would clip it.

use std::cell::UnsafeCell;
use std::ffi::c_int;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::core::util::{clampf, log};
use crate::game::api::{api, FontData, Point, Rect};
use crate::game::mem::Obj;
use crate::game::offsets::{
    layout, BODY_FONTS, CHOICE_FRAME, FRAME_RECT, GAME_H, GAME_W, TOOLTIP_BOTTOM, TOOLTIP_PAD_H, TOOLTIP_TITLE_PAD_H,
};

/// The `text_scale` value that leaves the scale to the screen.
pub const TEXT_SCALE_AUTO: f32 = 0.0;
pub const TEXT_SCALE_MAX: f32 = 2.0;
/// Auto scale: the share of the game's text size kept on a shrunk screen.
const AUTO_TEXT_SIZE: f32 = 0.875;
const UNSCALED: f32 = 1.0;
/// The tallest measured tooltip whose box, titled or not, still starts on screen.
const TOOLTIP_MAX_H: i32 = TOOLTIP_BOTTOM - TOOLTIP_PAD_H - TOOLTIP_TITLE_PAD_H;
/// How much a tooltip that does not fit shrinks per measure.
const TOOLTIP_SCALE_STEP: f32 = 0.05;
const SLOTS: usize = (*BODY_FONTS.end() - *BODY_FONTS.start() + 1) as usize;

static SCALE: AtomicU32 = AtomicU32::new(UNSCALED.to_bits());
static TOOLTIP_DEPTH: AtomicU32 = AtomicU32::new(0);
/// The scale of the tooltip being measured or drawn.
static TOOLTIP_SCALE: AtomicU32 = AtomicU32::new(UNSCALED.to_bits());
/// The event box inside its OnRender, or 0.
static RENDERING_BOX: AtomicUsize = AtomicUsize::new(0);
/// The transform each event box last drew its frame with.
static ZOOMS: Mutex<Vec<(usize, Option<BoxZoom>)>> = Mutex::new(Vec::new());

/// The scaled copies handed to the game, one per body text font id.
struct Slots(UnsafeCell<[FontData; SLOTS]>);

// SAFETY: the game looks fonts up from its one main thread only.
unsafe impl Sync for Slots {}

static SCALED: Slots = Slots(UnsafeCell::new([FontData::EMPTY; SLOTS]));

fn scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

/// Sets the scale from the `text_scale` setting and the game's current frame; logs a change.
pub fn update(setting: f32, app: Option<Obj>) {
    let scale = if setting == TEXT_SCALE_AUTO {
        app.map_or(UNSCALED, |a| {
            let l = layout();
            auto_scale(a.f32(l.capp_mouse_modifier_x).max(a.f32(l.capp_mouse_modifier_y)))
        })
    } else {
        clampf(setting, UNSCALED, TEXT_SCALE_MAX)
    };
    if SCALE.swap(scale.to_bits(), Ordering::Relaxed) != scale.to_bits() {
        log!("text scale {scale:.2}");
    }
}

/// The scale for a frame shrunk by `shrink` game pixels per screen pixel; 1 when not shrunk.
fn auto_scale(shrink: f32) -> f32 {
    clampf(AUTO_TEXT_SIZE * shrink, UNSCALED, TEXT_SCALE_MAX)
}

/// Keeps tooltip fonts scaled for the game code run while it lives; scopes nest, and the
/// outermost starts at the full scale.
pub struct TooltipScope(());

impl TooltipScope {
    pub fn enter() -> TooltipScope {
        if TOOLTIP_DEPTH.fetch_add(1, Ordering::Relaxed) == 0 {
            TOOLTIP_SCALE.store(SCALE.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        TooltipScope(())
    }
}

/// Measures a tooltip through `measure` at the largest scale, up to the full one, that keeps
/// it on screen; returns its size. That scale holds for the rest of the enclosing scope.
pub fn measure_tooltip(measure: impl Fn() -> Point) -> Point {
    let _scope = TooltipScope::enter();
    let mut k = scale();
    loop {
        TOOLTIP_SCALE.store(k.to_bits(), Ordering::Relaxed);
        let size = measure();
        if size.y <= TOOLTIP_MAX_H || k <= UNSCALED {
            return size;
        }
        k = (k - TOOLTIP_SCALE_STEP).max(UNSCALED);
    }
}

impl Drop for TooltipScope {
    fn drop(&mut self) {
        TOOLTIP_DEPTH.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The font a lookup of `id` hands the game: `found`, or within a tooltip scope a scaled
/// copy of a body text font.
pub fn font_for(id: c_int, found: *const FontData) -> *const FontData {
    let scale = f32::from_bits(TOOLTIP_SCALE.load(Ordering::Relaxed));
    if TOOLTIP_DEPTH.load(Ordering::Relaxed) == 0 || scale == UNSCALED || found.is_null() || !BODY_FONTS.contains(&id) {
        return found;
    }
    let slot = (id - BODY_FONTS.start()) as usize;
    // SAFETY: `found` is the game's live entry for `id`; the slots are only touched from the
    // game's main thread, and the game reads a returned slot before its next lookup of `id`.
    unsafe {
        let copy = &mut (*SCALED.0.get())[slot];
        *copy = found.read().scaled(scale);
        copy
    }
}

/// How an event box is drawn: a point of its layout lands at `p * k + (tx, ty)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxZoom {
    pub k: f32,
    pub tx: f32,
    pub ty: f32,
}

impl BoxZoom {
    /// The transform that scales `frame` by up to `scale` about its centre and keeps it on
    /// the game's screen; None when that leaves it unscaled.
    fn fit(frame: Rect, scale: f32) -> Option<BoxZoom> {
        if frame.w <= 0 || frame.h <= 0 {
            return None;
        }
        let k = scale.min(GAME_W as f32 / frame.w as f32).min(GAME_H as f32 / frame.h as f32);
        if k <= UNSCALED {
            return None;
        }
        let (cx, cy) = frame.center();
        let half_w = frame.w as f32 * k / 2.0;
        let half_h = frame.h as f32 * k / 2.0;
        let x = clampf(cx, half_w, GAME_W as f32 - half_w);
        let y = clampf(cy, half_h, GAME_H as f32 - half_h);
        Some(BoxZoom { k, tx: x - cx * k, ty: y - cy * k })
    }

    pub fn to_layout(self, x: i32, y: i32) -> (i32, i32) {
        (((x as f32 - self.tx) / self.k).round() as i32, ((y as f32 - self.ty) / self.k).round() as i32)
    }

    pub fn to_screen(self, r: Rect) -> Rect {
        Rect {
            x: (r.x as f32 * self.k + self.tx).round() as i32,
            y: (r.y as f32 * self.k + self.ty).round() as i32,
            w: (r.w as f32 * self.k).round() as i32,
            h: (r.h as f32 * self.k).round() as i32,
        }
    }
}

/// Marks an event box as rendering while it lives; boxes do not nest.
pub struct BoxRender(());

impl BoxRender {
    /// Starts rendering `choice` under a fresh matrix, which its frame's draw may scale.
    pub fn enter(choice: usize) -> BoxRender {
        // SAFETY: the game's matrix stack, popped when the guard drops.
        unsafe { (api().push_matrix)() };
        RENDERING_BOX.store(choice, Ordering::Relaxed);
        BoxRender(())
    }
}

impl Drop for BoxRender {
    fn drop(&mut self) {
        RENDERING_BOX.store(0, Ordering::Relaxed);
        // SAFETY: pops the matrix `enter` pushed.
        unsafe { (api().pop_matrix)() };
    }
}

/// Runs as a window frame draws at (`x`, `y`): when it is the rendering event box's own,
/// records the box's transform and applies it to everything drawn after.
pub fn frame_drawn(frame: usize, x: i32, y: i32) {
    // SAFETY: RENDERING_BOX only holds a ChoiceBox inside its own OnRender.
    let Some(choice) = (unsafe { Obj::from_addr(RENDERING_BOX.load(Ordering::Relaxed)) }) else {
        return;
    };
    let Some(own) = choice.obj(CHOICE_FRAME).filter(|f| f.addr() == frame) else {
        return;
    };
    let r: Rect = own.get(FRAME_RECT);
    let zoom = BoxZoom::fit(Rect { x: x + r.x, y: y + r.y, ..r }, scale());
    record(choice.addr(), zoom);
    if let Some(z) = zoom {
        // SAFETY: the game's matrix calls, inside the matrix BoxRender pushed.
        unsafe {
            (api().translate)(z.tx, z.ty, 0.0);
            (api().scale)(z.k, z.k, UNSCALED);
        }
    }
}

fn record(choice: usize, zoom: Option<BoxZoom>) {
    let mut zooms = ZOOMS.lock().unwrap_or_else(|e| e.into_inner());
    match zooms.iter_mut().find(|(c, _)| *c == choice) {
        Some(entry) => entry.1 = zoom,
        None => zooms.push((choice, zoom)),
    }
}

/// The transform the event box at `choice` last drew with; None when unscaled.
pub fn box_zoom(choice: usize) -> Option<BoxZoom> {
    let zooms = ZOOMS.lock().unwrap_or_else(|e| e.into_inner());
    zooms.iter().find(|(c, _)| *c == choice).and_then(|(_, z)| *z)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises the tests that set the scale, which is process-wide.
    static SCALE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn auto_scale_only_enlarges() {
        assert_eq!(auto_scale(1.0), UNSCALED);
        assert_eq!(auto_scale(2.0), 1.75);
        assert_eq!(auto_scale(4.0), TEXT_SCALE_MAX);
    }

    #[test]
    fn lookups_outside_a_tooltip_pass_through() {
        let _lock = SCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let font = FontData { h: 12.0, font: 22, size: 19.0, baseline: 12.0, line_height: 17.0 };
        SCALE.store(1.5f32.to_bits(), Ordering::Relaxed);
        assert_eq!(font_for(10, &font), &font as *const _);
        let scope = TooltipScope::enter();
        let scaled = font_for(10, &font);
        // SAFETY: a slot the call just filled.
        assert_eq!(unsafe { scaled.read() }, font.scaled(1.5));
        assert_eq!(font_for(*BODY_FONTS.end() + 1, &font), &font as *const _);
        drop(scope);
        SCALE.store(UNSCALED.to_bits(), Ordering::Relaxed);
    }

    #[test]
    fn a_tall_tooltip_shrinks_until_it_fits() {
        let _lock = SCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        SCALE.store(1.75f32.to_bits(), Ordering::Relaxed);
        // A stand-in whose height grows with the scale squared, as wrapped text does.
        let height = |base: f32| {
            move || {
                let k = f32::from_bits(TOOLTIP_SCALE.load(Ordering::Relaxed));
                Point { x: 0, y: (base * k * k) as i32 }
            }
        };
        let scope = TooltipScope::enter();
        assert!(measure_tooltip(height(300.0)).y <= TOOLTIP_MAX_H);
        let settled = f32::from_bits(TOOLTIP_SCALE.load(Ordering::Relaxed));
        assert!(settled < 1.75 && settled > UNSCALED);
        assert_eq!(measure_tooltip(height(300.0)).y, (300.0 * settled * settled) as i32);
        drop(scope);
        let _next = TooltipScope::enter();
        assert_eq!(measure_tooltip(height(100.0)).y, (100.0 * 1.75 * 1.75) as i32);
        assert_eq!(measure_tooltip(height(1000.0)).y, 1000);
        drop(_next);
        SCALE.store(UNSCALED.to_bits(), Ordering::Relaxed);
    }

    #[test]
    fn a_zoomed_box_stays_on_screen_and_maps_back() {
        let frame = Rect { x: 335, y: 147, w: 602, h: 377 };
        let z = BoxZoom::fit(frame, 1.5).expect("room to scale");
        let s = z.to_screen(frame);
        assert!(s.x >= 0 && s.y >= 0 && s.x + s.w <= GAME_W && s.y + s.h <= GAME_H);
        assert_eq!(z.to_layout(s.x, s.y), (frame.x, frame.y));
        let tall = BoxZoom::fit(Rect { h: 600, ..frame }, 1.5).expect("room to scale");
        assert_eq!(tall.k, GAME_H as f32 / 600.0);
        assert_eq!(BoxZoom::fit(Rect { h: GAME_H, ..frame }, 1.5), None);
    }
}
