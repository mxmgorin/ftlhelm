# Linux

ftlpad runs inside the native Linux build of FTL (`FTL.amd64`, x86_64). The library needs
glibc 2.35 or later.

## Files

`ftlpad-linux-x86_64.tar.gz`, from the latest release or `make dist`, holds:

| File | Contents |
| --- | --- |
| `ftlpad.so` | the library |
| `ftlpad.ini` | the layout and its settings |
| `ftlpad.sh` | runs a command with `ftlpad.so` preloaded and `ftlpad.ini` as the layout |
| `LICENSE` | the GNU General Public License v3.0 |
| `NOTICE` | copyright, additional license terms, third-party notices |

The three files stay together; `ftlpad.sh` finds the other two next to itself.

## Steam

1. Unpack the archive anywhere, e.g. `~/ftlpad/`.
2. FTL > **Properties > General > Launch Options**:

       ~/ftlpad/ftlpad-linux-x86_64/ftlpad.sh %command%

3. FTL > **Properties > Controller**: Steam Input **Disabled**.

`ftlpad.sh` keeps the preloads Steam sets, the Steam overlay among them.

## Without Steam

    ~/ftlpad/ftlpad-linux-x86_64/ftlpad.sh "/path/to/FTL Faster Than Light/FTL"

`FTL` is the game's own launch script; it starts `data/FTL.amd64`.

## By hand

The environment `ftlpad.sh` sets, for another launcher:

    LD_PRELOAD=/path/ftlpad.so FTLPAD_LAYOUT=/path/ftlpad.ini

Other variables are in [Configuration](CONFIGURATION.md#environment-variables).

## Editing the layout

`ftlpad.ini` is re-read within a second of saving it, while the game runs. See
[Configuration](CONFIGURATION.md) and [Controls](CONTROLS.md#custom-layouts).

## Pads

Face buttons go by their labels. With `SDL_GAMECONTROLLERCONFIG` set, its mapping names
them. Without it, `swap_ab` and `swap_xy` in `[config]` swap the raw A/B and X/Y pairs;
both are off by default.

## Troubleshooting

ftlpad writes to the game's standard error, each line starting with `ftlpad:`. With Steam,
start Steam from a terminal to see it; without Steam, the terminal that runs `ftlpad.sh`
shows it.

No `ftlpad:` lines means the library did not load, or the game has none of the symbols
ftlpad looks for; check the path in the launch options. `unknown build, inactive` means
another build of FTL than the Steam 1.6.13 one.

`FTLPAD_DEBUG=1` in the environment also logs every press, wheel pick and D-pad step.
