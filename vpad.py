#!/usr/bin/env python3
"""Virtual gamepad for the desktop probe run.

Creates a uinput pad with the evdev layout the engine's scanner expects, waits for the
game to initialise, then plays a fixed sequence of buttons, D-pad and stick moves twice.

    ./vpad.py [delay-seconds]
"""
import sys
import time

from evdev import AbsInfo, UInput, ecodes as e

STICK = AbsInfo(0, -32768, 32767, 16, 128, 0)
HAT = AbsInfo(0, -1, 1, 0, 0, 0)
BUTTONS = [e.BTN_SOUTH, e.BTN_EAST, e.BTN_NORTH, e.BTN_WEST, e.BTN_TL, e.BTN_TR, e.BTN_TL2,
           e.BTN_TR2, e.BTN_SELECT, e.BTN_START, e.BTN_MODE, e.BTN_THUMBL, e.BTN_THUMBR,
           e.BTN_DPAD_UP, e.BTN_DPAD_DOWN, e.BTN_DPAD_LEFT, e.BTN_DPAD_RIGHT]
CAPS = {
    e.EV_KEY: BUTTONS,
    e.EV_ABS: [(e.ABS_X, STICK), (e.ABS_Y, STICK), (e.ABS_RX, STICK), (e.ABS_RY, STICK),
               (e.ABS_HAT0X, HAT), (e.ABS_HAT0Y, HAT)],
}
STEP = 0.3


def hold(ui, axis, value, seconds):
    ui.write(e.EV_ABS, axis, value); ui.syn(); time.sleep(seconds)
    ui.write(e.EV_ABS, axis, 0); ui.syn(); time.sleep(STEP)


def tap(ui, button, seconds=STEP):
    ui.write(e.EV_KEY, button, 1); ui.syn(); time.sleep(seconds)
    ui.write(e.EV_KEY, button, 0); ui.syn(); time.sleep(STEP)


def cursor_sequence(ui):
    """Right stick moves, R2 clicks, L1 + D-pad left, left stick left."""
    print("vpad: cursor sequence", flush=True)
    hold(ui, e.ABS_RX, 32767, 1.0)
    hold(ui, e.ABS_RY, 32767, 0.5)
    hold(ui, e.ABS_RX, -16000, 1.0)
    tap(ui, e.BTN_TR2)
    tap(ui, e.BTN_SOUTH)
    ui.write(e.EV_KEY, e.BTN_TL, 1); ui.syn(); time.sleep(STEP)
    tap(ui, e.BTN_DPAD_LEFT)
    ui.write(e.EV_KEY, e.BTN_TL, 0); ui.syn(); time.sleep(STEP)
    hold(ui, e.ABS_X, -32768, 0.6)
    tap(ui, e.BTN_SELECT)
    print("vpad: done", flush=True)
    time.sleep(1)


def main():
    delay = float(sys.argv[1]) if len(sys.argv) > 1 else 10.0
    mode = sys.argv[2] if len(sys.argv) > 2 else "probe"
    ui = UInput(CAPS, name="ftlpad virtual pad", vendor=0x045e, product=0x028e, version=0x110)
    print(f"vpad: {ui.device.path} up, waiting {delay}s", flush=True)
    time.sleep(delay)
    if mode == "cursor":
        cursor_sequence(ui)
        ui.close()
        return
    if mode == "pages":
        def down(b): ui.write(e.EV_KEY, b, 1); ui.syn(); time.sleep(STEP)
        def up(b): ui.write(e.EV_KEY, b, 0); ui.syn(); time.sleep(STEP)
        tap(ui, e.BTN_DPAD_LEFT)                     # weapon 1
        down(e.BTN_TL); tap(ui, e.BTN_DPAD_LEFT); up(e.BTN_TL)      # depower weapon 1
        down(e.BTN_TR); tap(ui, e.BTN_SOUTH); up(e.BTN_TR)          # second: t
        down(e.BTN_TL); down(e.BTN_TR); tap(ui, e.BTN_SOUTH); up(e.BTN_TL); up(e.BTN_TR)  # crew: f12
        down(e.BTN_TR); hold(ui, e.ABS_RX, -32768, 0.5); up(e.BTN_TR)   # second: right stick left = f1
        hold(ui, e.ABS_RX, 32767, 0.5)                                 # base: cursor
        down(e.BTN_TL); hold(ui, e.ABS_X, -32768, 0.5); up(e.BTN_TL)   # depower: left stick = a shift
        tap(ui, e.BTN_TR2)                                             # left click
        print("vpad: done", flush=True)
        time.sleep(1)
        ui.close()
        return
    if mode == "wheel":
        ui.write(e.EV_KEY, e.BTN_NORTH, 1); ui.syn(); time.sleep(0.4)
        ui.write(e.EV_ABS, e.ABS_X, 23000); ui.write(e.EV_ABS, e.ABS_Y, -23000); ui.syn()
        time.sleep(4.0)
        ui.write(e.EV_KEY, e.BTN_NORTH, 0); ui.syn(); time.sleep(0.3)
        ui.write(e.EV_ABS, e.ABS_X, 0); ui.write(e.EV_ABS, e.ABS_Y, 0); ui.syn(); time.sleep(0.3)
        print("vpad: done", flush=True)
        ui.close()
        return
    if mode == "scheme1":
        # Hold L1 past the tap threshold, point the left stick up, A, then let go.
        ui.write(e.EV_KEY, e.BTN_TL, 1); ui.syn(); time.sleep(0.6)
        ui.write(e.EV_ABS, e.ABS_X, 0); ui.write(e.EV_ABS, e.ABS_Y, -30000); ui.syn(); time.sleep(0.4)
        ui.write(e.EV_ABS, e.ABS_X, 0); ui.write(e.EV_ABS, e.ABS_Y, 0); ui.syn(); time.sleep(2.5)
        tap(ui, e.BTN_SOUTH, 0.15)
        time.sleep(1.0)
        ui.write(e.EV_KEY, e.BTN_TL, 0); ui.syn(); time.sleep(0.5)
        tap(ui, e.BTN_TL, 0.08)           # tap: arm next weapon
        time.sleep(0.8)
        tap(ui, e.BTN_EAST, 0.1)          # cancel aim
        time.sleep(0.5)
        tap(ui, e.BTN_NORTH, 0.1)         # pause
        time.sleep(0.8)
        ui.write(e.EV_KEY, e.BTN_NORTH, 1); ui.syn(); time.sleep(0.8)   # hold: fast
        ui.write(e.EV_KEY, e.BTN_NORTH, 0); ui.syn(); time.sleep(0.5)
        print("vpad: done", flush=True)
        ui.close()
        return
    if mode == "scheme2":
        # Systems wheel on L2 with a screenshot pause, B depowers, then crew wheel on R1.
        ui.write(e.EV_KEY, e.BTN_TL2, 1); ui.syn(); time.sleep(0.6)
        ui.write(e.EV_ABS, e.ABS_RX, 30000); ui.write(e.EV_ABS, e.ABS_RY, 0); ui.syn(); time.sleep(0.4)
        ui.write(e.EV_ABS, e.ABS_RX, 0); ui.syn(); time.sleep(2.5)
        tap(ui, e.BTN_EAST, 0.15)
        time.sleep(0.5)
        ui.write(e.EV_KEY, e.BTN_TL2, 0); ui.syn(); time.sleep(0.5)
        ui.write(e.EV_KEY, e.BTN_TR, 1); ui.syn(); time.sleep(0.6)
        ui.write(e.EV_ABS, e.ABS_Y, -30000); ui.syn(); time.sleep(0.4)
        ui.write(e.EV_ABS, e.ABS_Y, 0); ui.syn(); time.sleep(2.5)
        tap(ui, e.BTN_SOUTH, 0.15)
        time.sleep(0.5)
        ui.write(e.EV_KEY, e.BTN_TR, 0); ui.syn(); time.sleep(0.5)
        print("vpad: done", flush=True)
        ui.close()
        return
    if mode == "doors":
        tap(ui, e.BTN_EAST, 0.1); time.sleep(0.5)
        tap(ui, e.BTN_THUMBL, 0.1); time.sleep(3.5)
        tap(ui, e.BTN_DPAD_RIGHT, 0.1); time.sleep(1.5)
        tap(ui, e.BTN_THUMBL, 0.1); time.sleep(0.3)
        print("vpad: done", flush=True)
        ui.close()
        return
    if mode == "tabs":
        tap(ui, e.BTN_TR2, 0.08); time.sleep(1.5)
        tap(ui, e.BTN_TR2, 0.08); time.sleep(1.5)
        tap(ui, e.BTN_EAST, 0.1); time.sleep(0.5)
        print("vpad: done", flush=True)
        ui.close()
        return
    if mode == "dpad":
        for b in [e.BTN_DPAD_RIGHT, e.BTN_DPAD_RIGHT, e.BTN_DPAD_DOWN, e.BTN_DPAD_LEFT, e.BTN_DPAD_UP]:
            tap(ui, b, 0.2)
            time.sleep(0.8)
        print("vpad: done", flush=True)
        time.sleep(1)
        ui.close()
        return
    for round_ in range(2):
        print(f"vpad: round {round_}", flush=True)
        for b in [e.BTN_SOUTH, e.BTN_EAST, e.BTN_NORTH, e.BTN_WEST, e.BTN_TL, e.BTN_TR, e.BTN_START]:
            ui.write(e.EV_KEY, b, 1); ui.syn(); time.sleep(STEP)
            ui.write(e.EV_KEY, b, 0); ui.syn(); time.sleep(STEP)
        for x, y in [(-1, 0), (0, 0), (0, 1), (0, 0)]:
            ui.write(e.EV_ABS, e.ABS_HAT0X, x); ui.write(e.EV_ABS, e.ABS_HAT0Y, y); ui.syn()
            time.sleep(STEP)
        for b in [e.BTN_DPAD_UP, e.BTN_DPAD_RIGHT]:
            ui.write(e.EV_KEY, b, 1); ui.syn(); time.sleep(STEP)
            ui.write(e.EV_KEY, b, 0); ui.syn(); time.sleep(STEP)
        for axis in (e.ABS_X, e.ABS_RY):
            for v in (32767, 0, -32768, 0):
                ui.write(e.EV_ABS, axis, v); ui.syn(); time.sleep(STEP)
    print("vpad: done", flush=True)
    time.sleep(2)
    ui.close()


if __name__ == "__main__":
    main()
