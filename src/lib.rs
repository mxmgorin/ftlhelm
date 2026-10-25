//! ftlpad: a gamepad layer for the Linux build of FTL, preloaded into the game process
//! natively or under box64.
//!
//! The game's SIL engine polls joysticks every frame, but nothing in the game reads them.
//! The library resolves the joystick API and the game's entry points from the executable's
//! symbol table, hooks the game loop, and drives the game through its own mouse and key
//! handlers. The game binary on disk is never changed.
//!
//! Modules: `core` resolves symbols, places hooks and runs the per-frame entry; `config`
//! reads the layout and its settings; `game` reads and drives the game's objects; `input`
//! reads the pad, resolves bindings and moves the cursor; `ui` draws over the game;
//! `debug` serves the FIFO test channel.
//!
//! State lives in one `Layer`, borrowed by one hook at a time. What the game reaches while
//! a hook holds it (hit-tested buttons, the Shift count, the CApp pointer) lives apart, so
//! no two borrows overlap. Every hook runs on the game's main thread.
//!
//! Positions are in the game's logical screen and pass through its transform into window
//! pixels for the mouse handlers. Key codes are SDL1 keysyms. Modes (aiming, crew
//! selection, open screens) are read from the game each frame, never tracked.

#![cfg_attr(test, allow(dead_code))]

mod config;
mod core;
mod debug;
mod game;
mod input;
mod ui;

/// Runs at load, before the game's main.
#[cfg(not(test))]
#[used]
#[link_section = ".init_array"]
static INIT: extern "C" fn() = core::main::init;
