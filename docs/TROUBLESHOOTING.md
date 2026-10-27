# Troubleshooting

ftlhelm logs each line with `ftlhelm:` in front, to the game's standard error and, when the
game starts through `ftlhelm.sh`, to `ftlhelm.log` next to it. `ftlhelm.old.log` holds the run
before. Attach `ftlhelm.log` to a bug report.

| Log | Meaning |
| --- | --- |
| no `ftlhelm.log`, no `ftlhelm:` lines | the library did not load, or the process is not the native FTL: check the path in the launch options and that no Steam Play compatibility tool is forced |
| `unknown build, inactive` | an FTL build other than Steam 1.6.13 or GOG 1.6.12 |
| `cannot save` | `ftlhelm.ini` or its directory is not writable; zoom and text size reset on the next run |
| `cannot write the default` | `ftlhelm.ini` is missing and its directory is not writable; the default layout runs, nothing is saved |
| `pointer off` in the `active` line | the pointer arrow stays shown after a touch and on a bracketed zone, and keeps its size on a small screen |
| `pointer tooltip off` in the `active` line | the pointer arrow keeps its size on a small screen |
| `zoom off` in the `active` line | the game's frame blit was not found; zoom is unavailable |
| `pad N: NAME (VENDOR:PRODUCT)` | a pad connected |
| `pad N: active` | ftlhelm reads this pad |
| `pad N: gone` | the pad disconnected |
| `pad N: fed by Steam Input` | Steam Input is on for FTL; if presses act twice, set it to **Disabled** in FTL's **Properties > Controller**, or on a Steam Deck use the **Gamepad** template |
| `touch: touchscreen found` | the game takes touches as touches, see [Touch](CONTROLS.md#touch) |
| `touch: no touchscreen found` | touches reach the game as mouse clicks, if at all |
| no `touch: first event` after a touch | touches reach the game as mouse clicks; on a Steam Deck in Game Mode, see [Steam Deck](../README.md#steam-deck) |
| `panic at FILE:LINE` | an ftlhelm bug, at that place in its source |
| `crash:` lines | the game crashed: the signal, the code it crashed in (`game` or `ftlhelm+OFFSET`), and the ftlhelm hook running, if any |

`FTLHELM_DEBUG=1` also logs every press, wheel pick and D-pad step.
