//! ftlpad: a gamepad layer for the Linux build of FTL, preloaded into the game process
//! (natively, or under box64 on ARM handhelds).
//!
//! The game's SIL engine scans /dev/input for joysticks and polls them every frame, but
//! nothing in the game reads the result. The library resolves the engine's joystick API
//! and the game's entry points from the executable's symbol table, hooks the game loop,
//! and drives the game from the pad: a virtual cursor fed to the mouse handlers, clicks,
//! key presses, and a Shift state the hotkey code reads. Hook sites are diverted through
//! a trampoline page mapped within a rel32 jump of the executable, since the library
//! itself loads far away. The game binary on disk is never changed.
//!
//! Modules: `core` resolves symbols, places hooks and runs the per-frame entry; `config`
//! reads ftlpad.ini and the layout; `game` reads the game's objects, builds the zones and
//! performs actions on the game; `input` reads the pad, resolves bindings and moves the
//! cursor; `ui` draws wheels, help and the mode badge; `debug` serves the FIFO test
//! channel.
//!
//! State: almost everything lives in one `Layer`, borrowed by one hook at a time. What the
//! game can reach while a hook holds it (the buttons its hit-test reports, the Shift count,
//! the CApp pointer) lives apart, so no two live borrows of the same state overlap. The
//! game calls every hook from its one main thread.
//!
//! The layout comes from a gptokeyb2-style ini: [controls] is the base state, each
//! [controls:NAME] a state a hold_state button pushes while held, overlay = parent letting
//! lookups fall through to the state below. A press records what it did so its release
//! undoes exactly that, whatever state is active by then. A binding may carry two actions,
//! "HOLD | TAP"; a tap half that would do nothing on the current screen makes the press a
//! hold at once.
//!
//! The cursor lives in the game's logical screen; the mouse handlers take window pixels,
//! so positions pass through the inverse of the game's own transform. Key codes are the
//! SDL1 keysyms the game's key handlers expect. Modes are read from the game, never
//! tracked: aiming from MouseControl, crew selection from CrewControl, screens from their
//! open flags.
//!
//! Zones for the D-pad come from the screen on top: an event's choices, the star map's
//! beacons or sector boxes, ship rooms and doors, equipment slots, and the buttons the
//! game hit-tests on mouse moves, collected by a hook on GenericButton::MouseMove and
//! refreshed with a synthetic move. Boxes kept relative to a parent (armament bars, the
//! star map) get the parent's position added.
//!
//! Wheels open around the cursor on the ship view; plates are icons drawn with the game's
//! own textures, fonts and renderer. Labels print through the game's font code, which
//! takes its bundled copy-on-write std::string, built and destroyed through the
//! constructors in the binary. Everything logs to stderr under one prefix.

#![cfg_attr(test, allow(dead_code))]

mod core;
mod game;
mod ui;
