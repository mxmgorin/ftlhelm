# Controls

Buttons go by their labels. The default buttons are in the [README](../README.md#controls);
the Help plate of the settings wheel shows them in the game. Settings, bindings and wheels
are set in `ftlhelm.ini`, see [Configuration](CONFIGURATION.md).

## Zones

The D-pad and the sticks move the cursor to the next zone in the pressed direction; a stick
moves it once per push past `flick_threshold`.

| Screen | Zones |
| --- | --- |
| ship view | the rooms of both ships, buttons |
| event | its choices |
| star map | beacons or sectors, the map's buttons |
| ship and store screens | slots and buttons, without the HUD |
| menus | their items |

With `focus_first`, a menu, a dialog or the map opens with the cursor on its first item.

Y selects the next crew member; A on a room orders the selected crew there; B clears the
selection. With `crew_follow`, the cursor moves to the room of a newly selected member.

## Doors

While L2 is held with no crew selected, the doors of the player ship are the zones; A opens
or closes the door under the cursor. An L2 tap opens or closes all doors of the room under
the cursor.

## Mouse mode

L3 or R3, or the Mouse plate of the settings wheel, switches the D-pad and both sticks
between zones and mouse mode. In mouse mode they move the cursor freely, at `cursor_speed`,
with `cursor_deadzone` and `cursor_curve` for the sticks.

The badge in the top right corner names the active modes: Mouse, Doors, Fast (fast
forward), Target with the armed weapon, and Paused on a zoomed ship view. `overlay = 0`
hides it.

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

`autopause_wheel` pauses the game while a wheel is open; `wheels_ship_only = 0` opens
wheels over menus, screens and the map too.

## Settings wheel

| Plate | A | D-pad up, down |
| --- | --- | --- |
| Mouse | switch the D-pad and sticks between zones and mouse mode | |
| Zoom | next zoom level, back to the whole frame after the last | zoom in, out |
| Text | next text size, back to auto after the largest | larger, smaller text |
| Help | the controls table | |

Zoom and text size are kept between runs, see [Configuration](CONFIGURATION.md#settings).

## Zoom

Zoom magnifies the ship view; the view follows the cursor and fills the screen. While
zoomed, the map, the ship and store screens, the game menu and the options show whole, as
large as the screen allows; event dialogs, the main menu and the controls overview show
the whole frame. Zoom levels: the whole frame, the level that fills the screen, and the
levels where a game pixel takes a whole or half number of screen pixels, up to 3 times the
whole frame. Auto, the default, is one level in on screens smaller than 1280x720 and the
whole frame on others.

| Input, zoomed | Effect |
| --- | --- |
| D-pad or flick toward no zone | move the view half its size that way, until the cursor moves |
| hold Start | the whole frame until release |

The view rests on the frame's edges while the cursor stays clear of the view's edges.

## Text size

Text sizes apply to event boxes and tooltips: 1 to 2 times the game's own, or auto. Auto
enlarges them on screens smaller than 1280x720, less at higher zoom levels.
