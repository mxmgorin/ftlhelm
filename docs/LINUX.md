# Linux

ftlhelm runs inside the native Linux build of FTL (`FTL.amd64`, x86_64): Steam 1.6.13 or
GOG 1.6.12. The library needs glibc 2.35 or later.

## Files

`ftlhelm-linux-x86_64.tar.gz`, from the latest release or `make dist`, holds:

| File | Contents |
| --- | --- |
| `ftlhelm.so` | the library |
| `ftlhelm.ini` | the layout and its settings |
| `ftlhelm.sh` | runs a command with `ftlhelm.so` preloaded and `ftlhelm.ini` as the layout |
| `LICENSE` | the GNU General Public License v3.0 |
| `NOTICE` | copyright, additional license terms, third-party notices |

`ftlhelm.sh` expects the other two files next to it.

## Steam

Setup steps are in the [README](../README.md#install). `ftlhelm.sh` keeps the preloads Steam
sets, such as the Steam overlay.

## GOG

    ~/ftlhelm/ftlhelm-linux-x86_64/ftlhelm.sh "/path/to/the GOG install/start.sh"

## Without Steam

A Steam copy runs from its own directory:

    cd "/path/to/FTL Faster Than Light"
    ~/ftlhelm/ftlhelm-linux-x86_64/ftlhelm.sh ./FTL

## By hand

The environment `ftlhelm.sh` sets, for another launcher:

    LD_PRELOAD=/path/ftlhelm.so FTLHELM_LAYOUT=/path/ftlhelm.ini

Other variables are in [Configuration](CONFIGURATION.md#environment-variables).

## Troubleshooting

ftlhelm logs to the game's standard error, each line starting with `ftlhelm:`. With Steam,
start Steam from a terminal to see it.

| Log | Meaning |
| --- | --- |
| no `ftlhelm:` lines | the library did not load, or the process is not the native FTL: check the path in the launch options and that no Steam Play compatibility tool is forced |
| `unknown build, inactive` | an FTL build other than Steam 1.6.13 or GOG 1.6.12 |
| `cannot save` | `ftlhelm.ini` or its directory is not writable; zoom and text size reset on the next run |
| `zoom off` in the `active` line | the game's frame blit was not found; zoom is unavailable |

`FTLHELM_DEBUG=1` also logs every press, wheel pick and D-pad step.
