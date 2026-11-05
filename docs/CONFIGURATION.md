# Configuration

`ftlhelm.ini` holds the settings, the bindings and the wheels. Changes apply on save, while
the game runs. `#` starts a comment outside quotes. Deleting the file resets it to the
default.

An outdated file is replaced by the default with its `[config]` values kept; the old one stays
beside it as `ftlhelm.old.ini`.

| Section | Contents |
| --- | --- |
| `[config]` | [settings](#settings) |
| `[controls]` | the base [state](#states) |
| `[controls:NAME]` | a state active while a `hold_state NAME` button is held |
| `[wheel:NAME]` | a [wheel](#wheels) opened by `wheel NAME` |

## Settings

One `key = value` per line. Switches take `0` (off) or `1` (on). A missing key takes its
default; a value out of range is clamped. Changes made in the wheels are saved here.

### Cursor

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `cursor_speed` | 800 | 1 and up | mouse mode cursor speed at full stick, in game pixels per second |
| `cursor_deadzone` | 0.25 | 0 to 0.9 | stick dead zone |
| `cursor_curve` | 2 | 0.5 to 4 | stick response: 1 linear, higher finer near the centre |

### Zones

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `snap_dpad` | 1 | switch | the D-pad jumps between zones |
| `flick_threshold` | 0.8 | 0.5 to 1 | stick push that steps to the next zone |
| `focus_repeat` | 2 | 0 to 3 | repeat speed of a held direction: 0 off, 1 slow, 2 medium, 3 fast |
| `focus_cone_narrow` | 30 | 5 to 89 | degrees either side searched first for the next zone |
| `focus_cone_wide` | 60 | 5 to 89 | degrees either side searched next |
| `focus_side_weight` | 2 | 0 to 10 | weight of sideways distance against forward |
| `focus_first` | 1 | switch | the cursor on the first item when a menu, a dialog or the map opens |
| `crew_follow` | 0 | switch | the cursor follows a newly selected crew member |
| `room_crew_pick` | 1 | switch | A on a room with crew selects its crew; 0: it clicks |

### Game

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `wheels_ship_only` | 1 | switch | wheels open only on the ship view; the settings wheel opens anywhere |
| `wheel_place` | center | `center`, `cursor` | where a wheel opens |
| `wheel_backdrop` | octagon | `octagon`, `octagon_dark`, `circle`, `circle_dark`, `dim`, `disc` | the backdrop under a wheel |
| `autopause` | 1 | 0 to 3 | the game pauses: 1 while a wheel is open, 2 while aiming, 3 both; 0: never |
| `cancel_pause` | 1 | switch | `cancel` with nothing to cancel pauses or resumes the game on the ship view |
| `speed_level` | 6 | 1 to 6 | fast forward speed |

### Pad

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `tap_hold` | 0.2 | 0.05 to 2 | seconds a press lasts to count as a hold |
| `rumble` | 1 | 0 to 2.5 | rumble strength; 0: none |
| `rumble_time` | 0.05 | 0.01 to 0.5 | rumble pulse in seconds |
| `swap_buttons` | 0 | 0 to 3 | swap face buttons: 1 A/B, 2 X/Y, 3 both; ignored with `SDL_GAMECONTROLLERCONFIG` set |

### Screen

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `overlay` | 1 | switch | the badge, the focus brackets and the crew markers; 0: none of them |
| `badge` | 1 | switch | the badge with the selected crew and the active modes |
| `focus_brackets` | 1 | switch | brackets on the zone under the cursor |
| `crew_marker` | 1 | switch | markers over the selected crew |
| `pointer_scale_max` | 2 | 1 to 4 | the largest pointer scale on small screens |
| `zoom` | 0 | 0, 1 to 3 | ship view [zoom](CONTROLS.md#zoom); 0: auto |
| `ipad_beacons` | 1 | switch | map beacons in the iPad art |
| `ipad_station_buttons` | 1 | switch | crew station buttons in the iPad art |
| `hud_shift` | 1 | switch | HUD panels moved toward the middle on narrow screens |
| `text_scale` | 0 | 0, 1 to 2 | event and tooltip [text size](CONTROLS.md#text-size); 0: auto |
| `help_at_start` | 1 | switch | the help on the first run |
| `frame_filter` | 1 | switch | smoother picture on small screens |
| `sharpen` | 0.5 | 0 to 1 | sharpening of the filtered picture |

### Touch

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `touch` | 1 | switch | the touchscreen, see [Touch](CONTROLS.md#touch) |
| `touch_long_press` | 0.5 | 0.05 to 2 | seconds held for a right click |
| `touch_slop` | 12 | 0 to 100 | screen pixels a finger may move and still tap |
| `touch_snap` | 32 | 0 to 100 | screen pixels a touch reaches for the nearest target; 0: none |
| `touch_step` | 16 | 0 to 100 | screen pixels of finger travel per bar of power; 0: none |
| `pause_button` | 1 | switch | the pause button after a touch |
| `touch_marks` | 1 | switch | rings where a finger holds and taps |

## Bindings

### States

    a = context_click
    b = wheel abilities | cancel
    left_analog = mouse_movement

| Input | Names |
| --- | --- |
| face buttons, by label | `a`, `b`, `x`, `y` |
| shoulders and triggers | `l1`, `r1`, `l2`, `r2` |
| stick buttons | `l3`, `r3` |
| others | `start`, `back`, `guide` |
| D-pad | `up`, `down`, `left`, `right` |
| stick directions | `left_analog_up`, `left_analog_down`, `left_analog_left`, `left_analog_right`, the same for `right_analog` |

`left_analog` and `right_analog` take `flick` or `mouse_movement`; `dpad = mouse_movement`
makes the D-pad move the cursor. `overlay = parent` makes a `[controls:NAME]` state use the
state below for inputs it does not bind.

### Actions

| Action | Effect |
| --- | --- |
| a key name | press the key: `a`..`z`, `0`..`9`, `f1`..`f15`, `space`, `enter`, `esc`, `tab`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `insert`, `home`, `end`, `pageup`, `pagedown`, `shift`, `ctrl`, `alt`, `/`, `-`, `=`, `,`, `.`, `;`, `'`, `[`, `]`, `\`, `` ` ``, `+` |
| key name `add_shift` | the key with Shift |
| `mouse_left`, `mouse_right` | click at the cursor |
| `hold_state NAME` | `[controls:NAME]` while held |
| `wheel NAME` | open `[wheel:NAME]` |
| `set KEY` | the next value of setting `KEY` |
| a built-in | see below |
| `HOLD \| TAP` | the tap action on a short press, the hold action on a hold |

| Built-in | Effect |
| --- | --- |
| `context_click` | click; with crew selected, send them to the room; in a wheel, act |
| `cancel` | close the wheel or go back; cancel the aim, clear the crew, close a menu or a screen; with nothing to cancel, pause (`cancel_pause`) |
| `arm_next` | arm the next powered weapon |
| `arm_all` | arm every powered weapon for a volley |
| `aim_crew` | select the next crew member |
| `prev_tab`, `next_tab` | previous, next tab of the ship and store screens |
| `repeat_ability` | the last ability used from a wheel |
| `precision` | doors become the zones while held |
| `room_doors` | open or close the doors of the room under the cursor |
| `center_swap` | the cursor to the other ship |
| `pause` | pause; held while zoomed, zoom out until release |
| `fast_forward` | fast forward while held |
| `dpad_mode` | switch between zones and mouse mode |
| `help` | the help |
| `zoom` | the next zoom level |
| `text_size` | the next text size |
| `autopause` | the next `autopause` value |

## Wheels

    [wheel:ship]
    kind = action
    up = center_swap "Other ship"
    up_right = x "Close doors" systemUI/button_door_top_on.png@10,13,20,15

| `kind` | Plates |
| --- | --- |
| `action` | from the section; the wheel closes after an action |
| `power` | from the section, with power pips |
| `crew` | the ship's crew |
| `systems` | the ship's systems |
| `settings` | from the section; opens on any screen |

Plate directions: `up`, `up_right`, `right`, `down_right`, `down`, `down_left`, `left`,
`up_left`. A plate is an action, an optional quoted label and an optional icon: an image path
under `img/` in `ftl.dat`, with `@X,Y,W,H` for a region of it.

## Environment variables

`ftlhelm.sh` sets them. Another launcher sets them itself:

    LD_PRELOAD=/path/ftlhelm.so FTLHELM_LAYOUT=/path/ftlhelm.ini FTLHELM_LOG=/path/ftlhelm.log

| Variable | Value |
| --- | --- |
| `LD_PRELOAD` | path to `ftlhelm.so`; under box64, `BOX64_LD_PRELOAD` |
| `FTLHELM_LAYOUT` | path to `ftlhelm.ini`, created when missing; without it, the default layout, nothing saved |
| `FTLHELM_LOG` | path to the log file; the previous run's log is kept as `.old.log` |
| `FTLHELM_EXE` | path to `FTL.amd64`; default `/proc/self/exe` |
| `FTLHELM_DEBUG` | set: log every press, wheel pick and D-pad step |
| `SDL_GAMECONTROLLERCONFIG` | an SDL mapping that names the pad's buttons |
