//! The layer's state, and the one borrow of it a hook holds at a time.

use std::sync::{Mutex, TryLockError};

use super::util::FrameClock;
use crate::config::layout::Layout;
use crate::config::settings::{FileWatch, Settings};
use crate::debug::commands::Commands;
use crate::game::actions::Actions;
use crate::game::hud::Hud;
use crate::game::ipad_art::IpadArt;
use crate::game::zones::Zones;
use crate::input::bindings::Bindings;
use crate::input::cursor::{Cursor, FirstFocus};
use crate::input::pad::PadState;
use crate::input::touch::Touch;
use crate::ui::badge::{BadgeLine, BannerArt};
use crate::ui::draw::{Images, WHEEL_FONT};
use crate::ui::overlay::Notice;
use crate::ui::wheel::WheelUi;
use crate::ui::zoom::Zoom;

/// Everything the hooks share; each module adds its methods.
pub struct Layer {
    pub debug: bool,
    pub clock: FrameClock,
    pub cfg: Settings,
    pub files: FileWatch,
    pub layout: Layout,
    pub bind: Bindings,
    pub cursor: Cursor,
    pub focus: FirstFocus,
    pub zones: Zones,
    pub actions: Actions,
    pub pad: PadState,
    pub touch: Touch,
    pub wheel: WheelUi,
    pub zoom: Zoom,
    pub hud: Hud,
    pub ipad_art: IpadArt,
    pub images: Images,
    pub banner_art: BannerArt,
    pub badge: BadgeLine,
    /// The font of the wheel's text.
    pub plate_font: i32,
    pub help_shown: bool,
    pub notice: Option<Notice>,
    pub cmd: Commands,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            debug: false,
            clock: FrameClock::default(),
            cfg: Settings::default(),
            files: FileWatch::default(),
            layout: Layout::default(),
            bind: Bindings::default(),
            cursor: Cursor::default(),
            focus: FirstFocus::default(),
            zones: Zones::default(),
            actions: Actions::default(),
            pad: PadState::default(),
            touch: Touch::default(),
            wheel: WheelUi::default(),
            zoom: Zoom::default(),
            hud: Hud::default(),
            ipad_art: IpadArt::default(),
            images: Images::default(),
            banner_art: BannerArt::default(),
            badge: BadgeLine::default(),
            plate_font: WHEEL_FONT,
            help_shown: false,
            notice: None,
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
