# Controls

The default layout is `ftlhelm.ini`, each binding commented in place; the Help plate of the
settings wheel shows a table of what each button does. Buttons go by their labels.

## Settings wheel

A Select tap opens the settings wheel, on any screen.

| Plate | A | D-pad up, down |
| --- | --- | --- |
| Mouse | switch the D-pad and sticks between zones and mouse mode | |
| Zoom | next zoom level, back to the whole frame after the last | zoom in, out |
| Text | next text size, back to auto after the largest | larger, smaller text |
| Help | the controls table | |

Zoom magnifies the ship view; the view follows the cursor and fills the screen. While
zoomed, the map, the ship and store screens, the game menu and the options show whole, as
large as the screen allows; event dialogs, the main menu and the controls overview show
the whole frame. Zoom levels: the
whole frame, the level that fills the screen, and the levels where a game pixel takes a
whole or half number of screen pixels, up to 3 times the whole frame.

| Input, zoomed | Effect |
| --- | --- |
| D-pad or flick toward no zone | move the view half its size that way, until the cursor moves |
| hold Start | the whole frame until release |

The view rests on the frame's edges while the cursor stays clear of the view's edges.

Text sizes apply to event boxes and tooltips: 1 to 2 times the game's own, or auto. Auto
enlarges them on screens smaller than 1280x720, less at higher zoom levels.

Zoom and text size are saved in `ftlhelm-saved.ini` beside `ftlhelm.ini` when the wheel
closes. The saved values override `zoom` and `text_scale` in `ftlhelm.ini` until one of
those two is changed in `ftlhelm.ini`.

## Wheels

Holding B, X, Y, L1 or R1 opens a wheel around the cursor on the ship view; a Select tap
opens the settings wheel on any screen. A wheel stays open after the button is released.

| Input | In a wheel |
| --- | --- |
| either stick | point at a plate |
| D-pad left, right | previous, next plate |
| D-pad up, down | add, take power; on the crew wheel, add to the selection, drop from it; on the settings wheel, step the zoom or text size |
| A | act on the plate |
| B, or the wheel's own button | close |
| hold another wheel's button | switch to that wheel |

## Custom layouts

`ftlhelm.ini` holds the settings, the button states and the wheels. `#` starts a comment
outside quotes.

| Section | Contents |
| --- | --- |
| `[config]` | settings, see [Configuration](CONFIGURATION.md) |
| `[controls]` | the base state |
| `[controls:NAME]` | a state pushed while a `hold_state NAME` button is held |
| `[wheel:NAME]` | a wheel opened by `wheel NAME` |

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

### Wheels

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
