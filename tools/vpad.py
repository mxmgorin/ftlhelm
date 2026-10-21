#!/usr/bin/env python3
"""Virtual gamepad for desktop test runs of ftlpad.

Creates a uinput pad with the evdev layout the SIL engine's scanner picks up, waits for
the game, then plays steps given on the command line or a named script:

    tools/vpad.py [--delay S] [--fifo PATH] STEP...
    tools/vpad.py [--delay S] [--fifo PATH] --script NAME

Steps:
    tap:BUTTON[:S]        press and release, held S seconds (default 0.1)
    hold:BUTTON:S         press, hold S seconds, release
    down:BUTTON, up:BUTTON
    stick:l|r:X,Y:S       deflect a stick (-1..1 per axis) for S seconds, then centre it
    wait:S
    cmd:TEXT              write TEXT to the ftlpad command FIFO (underscores become spaces)

Buttons are evdev positions: south, east, west, north (a, b, x, y on an Xbox pad), l1,
r1, l2, r2, l3, r3, select, start, guide, up, down, left, right. The game polls once a
frame, so a press must outlast a frame; a game slowed by an occluded window needs longer.
"""
import argparse
import time

from evdev import AbsInfo, UInput, ecodes as e

STICK = AbsInfo(0, -32768, 32767, 16, 128, 0)
HAT = AbsInfo(0, -1, 1, 0, 0, 0)
BUTTONS = {
    "south": e.BTN_SOUTH, "east": e.BTN_EAST, "west": e.BTN_WEST, "north": e.BTN_NORTH,
    "a": e.BTN_SOUTH, "b": e.BTN_EAST, "x": e.BTN_WEST, "y": e.BTN_NORTH,
    "l1": e.BTN_TL, "r1": e.BTN_TR, "l2": e.BTN_TL2, "r2": e.BTN_TR2,
    "l3": e.BTN_THUMBL, "r3": e.BTN_THUMBR, "select": e.BTN_SELECT, "start": e.BTN_START,
    "guide": e.BTN_MODE, "up": e.BTN_DPAD_UP, "down": e.BTN_DPAD_DOWN,
    "left": e.BTN_DPAD_LEFT, "right": e.BTN_DPAD_RIGHT,
}
CAPS = {
    e.EV_KEY: sorted(set(BUTTONS.values())),
    e.EV_ABS: [(e.ABS_X, STICK), (e.ABS_Y, STICK), (e.ABS_RX, STICK), (e.ABS_RY, STICK),
               (e.ABS_HAT0X, HAT), (e.ABS_HAT0Y, HAT)],
}
STICK_AXES = {"l": (e.ABS_X, e.ABS_Y), "r": (e.ABS_RX, e.ABS_RY)}
STICK_MAX = 32767
TAP_S = 0.1
# Pause after every step, so the game sees each one as its own event.
GAP_S = 0.4

SCRIPTS = {
    # Every button once, both sticks.
    "probe": ["tap:" + b for b in ("south", "east", "west", "north", "l1", "r1", "l2", "r2",
                                    "l3", "r3", "select", "start", "up", "down", "left", "right")]
             + ["stick:l:1,0:0.5", "stick:l:0,1:0.5", "stick:r:-1,0:0.5", "stick:r:0,-1:0.5"],
    # D-pad walk with a zone dump after each step.
    "dpad": [s for d in ("right", "right", "down", "left", "up") for s in ("tap:" + d + ":0.3", "cmd:dump")],
    # Fast forward on R2, with the speed state before, during and after.
    "fast": ["cmd:state", "down:r2", "wait:1", "cmd:state", "up:r2", "cmd:state"],
    # Systems wheel on L1: open, take a pip of power from the top plate, give it back, close.
    "power": ["hold:l1:0.6", "tap:down:0.3", "cmd:state", "tap:up:0.3", "tap:l1"],
}


def run(ui, step, fifo):
    """Performs one step; raises ValueError for one it does not know."""
    kind, _, arg = step.partition(":")
    if kind in ("tap", "hold"):
        name, _, secs = arg.partition(":")
        ui.write(e.EV_KEY, BUTTONS[name], 1); ui.syn()
        time.sleep(float(secs) if secs else TAP_S)
        ui.write(e.EV_KEY, BUTTONS[name], 0); ui.syn()
    elif kind in ("down", "up"):
        ui.write(e.EV_KEY, BUTTONS[arg], 1 if kind == "down" else 0); ui.syn()
    elif kind == "stick":
        side, xy, secs = arg.split(":")
        x, y = (float(v) for v in xy.split(","))
        ax, ay = STICK_AXES[side]
        ui.write(e.EV_ABS, ax, int(x * STICK_MAX)); ui.write(e.EV_ABS, ay, int(y * STICK_MAX)); ui.syn()
        time.sleep(float(secs))
        ui.write(e.EV_ABS, ax, 0); ui.write(e.EV_ABS, ay, 0); ui.syn()
    elif kind == "wait":
        time.sleep(float(arg))
    elif kind == "cmd":
        if not fifo:
            raise ValueError("cmd needs --fifo")
        with open(fifo, "w") as f:
            f.write(arg.replace("_", " ") + "\n")
    else:
        raise ValueError(f"unknown step {step}")
    time.sleep(GAP_S)


def main():
    parser = argparse.ArgumentParser(description="Virtual gamepad for ftlpad test runs.")
    parser.add_argument("--delay", type=float, default=4.0, help="seconds to wait before the first step")
    parser.add_argument("--fifo", help="the ftlpad command FIFO, for cmd: steps")
    parser.add_argument("--script", choices=sorted(SCRIPTS), help="a named step list")
    parser.add_argument("steps", nargs="*")
    args = parser.parse_args()
    steps = SCRIPTS[args.script] + args.steps if args.script else args.steps
    ui = UInput(CAPS, name="ftlpad virtual pad", vendor=0x045e, product=0x028e, version=0x110)
    print(f"vpad: {ui.device.path} up, waiting {args.delay}s", flush=True)
    time.sleep(args.delay)
    try:
        for step in steps:
            print(f"vpad: {step}", flush=True)
            run(ui, step, args.fifo)
    finally:
        ui.close()


if __name__ == "__main__":
    main()
