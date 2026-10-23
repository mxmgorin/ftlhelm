# ftlpad

Play [FTL: Faster Than Light](https://subsetgames.com/ftl.html) on Linux with a gamepad. FTL has no controller support; ftlpad adds it without changing the game files.

Works with the Steam build of FTL 1.6.13.

## Features

- **Zones**: the D-pad and sticks jump between rooms, doors, event choices, beacons and buttons
- **Free cursor mode**: the D-pad and sticks move the game's cursor
- **Wheels** for weapons, crew, systems and abilities, in the game's own art
- **Tap and hold** actions on every button
- **Fast forward** while a button is held
- **Controls table** in the game
- **Editable layout** in gptokeyb2's ini format, re-read while the game runs

## Install

1. Download [`ftlpad-linux-x86_64.tar.gz`](../../releases/latest/download/ftlpad-linux-x86_64.tar.gz) from the [latest release](../../releases/latest) and unpack it anywhere, e.g. `~/ftlpad/`.
2. In Steam, open FTL's **Properties > General > Launch Options** and enter:

       ~/ftlpad/ftlpad-linux-x86_64/ftlpad.sh %command%

3. In **Properties > Controller**, set Steam Input to **Disabled** for FTL.

See **[Linux](docs/LINUX.md)** for running without Steam and troubleshooting.

## Controls

Hold **Start** in the game for the controls table. See **[Controls](docs/CONTROLS.md)**.

## Configuration

Settings and bindings are in `ftlpad.ini`. See **[Configuration](docs/CONFIGURATION.md)**.

## Building

See **[Building](docs/BUILDING.md)**.

## Credits

- [FTL: Faster Than Light](https://subsetgames.com/ftl.html) by Subset Games
- The layout format follows [gptokeyb2](https://github.com/PortsMaster/gptokeyb2)
