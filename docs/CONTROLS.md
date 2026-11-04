# Controls

Buttons go by their labels. The default buttons are in the [README](../README.md#controls);
the Help plate of the settings wheel shows them in the game. Settings, bindings and wheels
are set in `ftlhelm.ini`, see [Configuration](CONFIGURATION.md).

## Help

The help opens from the Help plate of the settings wheel, and once by itself: the first time
the ship view shows with a pad connected. On the ship view the game is paused while it shows.

| Input | In the help |
| --- | --- |
| L1, R1 or D-pad left, right | previous, next page |
| any other button | close |

| Page | Shows |
| --- | --- |
| Buttons | each button's tap and hold, then what the D-pad and the sticks do |
| Wheels | the controls in a wheel |
| Weapons | arming, aiming, beam lines, moving weapons, picking a volley |
| Crew | selecting crew and sending them |
| Doors | doors mode and room doors |

A row names the button bound to its action, and is left out when none is.

## Pads

FTLHELM reads one pad: the first connected, then whichever pad presses a button while the
active one has none down. When the active pad disconnects, the ship view pauses and
"Pad disconnected" shows until a button is pressed or the screen is touched.

## Zones

The D-pad and the sticks move the cursor to the next zone in the pressed direction; a stick
moves it once per push past `flick_threshold`. A held direction repeats the step after a
moment, at the `focus_repeat` speed. With `focus_brackets`, the zone the cursor lands on
shows corner brackets in place of the pointer arrow until the cursor moves on. L3 or R3
moves the cursor to the other ship, on its room nearest the ship's middle.

| Screen | Zones |
| --- | --- |
| ship view | the rooms of both ships, the weapon and drone boxes holding an item (not while aiming), buttons |
| event | its choices |
| star map | beacons or sectors, the map's buttons |
| ship and store screens | slots and buttons, without the HUD |
| options | their choices and buttons |
| hangar | buttons, the weapon, drone, augment and crew boxes holding an item, the ship's achievements |
| game menu | its items, the ship's achievements |
| stats | its buttons, the achievements of the tab shown |
| menus | their items |

With `focus_first`, a menu, a dialog or the map opens with the cursor on its first item, and a
focus whose zone goes away moves to the nearest zone (not while aiming, in doors mode or during a
jump).

Y selects the next crew member; A on a room orders the selected crew there; B clears the
selection. With `room_crew_pick` and no crew selected in another room, A on a room with crew
selects its members one at a time, then all of them. With `crew_marker`, a green marker shows over each selected crew member on the ship. With `crew_follow`, the cursor moves to the room of a newly selected member and brackets it.

## Doors

While L2 is held with no crew selected, the doors of the player ship are the zones; A opens
or closes the door under the cursor. An L2 tap opens or closes all doors of the room under
the cursor.

## Weapons

An X tap arms the first powered weapon without a target; with a weapon armed, it arms the
next powered one.

A on a room places an armed beam's start at the room's centre and lays the beam on the line
over that centre through the most rooms. The D-pad and the sticks then set the line: left
and right turn it about its pivot, at first the room's centre; up and down slide its start
along it up or down the screen (a level beam's forward on up). With A held, they move the
pivot up, down, left or right, the line with it. A slide goes on while either end of the
beam is in a room, the beam turned round when only its far end is; a move keeps the pivot
in a room. Both stop at an edge with no room beyond, and the next step crosses to the next
room that way, if any. A step that cannot go on rumbles longer. A held direction repeats.
A fires the beam on its release, unless it moved the line; B takes its start back.

The All weapons plate of the weapons wheel arms every powered weapon and clears their
targets. A on a room aims them all at it; each beam then waits for its own line. B leaves
them all without a target. Autofire is the game's button under the weapons and a plate of the
ship wheel, lit while autofire is on.

A on a weapon or drone box clicks it. A held with D-pad left or right moves the weapon or
drone one box that way; on the weapons wheel, the same on its plate moves it one slot.

On the weapons wheel, D-pad up or R2 on a powered weapon picks it: the picked weapons, lit
yellow, aim together as with the All weapons plate. D-pad down or L2 drops a picked weapon,
then takes its power. A on a picked weapon closes the wheel.

## Mouse mode

The Mouse plate of the settings wheel switches the D-pad and both sticks between zones and
mouse mode. In mouse mode they move the cursor freely, at `cursor_speed`, with
`cursor_deadzone` and `cursor_curve` for the sticks.

The badge sits where the game writes PAUSED. It names the selected crew member, or counts
several, and the active modes: Mouse, Doors, Fast (fast forward) and Target with the armed
weapon, the number of picked weapons, or All weapons during a volley. Targeting and doors
mode show the game's own TARGETING and DOOR CONTROL banners, the rest a line under them; a
banner on a paused game takes PAUSED's place, and the line then ends in Paused. `badge = 0`
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

A tap on a tab of the help shows its page, on L1 or R1 the previous or next page; a tap
elsewhere closes it. With a wheel open, a tap on a plate acts on it; a tap
elsewhere closes the wheel.

## Wheels

Holding B, X, Y, L1 or R1 opens a wheel in the middle of the screen on the ship view; Select
opens the settings wheel on any screen. A wheel stays open after the button is released.

| Input | In a wheel |
| --- | --- |
| either stick | point at a plate |
| D-pad left, right | previous, next plate |
| D-pad up, down; R2, L2 | add, take power; on a powered weapon, pick it, drop it; on the crew wheel, add to the selection, drop from it; on the settings wheel, step the plate's setting |
| A | act on the plate; on a weapon or drone plate, on release |
| A held, D-pad left, right | on a weapon or drone plate, move it to the previous, next slot |
| B | close; in a wheel opened from a plate, back to the wheel it was opened from |
| the wheel's own button | close, with any wheel opened from it |
| hold another wheel's button | switch to that wheel |

`autopause` 1 or 3 pauses the game while a wheel is open; `wheels_ship_only = 0` opens
wheels over menus, screens and the map too; `wheel_place = cursor` opens them around the
cursor. While zoomed, an open wheel zooms out as far as it needs to show whole.

## Settings wheel

| Plate | A | D-pad up, down; R2, L2 |
| --- | --- | --- |
| Mouse | switch the D-pad and sticks between zones and mouse mode | mouse mode on, off |
| Zoom | next zoom level, back to auto after the last | zoom in, out; auto below the whole frame |
| Autopause | next mode: Wheels, Aim, Both, Off | next, previous mode |
| Fast speed | next fast forward speed, the slowest after the fastest | faster, slower |
| Help | the help | |
| Rumble | next rumble strength, off after the strongest; a pulse at the new strength | stronger, weaker |
| View | the view wheel | |
| Text | next text size, back to auto after the largest | larger, smaller text; auto below the smallest |

Autopause modes: Wheels pauses while a wheel is open, Aim while a weapon or ability waits
for a target, Both does both; the plate is lit unless Off. Mouse mode resets each
run; the other values are kept, see [Configuration](CONFIGURATION.md#settings).

## View wheel

| Plate | A | D-pad up, down; R2, L2 | Pickable |
| --- | --- | --- | --- |
| Overlay | switch the badge, the zone brackets and the crew markers | on, off | always |
| Crew marker | switch the crew markers | on, off | with the overlay on |
| HUD shift | switch the [HUD shift](#zoom) | on, off | on a screen narrower than 1280x720 |
| iPad beacons | switch the map's beacon icons to the iPad art | on, off | always |
| iPad buttons | switch the crew station buttons to the iPad art | on, off | always |
| Sharpen | next sharpening of the shrunk frame, off after the strongest | more, less | on a screen that shows the frame smaller, with the frame filter on |
| Frame filter | switch the area-averaged shrunk frame | on, off | on a screen that shows the frame smaller |
| Focus brackets | switch the zone brackets | on, off | with the overlay on |

## Zoom

Zoom magnifies the ship view; the view follows the cursor and fills the screen. While
zoomed on a screen smaller than 1280x720, the map, the ship and store screens, the game
menu and the options show whole, as large as the screen allows; on other screens, and
during the tutorial, they show at the lowest level. Event dialogs, the main menu and the help show the whole
frame. Zoom levels: the whole frame, the level that fills the screen, and the levels where
a game pixel takes a whole or half number of screen pixels, up to 3 times the whole frame.
Auto, the default, is one level in on screens smaller than 1280x720 and the whole frame on
others.

On a screen narrower than 1280x720, the HUD's edge panels move toward the middle as far as
the player ship leaves room (`hud_shift`), and the ship view keeps to the columns the HUD
covers: its lowest level shows the whole HUD in place of the whole frame, and so do event
dialogs, which keep within those columns. The panels stay in place during the tutorial.

| Input, zoomed | Effect |
| --- | --- |
| D-pad or flick toward no zone | move the view half its size that way, until the cursor moves |
| hold Start | the lowest level until release: the whole frame, or the whole HUD on a narrower screen |

The view rests on the frame's edges, or the HUD's, while the cursor stays clear of the
view's edges.

## Text size

Text sizes apply to event boxes and tooltips: 1 to 2 times the game's own, or auto. Auto
enlarges them on screens smaller than 1280x720, less at higher zoom levels. The help
and notices take the same size, as far as the frame holds them.

On a screen that shows the frame smaller than 1280x720, the pointer is drawn larger, up to
`pointer_scale_max` times its size, the crew markers up to one and a half times, and the
zone brackets keep their size.
