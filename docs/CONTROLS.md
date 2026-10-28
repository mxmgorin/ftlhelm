# Controls

Buttons go by their labels. The default buttons are in the [README](../README.md#controls);
the Help plate of the settings wheel shows them in the game. Settings, bindings and wheels
are set in `ftlhelm.ini`, see [Configuration](CONFIGURATION.md).

The controls table shows at the first start, once a pad is connected; any button but the D-pad closes it.

## Pads

ftlhelm reads one pad: the first connected, then whichever pad presses a button while the
active one has none down. When the active pad disconnects, the ship view pauses and
"Pad disconnected" shows until a button is pressed or the screen is touched.

## Zones

The D-pad and the sticks move the cursor to the next zone in the pressed direction; a stick
moves it once per push past `flick_threshold`. With `focus_brackets`, the zone the cursor
lands on shows corner brackets in place of the pointer arrow until the cursor moves on.

| Screen | Zones |
| --- | --- |
| ship view | the rooms of both ships, buttons |
| event | its choices |
| star map | beacons or sectors, the map's buttons |
| ship and store screens | slots and buttons, without the HUD |
| menus | their items |

With `focus_first`, a menu, a dialog or the map opens with the cursor on its first item.

Y selects the next crew member; A on a room orders the selected crew there; B clears the
selection. With `room_crew_pick` and no crew selected in another room, A on a room with crew
selects its members one at a time, then all of them. With `crew_marker`, a green marker shows over each selected crew member on the ship. With `crew_follow`, the cursor moves to the room of a newly selected member and brackets it.

## Doors

While L2 is held with no crew selected, the doors of the player ship are the zones; A opens
or closes the door under the cursor. An L2 tap opens or closes all doors of the room under
the cursor.

## Mouse mode

L3 or R3, or the Mouse plate of the settings wheel, switches the D-pad and both sticks
between zones and mouse mode. In mouse mode they move the cursor freely, at `cursor_speed`,
with `cursor_deadzone` and `cursor_curve` for the sticks.

The badge in the top right corner names the selected crew member, or counts several, and the
active modes: Mouse, Doors, Fast (fast forward) and Target with the armed weapon. `badge = 0`
hides it; `overlay = 0` hides it, the zone brackets and the crew markers.

## Touch

A touchscreen acts as the mouse, one finger at a time; a touch lands where the screen
shows it, zoomed or not. A touch on nothing the game takes moves to the nearest room,
button, beacon, choice, slot, system icon or weapon box within `touch_snap`.

| Touch | Effect |
| --- | --- |
| tap | click; with crew selected, a room orders them there unless a crew member is under the finger |
| hold for `touch_long_press`, release | right click; a ring around the finger fills while it is held and turns white once the hold is long enough |
| move up or down from a system icon or a weapon box | a bar of power added or removed per `touch_step`; the power follows the finger back; a weapon or drone only switches on or off |
| drag | drag with the left button: a selection box, an item, a slider; also after a hold |
| tap with two fingers | pause or resume |

After a touch, the pointer arrow hides and, with `pause_button`, the ship view shows a pause
button in the bottom right corner, lit yellow while the game is paused, until a pad or the
mouse is used; tooltips and the targeting pointers stay. A zoomed view without that corner
shows the button in its top right corner. A touch's click leaves a small ring that fades.
`touch_marks = 0` hides both rings.

A tap closes the controls table. With a wheel open, a tap on a plate acts on it; a tap
elsewhere closes the wheel.

## Wheels

Holding B, X, Y, L1 or R1 opens a wheel around the cursor on the ship view; a Select tap
opens the settings wheel on any screen. A wheel stays open after the button is released.

| Input | In a wheel |
| --- | --- |
| either stick | point at a plate |
| D-pad left, right | previous, next plate |
| D-pad up, down | add, take power; on the crew wheel, add to the selection, drop from it; on the settings wheel, step the plate's setting |
| A | act on the plate |
| B, or the wheel's own button | close |
| hold another wheel's button | switch to that wheel |

`autopause_wheel` pauses the game while a wheel is open; `wheels_ship_only = 0` opens
wheels over menus, screens and the map too.

## Settings wheel

| Plate | A | D-pad up, down |
| --- | --- | --- |
| Mouse | switch the D-pad and sticks between zones and mouse mode | |
| Zoom | next zoom level, back to auto after the last | zoom in, out; auto below the whole frame |
| Autopause | both autopauses off when either is on, else both on: while a wheel is open, and while a weapon or ability waits for a target | on, off |
| Fast speed | next fast forward speed, the slowest after the fastest | faster, slower |
| Help | the controls table | |
| Rumble | next rumble strength, off after the strongest; a pulse at the new strength | stronger, weaker |
| Overlay | switch the badge, the zone brackets and the crew markers | on, off |
| Text | next text size, back to auto after the largest | larger, smaller text; auto below the smallest |

Autopause is lit while `autopause_wheel` or `autopause_aim` is on. Mouse mode resets each
run; the other values are kept, see [Configuration](CONFIGURATION.md#settings).

## Zoom

Zoom magnifies the ship view; the view follows the cursor and fills the screen. While
zoomed, the map, the ship and store screens, the game menu and the options show whole, as
large as the screen allows; event dialogs, the main menu and the controls overview show
the whole frame. Zoom levels: the whole frame, the level that fills the screen, and the
levels where a game pixel takes a whole or half number of screen pixels, up to 3 times the
whole frame. Auto, the default, is one level in on screens smaller than 1280x720 and the
whole frame on others.

On a screen narrower than 1280x720, the HUD's edge panels move toward the middle as far as
the player ship leaves room (`hud_shift`), and the ship view keeps to the columns the HUD
covers: its lowest level shows the whole HUD in place of the whole frame.

| Input, zoomed | Effect |
| --- | --- |
| D-pad or flick toward no zone | move the view half its size that way, until the cursor moves |
| hold Start | the lowest level until release: the whole frame, or the whole HUD on a narrower screen |

The view rests on the frame's edges, or the HUD's, while the cursor stays clear of the
view's edges.

## Text size

Text sizes apply to event boxes and tooltips: 1 to 2 times the game's own, or auto. Auto
enlarges them on screens smaller than 1280x720, less at higher zoom levels. The controls
table and notices take the same size, as far as the frame holds them.

On a screen that shows the frame smaller than 1280x720, the pointer is drawn larger, up to
`pointer_scale_max` times its size, the crew markers up to one and a half times, and the
zone brackets keep their size.
