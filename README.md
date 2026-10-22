# ftlpad

Gamepad layer for the Linux build of FTL: Faster Than Light 1.6.13 (`FTL.amd64`). An
x86_64 shared library preloaded into the game; it reads the pad through the game's own
joystick layer and drives the cursor, clicks and hotkeys. The game files stay unchanged.

## Files

| Path | Contents |
|---|---|
| `src/lib.rs` | the overview of the design; the load-time entry |
| `src/core/` | symbol lookup, hooks, the layer's state, the per-frame entry |
| `src/config/` | `ftlpad.ini` and layout parsing |
| `src/game/` | the game's API and object layout, state readers, D-pad zones, actions |
| `src/input/` | pad reading, bindings, cursor and focus |
| `src/ui/` | wheels, help, mode badge |
| `src/debug/` | the FIFO test channel |
| `config/ftl_scheme.ini` | the layout: buttons, taps and holds, wheels |
| `config/ftlpad.ini` | settings, each documented in place |
| `tools/vpad.py` | uinput virtual pad for desktop tests (python-evdev) |

## Build

    make                 # ftlpad.so, through cargo build --release
    make OUT=path.so
    make test            # unit tests: layout parsing, sticks, wheel geometry
    make lint            # rustfmt check, clippy with warnings as errors

Needs Rust for x86_64-unknown-linux-gnu; the only dependency is the libc crate. The
library needs glibc 2.35 or later. `make` fails when it needs any shared library besides
libc, libm and ld-linux.

## Run

Environment of the game process:

| Variable | Value |
|---|---|
| `LD_PRELOAD` | path to `ftlpad.so` (under box64: `BOX64_LD_PRELOAD`) |
| `FTLPAD_LAYOUT` | path to `ftl_scheme.ini`; without it a minimal built-in layout |
| `FTLPAD_CONFIG` | path to `ftlpad.ini` |
| `FTLPAD_EXE` | path to `FTL.amd64`; default `/proc/self/exe` |
| `FTLPAD_CMD` | optional FIFO for test commands |
| `FTLPAD_DEBUG` | set: log presses, picks and D-pad steps to stderr |

Steam launch options (Linux, Steam Deck):

    LD_PRELOAD=/path/ftlpad.so FTLPAD_LAYOUT=/path/config/ftl_scheme.ini FTLPAD_CONFIG=/path/config/ftlpad.ini %command%

The library stays inactive in processes that are not FTL. Both ini files are re-read while
the game runs. Xbox-style pads without an SDL mapping take `swap_ab = 0` in `ftlpad.ini`.

## Layout format

`ftl_scheme.ini` is gptokeyb2's ini format with wheels and built-in actions on top.

| Section | Contents |
|---|---|
| `[controls]` | the base state |
| `[controls:NAME]` | a state pushed while a `hold_state NAME` button is held |
| `[wheel:NAME]` | a wheel opened by `wheel NAME` |

Inputs: `a`, `b`, `x`, `y` (by label), `l1`, `r1`, `l2`, `r2`, `l3`, `r3`, `start`,
`back`, `guide`, `up`, `down`, `left`, `right`, `left_analog_up` and the other stick
directions. `left_analog`, `right_analog` and `dpad` take `mouse_movement` (cursor) or
`flick` (jump between zones). `overlay = parent` lets a state fall through to the one
below.

| Action | Effect |
|---|---|
| a key name | `a`..`z`, `0`..`9`, `f1`..`f15`, `space`, `enter`, `esc`, `tab`, `up`, `/`, ...; `add_shift` after it adds Shift |
| `mouse_left`, `mouse_right` | click at the cursor |
| `hold_state NAME` | push a state while held |
| `wheel NAME` | open a wheel |
| a built-in | see below |
| `HOLD \| TAP` | the hold action past `tap_hold` seconds, the tap action on a shorter press |

| Built-in | Effect |
|---|---|
| `context_click` | click; with crew selected, a room orders them there; in a wheel, act |
| `cancel` | close the wheel, cancel the aim, clear the crew, close the menu or a screen |
| `arm_next` | arm the next powered weapon |
| `aim_crew` | select the next crew member; on a ship screen, the next tab |
| `prev_tab`, `next_tab` | ship and store screen tabs |
| `repeat_ability` | the last ability used from a wheel |
| `precision` | doors become the zones while held |
| `room_doors` | toggle the doors of the room under the cursor |
| `center_swap` | the cursor to the centre of the farther ship |
| `pause`, `fast_forward` | pause; run fast while held |
| `dpad_mode` | the D-pad and sticks switch between zones and a free cursor |
| `help` | show the controls table; any button closes it |

Wheel sections: `kind = action|power|crew|systems`, then plates by direction, `up`,
`up_right`, `right`, `down_right`, `down`, `down_left`, `left`, `up_left`:

    up = z "Open doors" systemUI/button_door_bottom_off.png@10,33,20,20

An action, a quoted label, and an optional icon: a game image path under `img/`, with
`@X,Y,W,H` for a region in pixels. `crew` and `systems` wheels fill their plates from the
game; `power` wheels show power pips and take power with D-pad up and down.

## Test channel

With `FTLPAD_CMD` set to a FIFO: `move X Y`, `click`, `rclick`, `key N`, `dump`, `state`,
`wheel NAME`, `pick N`, `buttons`, `doors`, `roomdoors`, `badge`, `font N`. Output goes
to stderr. `tools/vpad.py --fifo PATH` drives the pad and the channel together.

## Used by

The FTL PortMaster port in handheld-porting (`ports/portmaster/ftl`) runs this library
under box64 and ships both ini files.
