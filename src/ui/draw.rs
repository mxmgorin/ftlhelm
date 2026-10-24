//! Drawing over the game's renderer: text through its fonts on its own std::string, panels,
//! discs, game images by path, and the shared palette.

use std::collections::HashMap;
use std::ffi::{c_void, CString};

use crate::core::util::log;
use crate::game::api::{api, GlColor, Ptr};

const DISC_SCANLINE: f32 = 4.0;
const BEVEL: f32 = 2.0;
pub const OVERLAY_LINE: f32 = 1.0;
/// The game's outlined header font, as on its own buttons.
pub const WHEEL_FONT: i32 = 62;
/// The height of a WHEEL_FONT line.
pub const WHEEL_FONT_H: f32 = 18.0;

/// The game's palette: dark teal panels, pale green frames, white text.
pub const PLATE_COLOR: GlColor = GlColor::new(0.05, 0.11, 0.1, 0.9);
pub const PLATE_EDGE_COLOR: GlColor = GlColor::new(0.63, 0.9, 0.76, 0.95);
const PLATE_BEVEL_COLOR: GlColor = GlColor::new(0.0, 0.05, 0.05, 0.9);
pub const TEXT_COLOR: GlColor = GlColor::new(1.0, 1.0, 1.0, 1.0);
pub const TEXT_DISABLED_COLOR: GlColor = GlColor::new(0.42, 0.46, 0.45, 1.0);

/// A std::string built by the game's own constructor, released on drop.
struct GameString(Ptr);

impl GameString {
    fn new(text: &str) -> GameString {
        let c = CString::new(text.replace('\0', "")).expect("NUL bytes were removed");
        let mut s: Ptr = std::ptr::null_mut();
        let alloc = 0u8;
        // SAFETY: the game's std::string constructor, with an allocator it ignores.
        unsafe { (api().string_ctor)(&mut s, c.as_ptr(), &alloc as *const u8 as *const c_void) };
        GameString(s)
    }

    fn as_ptr(&self) -> *const Ptr {
        &self.0
    }
}

impl Drop for GameString {
    fn drop(&mut self) {
        // SAFETY: the string came from the game's constructor and is released once.
        unsafe { (api().string_dtor)(&mut self.0) };
    }
}

pub fn draw_text_left(font: i32, x: f32, y: f32, text: &str) {
    let s = GameString::new(text);
    // SAFETY: the game's font code on a std::string it built.
    unsafe { (api().print_left)(font, x, y, s.as_ptr()) };
}

/// Prints text centred at x with its top at y.
pub fn draw_text_center(font: i32, x: f32, y: f32, text: &str) {
    let s = GameString::new(text);
    // SAFETY: as above.
    unsafe { (api().print_center)(font, x, y, s.as_ptr()) };
}

pub fn text_width(font: i32, text: &str) -> f32 {
    let s = GameString::new(text);
    // SAFETY: as above.
    unsafe { (api().measure_width)(font, s.as_ptr()) as f32 }
}

pub fn set_color(color: GlColor) {
    // SAFETY: the renderer's colour state.
    unsafe { (api().set_color)(color) }
}

pub fn draw_rect(x: f32, y: f32, w: f32, h: f32, color: GlColor) {
    // SAFETY: the renderer's rectangle fill.
    unsafe { (api().draw_rect)(x, y, w, h, color) }
}

pub fn draw_rect_outline(x: f32, y: f32, w: f32, h: f32, color: GlColor) {
    // SAFETY: the renderer's rectangle outline.
    unsafe { (api().draw_rect_outline)(x as i32, y as i32, w as i32, h as i32, color, OVERLAY_LINE) }
}

/// A panel in the game's style: a dark bevel around a framed fill.
pub fn draw_panel(x: f32, y: f32, w: f32, h: f32, fill: GlColor, edge: GlColor) {
    draw_rect(x - BEVEL, y - BEVEL, w + 2.0 * BEVEL, h + 2.0 * BEVEL, PLATE_BEVEL_COLOR);
    draw_rect(x, y, w, h, fill);
    draw_rect_outline(x, y, w, h, edge);
}

/// A filled disc out of scanlines: the game draws no curves.
pub fn draw_disc(cx: f32, cy: f32, r: f32, color: GlColor) {
    let mut y = -r;
    while y < r {
        let t = (y + DISC_SCANLINE / 2.0) / r;
        let half = r * (1.0 - t * t).max(0.0).sqrt();
        draw_rect(cx - half, cy + y, 2.0 * half, DISC_SCANLINE, color);
        y += DISC_SCANLINE;
    }
}

/// Game images looked up by path under img/, misses included.
#[derive(Default)]
pub struct Images {
    cache: HashMap<String, usize>,
}

impl Images {
    /// The texture of a game image, or None when the game has none.
    pub fn get(&mut self, path: &str) -> Option<Ptr> {
        let texture = *self.cache.entry(path.to_string()).or_insert_with(|| {
            let s = GameString::new(path);
            let a = api();
            // SAFETY: ResourceControl::GetImageId on the global resources and a std::string.
            let texture = unsafe { (a.image_id)(a.resources as Ptr, s.as_ptr()) } as usize;
            if texture == 0 {
                log!("image missing: {path}");
            }
            texture
        });
        (texture != 0).then_some(texture as Ptr)
    }

    /// Caches `texture` for `path` as a lookup would; 0 caches a miss.
    #[cfg(test)]
    pub fn insert(&mut self, path: &str, texture: usize) {
        self.cache.insert(path.to_string(), texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{live_strings, with_game};

    #[test]
    fn text_reaches_the_game_as_its_own_string_released_after() {
        with_game(|_| {
            assert_eq!(text_width(WHEEL_FONT, "Shields"), "Shields".len() as f32);
            assert_eq!(text_width(WHEEL_FONT, "a\0b"), "ab".len() as f32);
            assert_eq!(live_strings(), 0);
        });
    }
}
