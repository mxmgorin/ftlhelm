# ftlpad

<div align="center">
  <a href="https://github.com/mxmgorin/ftlpad/releases/latest"><img src="https://img.shields.io/github/v/release/mxmgorin/ftlpad?style=flat-square&labelColor=16171a&color=3fb8a0&cacheSeconds=180&label=release" alt="Latest release"></a>
  <a href="https://github.com/mxmgorin/ftlpad/actions/workflows/check.yml"><img src="https://img.shields.io/github/actions/workflow/status/mxmgorin/ftlpad/check.yml?branch=main&style=flat-square&labelColor=16171a&color=3fb8a0&cacheSeconds=180&logo=githubactions&logoColor=white&label=ci" alt="CI"></a>
</div>

<p align="center">
  <a href="#install">Install</a> &middot;
  <a href="docs/CONTROLS.md">Docs</a> &middot;
  <a href="https://github.com/mxmgorin/ftlpad/issues">Issues</a>
</p>

Play [FTL: Faster Than Light](https://subsetgames.com/ftl.html) with a gamepad. FTL has no controller support; ftlpad adds it without changing the game files.

<p align="center"><img src="docs/images/demo.gif" alt="The D-pad walking the rooms, a crew order, door mode, the crew sent back to their stations, the weapons, crew, systems and abilities wheels, the upgrades screen with its tabs, the controls table, a jump and a volley at an enemy ship" width="800"></p>

## Features

- **Zones**: the D-pad and sticks jump between rooms, doors, event choices, beacons and buttons
- **Door mode**: hold L2 to jump between doors, tap it to toggle a room's doors
- **Wheels** for weapons, crew, systems, ship orders and abilities, in the game's own art
- **Tap and hold** actions on every button
- **Fast forward** while a button is held
- **Help**: hold Start for the controls table
- **Editable layout** in `ftlpad.ini`, re-read while the game runs
- **Free cursor mode**: the D-pad and sticks move the game's cursor

## Install

Requires the Steam build of FTL 1.6.13.

1. Download [`ftlpad-linux-x86_64.tar.gz`](https://github.com/mxmgorin/ftlpad/releases/latest/download/ftlpad-linux-x86_64.tar.gz) from the [latest release](https://github.com/mxmgorin/ftlpad/releases/latest) and unpack it anywhere, e.g. `~/ftlpad/`.
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

## License

Copyright (C) 2026 Maxim Gorin. ftlpad is free software under the
[GNU General Public License v3.0 or later](LICENSE), with the additional terms in
[NOTICE](NOTICE).

ftlpad is an unofficial mod, not affiliated with or endorsed by Subset Games.
FTL: Faster Than Light is a trademark of Subset Games.
