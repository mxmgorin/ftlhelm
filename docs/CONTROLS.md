# Controls

The default layout is `ftlpad.ini`, each binding commented in place; holding Start in the
game shows a table of what each button does. Buttons go by their labels.

## Wheels

A wheel opens around the cursor on the ship view and stays open after the button is
released.

| Input | In a wheel |
| --- | --- |
| either stick | point at a plate |
| D-pad left, right | previous, next plate |
| D-pad up, down | add, take power; on the crew wheel, add to the selection, drop from it |
| A | act on the plate |
| B, or the wheel's own button | close |

## Custom layouts

`ftlpad.ini` is gptokeyb2's ini format with wheels and built-in actions on top. `#` starts
a comment outside quotes.

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
jumps between zones, and moves the cursor in the free cursor mode. `dpad = mouse_movement`
makes the D-pad move the cursor in both modes. `dpad_mode` switches the mode.
`overlay = parent` lets a `[controls:NAME]` state fall through to the state below for
inputs it does not bind.

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
| `pause` | pause |
| `fast_forward` | run fast while held |
| `dpad_mode` | switch between zones and the free cursor mode |
| `help` | show the controls table; any button but the D-pad closes it |

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

Plates go by direction: `up`, `up_right`, `right`, `down_right`, `down`, `down_left`,
`left`, `up_left`. A plate is an action, an optional quoted label (the direction's name
without one), and an optional icon: a game image path under `img/` in `ftl.dat`, with
`@X,Y,W,H` for a region of it in pixels.
