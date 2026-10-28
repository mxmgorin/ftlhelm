#!/usr/bin/env python3
"""Prints HULL_MARGINS for src/game/hud.rs: the clear columns left and right of each player
hull image in an FTL ftl.dat, by image width. Needs Pillow.

    tools/hull_margins.py "/path/to/FTL Faster Than Light/data/ftl.dat"

A column is clear when no pixel in it reaches ALPHA. Images of one width must agree, since
the layer tells them apart by width alone.
"""

import io
import re
import struct
import sys

from PIL import Image

# The glow every hull image fades out with stays below this alpha.
ALPHA = 64
HULL = re.compile(r"img/ship/[a-z0-9_]+_base\.png")


def entries(path):
    """Yields the name and bytes of each file in an FTL PKG archive."""
    with open(path, "rb") as f:
        data = f.read()
    head, entry_size, count, _ = struct.unpack(">HHII", data[4:16])
    table = data[head:]
    names = table[entry_size * count :]
    for i in range(count):
        _, name_at, at, size, _ = struct.unpack_from(">IIIII", table, entry_size * i)
        yield names[name_at : names.index(0, name_at)].decode(), data[at : at + size]


def clear_columns(png):
    """The clear columns on the left and on the right of an image; its width."""
    alpha = Image.open(io.BytesIO(png)).convert("RGBA").getchannel("A")
    w, h = alpha.size
    px = alpha.load()
    solid = [x for x in range(w) if any(px[x, y] >= ALPHA for y in range(h))]
    return w, solid[0], w - 1 - solid[-1]


def main():
    margins = {}
    for name, png in entries(sys.argv[1]):
        if not HULL.fullmatch(name):
            continue
        w, left, right = clear_columns(png)
        if margins.setdefault(w, (left, right)) != (left, right):
            sys.exit(f"{name}: width {w} already has margins {margins[w]}")
    for w, (left, right) in sorted(margins.items()):
        print(f"    ({w}, {left}, {right}),")


if __name__ == "__main__":
    main()
