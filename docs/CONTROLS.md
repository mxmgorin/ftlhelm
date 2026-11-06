# Controls

The default buttons are in the [README](../README.md#controls); in the game, the Help plate of
the settings wheel shows them. Bindings and settings: [Configuration](CONFIGURATION.md).

FTLHELM uses one pad at a time: the one that pressed a button last. When it disconnects, the
game pauses.

## Help

Opens from the Help plate of the settings wheel, and once by itself on the first run. Pages:
Buttons, Wheels, Weapons, Crew, Doors.

| Input | In the help |
| --- | --- |
| L1, R1 or D-pad left, right | previous, next page |
| any other button | close |

## Keyboard

Opens while a name is being changed: the ship's or a crew member's in the hangar, a crew
member's on the Crew tab. Stays closed while the mouse is in use.

| Input | On the keyboard |
| --- | --- |
| D-pad, sticks | move between keys, and up to the name; held, repeat |
| left, right on the name | move the caret |
| A, or a tap on a key | press the key |
| X, L1 | erase; held, repeats |
| Y, R1 | space |
| Start | done |
| B | cancel, keeping the old name |

A word's first letter is a capital. Shift changes the case of the next letter; pressed twice,
it locks capitals.

## Zones

The D-pad and the sticks move the cursor to the next zone that way: a room, a weapon or drone
box, a button, an event choice, a beacon, a store slot, a menu item. A held direction repeats.

| Input | Effect |
| --- | --- |
| L3, R3 | the other ship |
| Y | select the next crew member |
| A on a room | send the selected crew there; with none selected, select the room's crew one at a time, then all |
| B | clear the selection |

## Doors

| Input | Effect |
| --- | --- |
| L2 held | doors become the zones; A opens or closes the door under the cursor |
| L2 tap | open or close all doors of the room |

## Weapons

| Input | Effect |
| --- | --- |
| X | arm the next powered weapon |
| A on a room | target it with the armed weapon |
| A on a weapon or drone box | click it |
| A held, D-pad left, right on a weapon or drone box | move it one box |
| B | cancel the aim |

With a beam armed:

| Input | Effect |
| --- | --- |
| A on a room | lay the beam through the room, over the most rooms |
| D-pad left, right | turn the line |
| D-pad down, up | slide the line towards the aim cursor, away from it |
| A held, D-pad | move the line any way |
| A | fire on release |
| B | take the line back |

Volleys, on the weapons wheel:

- **All weapons** plate: arms every powered weapon; A on a room aims them all.
- D-pad up or R2 on a powered weapon picks it for a volley, D-pad down or L2 drops it.

Autofire: the game's button under the weapons, or the plate on the ship wheel.

## Wheels

Holding B, X, Y, L1 or R1 opens a wheel on the ship view; Select opens the settings wheel on any
screen. A wheel stays open after the button is released, and the game pauses while it is open.

| Input | In a wheel |
| --- | --- |
| either stick | point at a plate |
| D-pad left, right | previous, next plate |
| D-pad up, down; R2, L2 | add, take power; pick, drop a weapon; add, drop crew; step a setting |
| A | act on the plate |
| A held, D-pad left, right | move a weapon or drone one slot |
| B | close, or back to the previous wheel |
| the wheel's own button | close |
| another wheel's button, held | switch to that wheel |

Lockdown, on the abilities wheel: locks the room of the selected charged crystal, or of the only
charged crystal when none is selected; its pips show the charge.

## Settings wheel

A steps a plate to its next value; D-pad up, down or R2, L2 step it up or down.

| Plate | Sets |
| --- | --- |
| Mouse | zones or mouse mode |
| Zoom | [zoom](#zoom) level |
| Autopause | when the game pauses itself: Wheels, Aim, Both, Off |
| Fast speed | fast forward speed |
| Help | opens the help |
| Controls | opens the [controls wheel](#controls-wheel) |
| View | opens the [view wheel](#view-wheel) |
| Text | [text size](#text-size) |

Mouse mode resets each run; the other values are kept.

## View wheel

A plate is greyed out where it does nothing.

| Plate | Switches |
| --- | --- |
| Badge | the badge on the PAUSED row |
| Crew marker | markers over the selected crew |
| HUD shift | HUD panels moved toward the middle on narrow screens |
| iPad beacons | map beacons in the iPad art |
| iPad buttons | crew station buttons in the iPad art |
| Pointer size | pointer size against the game's own |
| Frame filter | smoother picture on small screens, sharper at higher levels |
| Focus brackets | brackets on the zone under the cursor |

## Controls wheel

| Plate | Sets |
| --- | --- |
| Repeat | repeat speed of a held direction |
| Stick | stick sensitivity |
| Room crew | A on a room selects its crew |
| Swap buttons | swap A/B, X/Y or both when the pad's labels do not match |
| Touch | touchscreen on or off |
| B pause | B pauses and resumes when there is nothing to cancel |
| Rumble | rumble strength |
| Hold time | how long a press lasts to count as a hold |

## Mouse mode

The Mouse plate of the settings wheel switches the D-pad and the sticks to move the cursor
freely, and back.

## Overlay

- The badge on the game's PAUSED row names the selected crew and the active modes: Mouse,
  Doors, Fast, Target.
- Brackets mark the zone under the cursor.
- Green markers show over the selected crew.

## Touch

A touchscreen acts as the mouse. A touch next to a target snaps to it.

| Touch | Effect |
| --- | --- |
| tap | click |
| hold, release | right click |
| move up or down from a system icon or a weapon box | add or take power |
| drag | drag |
| two-finger tap | pause or resume |
| pinch on the ship view | zoom; the view moves with the fingers |

After a touch, a pause button shows in the bottom right corner of the ship view. In the help,
a tap on a tab opens its page; in a wheel, a tap on a plate acts on it; a tap elsewhere closes
either.

## Zoom

Zoom magnifies the ship view up to 3 times; the view follows the cursor. Other screens show
whole. Auto zooms one level in on screens smaller than 1280x720.

| Input, zoomed | Effect |
| --- | --- |
| D-pad or flick toward no zone | move the view |
| hold Start | zoom out until release |

## Text size

Event boxes and tooltips take 1 to 2 times the game's text size. Auto enlarges them on
screens smaller than 1280x720.
