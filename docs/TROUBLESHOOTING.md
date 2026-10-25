# Troubleshooting

ftlhelm logs to the game's standard error, each line starting with `ftlhelm:`. With Steam,
start Steam from a terminal to see it.

| Log | Meaning |
| --- | --- |
| no `ftlhelm:` lines | the library did not load, or the process is not the native FTL: check the path in the launch options and that no Steam Play compatibility tool is forced |
| `unknown build, inactive` | an FTL build other than Steam 1.6.13 or GOG 1.6.12 |
| `cannot save` | `ftlhelm.ini` or its directory is not writable; zoom and text size reset on the next run |
| `cannot write the default` | `ftlhelm.ini` is missing and its directory is not writable; the default layout runs, nothing is saved |
| `zoom off` in the `active` line | the game's frame blit was not found; zoom is unavailable |

`FTLHELM_DEBUG=1` also logs every press, wheel pick and D-pad step.
