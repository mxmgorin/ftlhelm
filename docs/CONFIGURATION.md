# Configuration

Settings live in the `[config]` section of `ftlpad.ini`, one `key = value` per line, `#`
starting a comment. Each key is documented in place; switches take `0` (off) or `1` (on).
A missing key takes its default; a value outside its range is clamped. The file is re-read
within a second of saving it, while the game runs.

Bindings and wheels are in [Controls](CONTROLS.md#custom-layouts).

## Environment variables

| Variable | Value |
| --- | --- |
| `LD_PRELOAD` | path to `ftlpad.so`; under box64, `BOX64_LD_PRELOAD` |
| `FTLPAD_LAYOUT` | path to `ftlpad.ini`; without it, a minimal built-in layout with default settings |
| `FTLPAD_EXE` | path to `FTL.amd64`; default `/proc/self/exe` |
| `FTLPAD_CMD` | path to a FIFO for test commands, see [Building](BUILDING.md#test-channel) |
| `FTLPAD_DEBUG` | set: log every press, wheel pick and D-pad step |
| `SDL_GAMECONTROLLERCONFIG` | an SDL mapping that names the pad's buttons |
