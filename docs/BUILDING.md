# Building

Needs Rust for `x86_64-unknown-linux-gnu` and a C compiler for `ftlpad_probe`. The only
crate dependency is `libc`.

| Command | Result |
| --- | --- |
| `make` | `ftlpad.so` (through `cargo build --release`) and `ftlpad_probe` |
| `make OUT=path.so` | the library at another path |
| `make dist` | `dist/ftlpad-linux-x86_64.tar.gz`: `ftlpad.so`, `ftlpad.ini`, `ftlpad.sh`, `LICENSE`, `NOTICE` |
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

With `FTLPAD_CMD` naming a FIFO, ftlpad reads commands from it once a frame and logs the
results to standard error. The commands are the cases of `run_command` in
`src/debug/commands.rs`: `dump` logs the zones, `state` the state stack, `move X Y` moves
the cursor, among others.

    mkfifo /tmp/ftlpad.cmd
    FTLPAD_CMD=/tmp/ftlpad.cmd LD_PRELOAD=$PWD/ftlpad.so FTLPAD_LAYOUT=$PWD/config/ftlpad.ini \
        "/path/to/FTL Faster Than Light/FTL"
    echo dump > /tmp/ftlpad.cmd

## Virtual pad

`tools/vpad.py` makes a uinput pad the game picks up and plays steps on it, and writes to
the test channel:

    tools/vpad.py --fifo /tmp/ftlpad.cmd tap:a hold:x:1 stick:r:1,0:0.2 cmd:dump
    tools/vpad.py --script dpad

Steps and buttons are listed in `tools/vpad.py --help` and its header. With Steam running,
Steam takes the virtual pad; close Steam first.

## box64 check

    FTLPAD_DEBUG=1 BOX64_LOG=1 BOX64_LD_PRELOAD=/path/ftlpad.so box64 ./ftlpad_probe

`ftlpad:` lines in the output mean box64 loaded the library; otherwise box64's errors name
the reason. The PortMaster launcher runs the same check before the game.
