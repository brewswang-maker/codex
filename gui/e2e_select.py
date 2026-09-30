#!/usr/bin/env python3
"""E2E driver for the GUI text-selection/copy feature.

Drives pointer and keyboard through XTEST on a dedicated display (Xephyr)
and reads the CLIPBOARD selection so copy results can be asserted.

Subcommands:
  shot FILE                     screenshot the whole root window
  click X Y                     move pointer and left-click
  drag X1 Y1 X2 Y2              press, move in steps, release (text drag)
  type TEXT                     type a plain-ASCII string
  key MODS:NAME                 send a chord, e.g. "ctrl+c", "Escape"
  clipboard                     print the CLIPBOARD selection as UTF-8
"""

import select
import sys
import time

from Xlib import X, XK, display
from Xlib.ext import xtest

SHIFTED = {
    "!": "exclam",
    "@": "at",
    "#": "numbersign",
    "$": "dollar",
    "%": "percent",
    "^": "asciicircum",
    "&": "ampersand",
    "*": "asterisk",
    "(": "parenleft",
    ")": "parenright",
    "_": "underscore",
    "+": "plus",
    "{": "braceleft",
    "}": "braceright",
    ":": "colon",
    '"': "quotedbl",
    "~": "asciitilde",
    "|": "bar",
    "<": "less",
    ">": "greater",
    "?": "question",
}

NAMED = {" ": "space", "\n": "Return", "\t": "Tab"}

MODS = {"ctrl": "Control_L", "alt": "Alt_L", "shift": "Shift_L", "super": "Super_L"}


def conn():
    return display.Display()


def move(d, x, y):
    xtest.fake_input(d, X.MotionNotify, x=x, y=y)
    d.sync()


def do_shot(path):
    from PIL import Image

    d = conn()
    root = d.screen().root
    geom = root.get_geometry()
    raw = root.get_image(0, 0, geom.width, geom.height, X.ZPixmap, 0xFFFFFFFF)
    img = Image.frombytes("RGB", (geom.width, geom.height), raw.data, "raw", "BGRX")
    img.save(path)
    print(f"shot saved: {path} {img.size}")


def do_click(x, y, button=1):
    d = conn()
    move(d, x, y)
    time.sleep(0.08)
    xtest.fake_input(d, X.ButtonPress, button)
    d.sync()
    time.sleep(0.08)
    xtest.fake_input(d, X.ButtonRelease, button)
    d.sync()
    time.sleep(0.12)


def do_drag(x1, y1, x2, y2, steps=30):
    d = conn()
    move(d, x1, y1)
    time.sleep(0.15)
    xtest.fake_input(d, X.ButtonPress, 1)
    d.sync()
    time.sleep(0.15)
    for i in range(1, steps + 1):
        x = x1 + (x2 - x1) * i // steps
        y = y1 + (y2 - y1) * i // steps
        move(d, x, y)
        time.sleep(0.02)
    time.sleep(0.15)
    xtest.fake_input(d, X.ButtonRelease, 1)
    d.sync()
    time.sleep(0.2)


def do_type(text):
    d = conn()
    shift_code = d.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
    for ch in text:
        shift = False
        if ch in NAMED:
            name = NAMED[ch]
        elif ch in SHIFTED:
            name = SHIFTED[ch]
            shift = True
        elif ch.isupper():
            name = ch.lower()
            shift = True
        else:
            name = ch
        code = d.keysym_to_keycode(XK.string_to_keysym(name))
        if not code:
            print(f"skip unmappable char {ch!r}", file=sys.stderr)
            continue
        if shift:
            xtest.fake_input(d, X.KeyPress, shift_code)
        xtest.fake_input(d, X.KeyPress, code)
        xtest.fake_input(d, X.KeyRelease, code)
        if shift:
            xtest.fake_input(d, X.KeyRelease, shift_code)
        d.sync()
        time.sleep(0.012)
    time.sleep(0.1)


def do_key(spec):
    d = conn()
    sep = ":" if ":" in spec else "+"
    parts = spec.split(sep)
    if len(parts) == 1:
        mods, name = [], parts[0]
    else:
        mods, name = parts[:-1], parts[-1]
    mod_codes = [d.keysym_to_keycode(XK.string_to_keysym(MODS[m])) for m in mods]
    code = d.keysym_to_keycode(XK.string_to_keysym(name))
    if not code:
        print(f"unmappable key {name!r}", file=sys.stderr)
        sys.exit(1)
    for c in mod_codes:
        xtest.fake_input(d, X.KeyPress, c)
    xtest.fake_input(d, X.KeyPress, code)
    xtest.fake_input(d, X.KeyRelease, code)
    for c in reversed(mod_codes):
        xtest.fake_input(d, X.KeyRelease, c)
    d.sync()
    time.sleep(0.2)


def do_clipboard():
    d = conn()
    screen = d.screen()
    win = screen.root.create_window(
        0, 0, 1, 1, 0, screen.root_depth, X.InputOutput, X.CopyFromParent
    )
    clipboard_atom = d.intern_atom("CLIPBOARD")
    utf8_atom = d.intern_atom("UTF8_STRING")
    prop_atom = d.intern_atom("E2E_CLIP")
    win.convert_selection(clipboard_atom, utf8_atom, prop_atom, X.CurrentTime)
    d.sync()

    deadline = time.time() + 4.0
    result = ""
    while time.time() < deadline:
        if not d.pending_events():
            if not select.select([d.fileno()], [], [], 0.1)[0]:
                continue
        e = d.next_event()
        if e.type == X.SelectionNotify:
            if e.property == X.NONE:
                result = ""
            else:
                data = win.get_full_property(e.property, X.AnyPropertyType)
                if data is not None and data.value is not None:
                    raw = data.value
                    result = raw.decode("utf-8", "replace") if isinstance(raw, bytes) else str(raw)
            break
    win.destroy()
    d.sync()
    print(result)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    cmd = sys.argv[1]
    args = sys.argv[2:]
    if cmd == "shot":
        do_shot(args[0])
    elif cmd == "click":
        do_click(int(args[0]), int(args[1]))
    elif cmd == "drag":
        do_drag(int(args[0]), int(args[1]), int(args[2]), int(args[3]))
    elif cmd == "type":
        do_type(args[0])
    elif cmd == "key":
        do_key(args[0])
    elif cmd == "clipboard":
        do_clipboard()
    else:
        print(f"unknown command {cmd}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
