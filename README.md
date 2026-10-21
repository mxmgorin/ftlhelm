# ftlpad

Gamepad layer for the Linux build of FTL: Faster Than Light 1.6.13 (`FTL.amd64`). An
x86_64 shared library preloaded into the game; it reads the pad through the game's own
joystick layer and drives the cursor, clicks and hotkeys. The game files stay unchanged.

## Files

| File | Contents |
|---|---|
| `ftlpad.c` | the library |
| `Makefile` | builds `ftlpad.so`; `make OUT=path` builds elsewhere |
| `config/ftl_scheme.ini` | the layout: buttons, taps and holds, wheels |
| `config/ftlpad.ini` | settings: cursor, rumble, label swaps, speed level |
| `tools/vpad.py` | uinput virtual pad for desktop tests (python-evdev) |

## Build

    make

Needs gcc for x86_64 and libm.

## Run

Environment of the game process:

| Variable | Value |
|---|---|
| `LD_PRELOAD` | path to `ftlpad.so` (under box64: `BOX64_LD_PRELOAD`) |
| `FTLPAD_LAYOUT` | path to `ftl_scheme.ini`; without it a minimal built-in layout |
| `FTLPAD_CONFIG` | path to `ftlpad.ini` |
| `FTLPAD_EXE` | path to `FTL.amd64`; default `/proc/self/exe` |
| `FTLPAD_CMD` | optional FIFO for test commands |
| `FTLPAD_DEBUG` | set: log presses and picks to stderr |

Steam launch options (Linux, Steam Deck):

    LD_PRELOAD=/path/ftlpad.so FTLPAD_LAYOUT=/path/ftl_scheme.ini FTLPAD_CONFIG=/path/ftlpad.ini %command%

The library stays inactive in processes that are not FTL. Both ini files are re-read while
the game runs. Xbox-style pads take `nintendo_labels = 0` in `ftlpad.ini`.

## Test channel

With `FTLPAD_CMD` set to a FIFO: `move X Y`, `click`, `rclick`, `key N`, `dump`, `state`,
`wheel NAME`, `pick N`, `buttons`, `doors`, `roomdoors`, `badge`, `font N`.

## Used by

The FTL PortMaster port in handheld-porting (`ports/portmaster/ftl`) runs this library
under box64 and ships both ini files.
