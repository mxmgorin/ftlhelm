# Building

Needs Rust for `x86_64-unknown-linux-gnu` and a C compiler for `ftlhelm_probe`. The only
crate dependency is `libc`.

| Command | Result |
| --- | --- |
| `make` | `ftlhelm.so` (through `cargo build --release`) and `ftlhelm_probe` |
| `make OUT=path.so` | the library at another path |
| `make dist` | `dist/ftlhelm-linux-x86_64.tar.gz`: `ftlhelm.so`, `ftlhelm.ini`, `ftlhelm.sh`, `LICENSE`, `NOTICE` |
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
e.g. `dump` (zones), `state` (state stack), `move X Y` (cursor).

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

## box64 check

    FTLHELM_DEBUG=1 BOX64_LOG=1 BOX64_LD_PRELOAD=/path/ftlhelm.so box64 ./ftlhelm_probe

`ftlhelm:` lines in the output: the library loaded; otherwise box64's errors name the
reason. The PortMaster launcher runs the same check before the game.
