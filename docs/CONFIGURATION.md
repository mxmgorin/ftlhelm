# Configuration

`ftlhelm.ini` holds the settings, the button states and the wheels. `#` starts a comment
outside quotes. The file is re-read within a second of saving it, while the game runs.
Deleting it resets the layout and the settings: ftlhelm writes the default file in its place.

| Section | Contents |
| --- | --- |
| `[config]` | [settings](#settings) |
| `[controls]` | the base [state](#states) |
| `[controls:NAME]` | a state pushed while a `hold_state NAME` button is held |
| `[wheel:NAME]` | a [wheel](#wheels) opened by `wheel NAME` |

## Settings

One `key = value` per line, each key documented in place in `ftlhelm.ini`. Switches take
`0` (off) or `1` (on). A missing key takes its default; a value outside its range is
clamped. When the settings wheel closes, it writes `zoom` and `text_scale` here.

### Cursor

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `cursor_speed` | 800 | 1 and up | cursor speed at full stick deflection, in the game's 1280x720 pixels per second |
| `cursor_deadzone` | 0.25 | 0 to 0.9 | stick travel ignored around the centre |
| `cursor_curve` | 2 | 0.5 to 4 | response curve exponent: 1 is linear, higher is finer near the centre |

### Zones

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `snap_dpad` | 1 | switch | the D-pad jumps between zones unless it moves the cursor or a `hold_state` state is held |
| `flick_threshold` | 0.8 | 0.35 to 1 | stick deflection that fires a flick; the stick re-arms below 0.25 |
| `flick_repeat` | 0 | 0 and up | seconds between repeated flicks while the stick stays deflected; 0: one flick per deflection |
| `focus_cone_narrow` | 30 | 5 to 89 | half-angle in degrees of the cone searched first for the next zone |
| `focus_cone_wide` | 60 | 5 to 89 | half-angle in degrees of the cone searched next |
| `focus_side_weight` | 2 | 0 to 10 | how many times sideways distance counts over forward |
| `focus_first` | 1 | switch | the cursor on the first item when a menu, a dialog or the map opens |
| `crew_follow` | 0 | switch | the cursor to the room of a newly selected crew member |

### Game

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `wheels_ship_only` | 1 | switch | wheels open only on the ship view; the settings wheel opens anywhere |
| `autopause_wheel` | 1 | switch | pause while a wheel is open |
| `autopause_aim` | 0 | switch | pause when a weapon or ability starts waiting for a target |
| `speed_level` | 6 | 1 to 6 | game speed while fast forward is held; each game step is scaled by (16 + 7 * level) / 16 |

### Pad

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `tap_hold` | 0.2 | 0.05 to 2 | seconds a press lasts to count as a hold on a button with a tap action |
| `rumble` | 1 | switch | a pulse when the cursor jumps to a zone or the wheel selection changes |
| `rumble_strength` | 0.4 | 0 to 1 | pulse strength |
| `rumble_time` | 0.05 | 0.01 to 0.5 | pulse length in seconds |
| `swap_ab`, `swap_xy` | 0 | switch | swap the raw A/B, X/Y pairs to match the labels |

Face buttons go by their labels. With `SDL_GAMECONTROLLERCONFIG` set, its mapping names
them; without it, `swap_ab` and `swap_xy` apply.

### Screen

| Key | Default | Range | Effect |
| --- | --- | --- | --- |
| `overlay` | 1 | switch | the mode badge in the top right corner |
| `zoom` | 0 | 0, 1 to 3 | ship view [zoom](CONTROLS.md#zoom) in times the whole frame; 0: auto |
| `text_scale` | 0 | 0, 1 to 2 | event and tooltip [text size](CONTROLS.md#text-size) in times the game's own; 0: auto |

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

`left_analog` and `right_analog` take `flick` or `mouse_movement`; either way the stick
jumps between zones, and moves the cursor in mouse mode. `dpad = mouse_movement`
makes the D-pad move the cursor in both modes. `dpad_mode` switches the mode.
`overlay = parent` lets a `[controls:NAME]` state fall through to the state below for
inputs it does not bind. A press made while a `hold_state` button is down resolves in its
state.

### Actions

| Action | Effect |
| --- | --- |
| a key name | press the key: `a`..`z`, `0`..`9`, `f1`..`f15`, `space`, `enter`, `esc`, `tab`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `insert`, `home`, `end`, `pageup`, `pagedown`, `shift`, `ctrl`, `alt`, `/`, `-`, `=`, `,`, `.`, `;`, `'`, `[`, `]`, `\`, `` ` ``, `+` |
| key name `add_shift` | the key with Shift |
| `mouse_left`, `mouse_right` | click at the cursor |
| `hold_state NAME` | push `[controls:NAME]` while held |
| `wheel NAME` | open `[wheel:NAME]` |
| a built-in | see below |
| `HOLD \| TAP` | the tap action on a press shorter than `tap_hold`, the hold action past it |

| Built-in | Effect |
| --- | --- |
| `context_click` | click; with crew selected, a room orders them there; in a wheel, act |
| `cancel` | close the wheel, cancel the aim, clear the crew, close the menu or a screen |
| `arm_next` | arm the next powered weapon |
| `aim_crew` | select the next crew member |
| `prev_tab`, `next_tab` | ship and store screen tabs |
| `repeat_ability` | the last ability used from a wheel |
| `precision` | doors become the zones while held |
| `room_doors` | toggle the doors of the room under the cursor |
| `center_swap` | the cursor to the centre of the farther ship |
| `pause` | pause; held, the whole frame of a zoomed view until release |
| `fast_forward` | run fast while held |
| `dpad_mode` | switch between zones and mouse mode |
| `help` | show the controls table; any button but the D-pad closes it |
| `zoom` | the next zoom level, back to the whole frame after the last |
| `text_size` | the next text size, back to auto after the largest |

## Wheels

    [wheel:ship]
    kind = action
    up = u "Upgrades" statusUI/top_ship_off.png
    up_right = z "Open doors" systemUI/button_door_bottom_off.png@10,33,20,20

| `kind` | Plates |
| --- | --- |
| `action` | from the section; the wheel closes after an action |
| `power` | from the section, with power pips; D-pad up and down add and take power |
| `crew` | filled from the ship's crew |
| `systems` | filled from the ship's systems |
| `settings` | from the section; opens on any screen; D-pad up and down step `zoom` and `text_size` plates; other plates close the wheel after an action |

Plates go by direction: `up`, `up_right`, `right`, `down_right`, `down`, `down_left`,
`left`, `up_left`. A plate is an action, an optional quoted label (the direction's name
without one), and an optional icon: a game image path under `img/` in `ftl.dat`, with
`@X,Y,W,H` for a region of it in pixels.

## Environment variables

| Variable | Value |
| --- | --- |
| `LD_PRELOAD` | path to `ftlhelm.so`; under box64, `BOX64_LD_PRELOAD` |
| `FTLHELM_LAYOUT` | path to `ftlhelm.ini`, written with the default layout when missing; without it, the default layout, and nothing is saved |
| `FTLHELM_EXE` | path to `FTL.amd64`; default `/proc/self/exe` |
| `FTLHELM_CMD` | path to a FIFO for test commands, see [Building](BUILDING.md#test-channel) |
| `FTLHELM_DEBUG` | set: log every press, wheel pick and D-pad step |
| `SDL_GAMECONTROLLERCONFIG` | an SDL mapping that names the pad's buttons |
