# Building

Needs Rust for `x86_64-unknown-linux-gnu` and a C compiler for `ftlhelm_probe`. The only
crate dependency is `libc`.

| Command | Result |
| --- | --- |
| `make` | `ftlhelm.so` (through `cargo build --release`) and `ftlhelm_probe` |
| `make OUT=path.so` | the library at another path |
| `make dist` | `dist/ftlhelm-linux-x86_64.tar.gz`: `lib/ftlhelm.so`, `ftlhelm.ini`, `ftlhelm.sh`, `LICENSE`, `NOTICE`, `TRADEMARKS.md` |
| `make test` | unit tests |
| `make lint` | rustfmt check, clippy with warnings as errors |
| `make clean` | removes the build output |

`make` fails when the library needs any shared library besides libc, libm and ld-linux.

## CI

| Workflow | Runs on | Does |
| --- | --- | --- |
| Check | a push to `main`, a pull request | `make lint`, `make test`, `make` |
| Release | a published release | `make dist`; attaches the archive, its SHA-256 and a build provenance attestation to the release |

A release tag is the `Cargo.toml` version, with or without a leading `v`.

## Test channel

With `FTLHELM_CMD` naming a FIFO, ftlhelm runs the lines written to it once a frame and logs
to standard error. Commands are the cases of `run_command` in `src/debug/commands.rs`,
e.g. `dump` (zones), `state` (state stack), `move X Y` (cursor), `focus DX DY` (a D-pad step),
`touch down|move|up|cancel X Y [ID]` (a touch event at window pixels, as the engine reports
one).

    mkfifo /tmp/ftlhelm.cmd
    FTLHELM_CMD=/tmp/ftlhelm.cmd LD_PRELOAD=$PWD/ftlhelm.so FTLHELM_LAYOUT=$PWD/config/ftlhelm.ini \
        "/path/to/FTL Faster Than Light/FTL"
    echo dump > /tmp/ftlhelm.cmd

## Virtual pad

`tools/vpad.py` creates a uinput pad, plays steps on it and writes commands to the test
channel:

    tools/vpad.py --fifo /tmp/ftlhelm.cmd tap:a hold:x:1 stick:r:1,0:0.2 cmd:dump
    tools/vpad.py --script dpad

Steps and buttons: `tools/vpad.py --help` and the script's header. A running Steam grabs
the virtual pad; close it first.

## Crash reports

`tools/crash.py` prints the `crash:` and `panic at` lines of a log, each `rip` resolved by
`addr2line`: a `game` address against the game's executable, an `ftlhelm+OFFSET` against the
`ftlhelm.so` of that run.

    tools/crash.py ftlhelm.log --game "/path/to/FTL Faster Than Light/data/FTL.amd64" --lib ftlhelm.so

## box64 check

    FTLHELM_DEBUG=1 BOX64_LOG=1 BOX64_LD_PRELOAD=/path/ftlhelm.so box64 ./ftlhelm_probe

`ftlhelm:` lines in the output: the library loaded; otherwise box64's errors name the
reason.
