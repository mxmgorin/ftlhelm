//! The layer's state, and the one borrow of it a hook holds at a time.

use std::sync::{Mutex, TryLockError};

use crate::config::layout::Layout;
use crate::config::settings::{FileWatch, Settings};
use crate::debug::commands::Commands;
use crate::game::actions::Actions;
use crate::game::zones::Zones;
use crate::input::bindings::Bindings;
use crate::input::cursor::{Cursor, FirstFocus};
use crate::input::pad::PadState;
use crate::ui::draw::{Images, WHEEL_FONT};
use crate::ui::wheel::WheelUi;

/// Everything the hooks share; each module adds its methods.
pub struct Layer {
    pub debug: bool,
    pub cfg: Settings,
    pub files: FileWatch,
    pub layout: Layout,
    pub bind: Bindings,
    pub cursor: Cursor,
    pub focus: FirstFocus,
    pub zones: Zones,
    pub actions: Actions,
    pub pad: PadState,
    pub wheel: WheelUi,
    pub images: Images,
    /// The font of the wheel's text.
    pub plate_font: i32,
    pub help_shown: bool,
    pub cmd: Commands,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            debug: false,
            cfg: Settings::default(),
            files: FileWatch::default(),
            layout: Layout::default(),
            bind: Bindings::default(),
            cursor: Cursor::default(),
            focus: FirstFocus::default(),
            zones: Zones::default(),
            actions: Actions::default(),
            pad: PadState::default(),
            wheel: WheelUi::default(),
            images: Images::default(),
            plate_font: WHEEL_FONT,
            help_shown: false,
            cmd: Commands::default(),
        }
    }
}

// SAFETY: the raw pointers a Layer holds point into the game, and the game calls every hook
// from its one main thread, so the layer never crosses threads.
unsafe impl Send for Layer {}

static LAYER: Mutex<Option<Layer>> = Mutex::new(None);

/// Runs `f` on the layer; None when another hook up the stack holds it, so a call the game
/// makes back into a hook skips the layer.
pub fn with_layer<R>(f: impl FnOnce(&mut Layer) -> R) -> Option<R> {
    let mut guard = match LAYER.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
        Err(TryLockError::WouldBlock) => return None,
    };
    Some(f(guard.get_or_insert_with(Layer::default)))
}
