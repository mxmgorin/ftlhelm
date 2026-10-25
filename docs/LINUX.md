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

`ftlpad.sh` expects the other two files next to it.

## Steam

Setup steps are in the [README](../README.md#install). `ftlpad.sh` keeps the preloads Steam
sets, such as the Steam overlay.

## Without Steam

    ~/ftlpad/ftlpad-linux-x86_64/ftlpad.sh "/path/to/FTL Faster Than Light/FTL"

`FTL` is the game's own launch script; it starts `data/FTL.amd64`.

## By hand

The environment `ftlpad.sh` sets, for another launcher:

    LD_PRELOAD=/path/ftlpad.so FTLPAD_LAYOUT=/path/ftlpad.ini

Other variables are in [Configuration](CONFIGURATION.md#environment-variables).

## Pads

Face buttons go by their labels. With `SDL_GAMECONTROLLERCONFIG` set, its mapping names
them. Without it, `swap_ab` and `swap_xy` in `[config]` swap the raw A/B and X/Y pairs;
both are off by default.

## Troubleshooting

ftlpad logs to the game's standard error, each line starting with `ftlpad:`. With Steam,
start Steam from a terminal to see it.

| Log | Meaning |
| --- | --- |
| no `ftlpad:` lines | the library did not load, or the process is not FTL: check the path in the launch options |
| `unknown build, inactive` | an FTL build other than Steam 1.6.13 |

`FTLPAD_DEBUG=1` also logs every press, wheel pick and D-pad step.
