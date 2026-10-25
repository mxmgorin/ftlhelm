# ftlhelm

<div align="center">
  <a href="https://github.com/mxmgorin/ftlhelm/releases/latest"><img src="https://img.shields.io/github/v/release/mxmgorin/ftlhelm?style=flat-square&labelColor=16171a&color=3fb8a0&cacheSeconds=180&label=release" alt="Latest release"></a>
  <a href="https://github.com/mxmgorin/ftlhelm/releases"><img src="https://img.shields.io/github/downloads/mxmgorin/ftlhelm/total?style=flat-square&labelColor=16171a&color=3fb8a0&label=downloads&cacheSeconds=180" alt="Downloads"></a>
  <a href="https://github.com/mxmgorin/ftlhelm/actions/workflows/check.yml"><img src="https://img.shields.io/github/actions/workflow/status/mxmgorin/ftlhelm/check.yml?branch=main&style=flat-square&labelColor=16171a&color=3fb8a0&cacheSeconds=180&logo=githubactions&logoColor=white&label=ci" alt="CI"></a>
</div>

<p align="center">
  <a href="#install">Install</a> &middot;
  <a href="docs/CONTROLS.md">Docs</a> &middot;
  <a href="https://github.com/mxmgorin/ftlhelm/issues">Issues</a>
</p>

Controller support for [FTL: Faster Than Light](https://subsetgames.com/ftl.html) on Linux: play it with a gamepad, without changing the game files.

<p align="center"><img src="docs/images/demo.gif" alt="The D-pad walking the rooms, a crew order, door mode, the crew sent back to their stations, the weapons, crew, systems and abilities wheels, the upgrades screen with its tabs, the controls table, a jump and a volley at an enemy ship" width="800"></p>

## Features

- **Zones**: the D-pad and sticks jump between rooms, doors, event choices, beacons and buttons
- **Door mode**: hold L2 to jump between doors, tap it to toggle a room's doors
- **Wheels** for weapons, crew, systems, ship orders and abilities, in the game's own art
- **Settings wheel** on a Select tap: mouse mode, zoom, text size and the controls table; zoom and text size are kept between runs
- **Text size** in event boxes and tooltips: auto, larger on screens smaller than 1280x720, or 1 to 2 times the game's own
- **Tap and hold** actions on every button
- **Fast forward** while a button is held
- **Mouse mode**: the D-pad and sticks move the game's cursor freely
- **Editable layout** in `ftlhelm.ini`, re-read while the game runs
- **Build check**: on an FTL build other than Steam 1.6.13 or GOG 1.6.12, ftlhelm stays inactive and the game runs without it
- **Zoom** for small screens, on by default below 1280x720: the ship view magnified around the cursor; the map, ship and store screens enlarged to fit

<p align="center"><img src="docs/images/zoom.gif" alt="Zoom on a 640x480 screen: the settings wheel steps the zoom to 1.33x, briefly 2x and back to 1.33x, the view follows the cursor from room to room and onto the enemy ship, the beacon map enlarged to fit the screen, and the zoom stepped back to the whole frame" width="640"></p>

## Install

Requires the native Linux build of FTL: 1.6.13 from Steam or 1.6.12 from GOG. The Windows and macOS builds are not supported. The steps below are for Steam.

1. Download [`ftlhelm-linux-x86_64.tar.gz`](https://github.com/mxmgorin/ftlhelm/releases/latest/download/ftlhelm-linux-x86_64.tar.gz) from the [latest release](https://github.com/mxmgorin/ftlhelm/releases/latest) and unpack it anywhere, e.g. `~/ftlhelm/`.
2. In Steam, open FTL's **Properties > General > Launch Options** and enter:

       ~/ftlhelm/ftlhelm-linux-x86_64/ftlhelm.sh %command%

3. In **Properties > Controller**, set Steam Input to **Disabled** for FTL.
4. In **Properties > Compatibility**, leave **Force the use of a specific Steam Play compatibility tool** off.

See **[Linux](docs/LINUX.md)** for GOG, running without Steam and troubleshooting.

## Controls

Tap **Select** in the game and pick **Help** for the controls table. See **[Controls](docs/CONTROLS.md)**.

## Configuration

Settings and bindings are in `ftlhelm.ini`. See **[Configuration](docs/CONFIGURATION.md)**.

## Building

See **[Building](docs/BUILDING.md)**.

## How to help

If you find the project useful, here is how you can help:

- **Tell other people about it.** Sharing the project helps it reach more users.
- **Report bugs and request features** in [Issues](https://github.com/mxmgorin/ftlhelm/issues). Feedback is welcome.
- **Star the repo.** It helps the project get noticed and keeps me motivated.
- **[Buy me a coffee](https://ko-fi.com/mxmgorin)** on Ko-fi.

## Credits

- [FTL: Faster Than Light](https://subsetgames.com/ftl.html) by Subset Games

## License

Copyright (C) 2026 Maxim Gorin. ftlhelm is free software under the
[GNU General Public License v3.0 or later](LICENSE), with the additional terms in
[NOTICE](NOTICE).

ftlhelm is an unofficial mod, not affiliated with or endorsed by Subset Games.
FTL: Faster Than Light is a trademark of Subset Games.
