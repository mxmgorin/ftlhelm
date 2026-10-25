//! The game's std::string: the copy-on-write layout, one pointer to NUL-terminated
//! characters, built and destroyed through the constructors in the binary.

use std::ffi::{c_char, c_void, CStr, CString};

use super::api::{api, Ptr};

/// A std::string of the game's own, released on drop.
pub struct GameString(Ptr);

impl GameString {
    /// A copy of `text` built by the game's constructor, without its NUL bytes.
    pub fn new(text: &str) -> GameString {
        let c = CString::new(text.replace('\0', "")).expect("NUL bytes were removed");
        let mut s = GameString::returned();
        let alloc = 0u8;
        // SAFETY: the game's std::string constructor, with an allocator it ignores.
        unsafe { (api().string_ctor)(s.as_mut_ptr(), c.as_ptr(), &alloc as *const u8 as *const c_void) };
        s
    }

    /// Room for a std::string a game function returns; that call constructs it.
    pub fn returned() -> GameString {
        GameString(std::ptr::null_mut())
    }

    pub fn as_ptr(&self) -> *const Ptr {
        &self.0
    }

    pub fn as_mut_ptr(&mut self) -> *mut Ptr {
        &mut self.0
    }

    /// The characters up to the terminator; empty while unconstructed.
    pub fn text(&self) -> String {
        // SAFETY: a constructed std::string's characters are NUL-terminated.
        unsafe { chars_text(self.0) }
    }
}

impl Drop for GameString {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // SAFETY: the string came from the game's constructor and is released once.
        unsafe { (api().string_dtor)(&mut self.0) };
    }
}

/// The text of a std::string's characters, up to the terminator; empty for null.
/// # Safety
/// A non-null `chars` must point at NUL-terminated characters.
pub unsafe fn chars_text(chars: Ptr) -> String {
    if chars.is_null() {
        return String::new();
    }
    CStr::from_ptr(chars as *const c_char).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{live_strings, with_game};

    #[test]
    fn a_string_is_released_once_and_an_unconstructed_one_never() {
        with_game(|_| {
            let s = GameString::new("Shields");
            assert_eq!((s.text(), live_strings()), ("Shields".to_string(), 1));
            drop(s);
            let unconstructed = GameString::returned();
            assert_eq!(unconstructed.text(), "");
            drop(unconstructed);
            assert_eq!(live_strings(), 0);
        });
    }
}
