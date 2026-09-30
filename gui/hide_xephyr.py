#!/usr/bin/env python3
"""Unmaps the Xephyr host window on the physical display.

The E2E harness runs Xephyr nested inside the user's desktop session; its
host window shows up as a plain black window that the user may close. This
helper waits for the window to appear and unmaps it so the nested server
keeps running while staying invisible on the physical display.
"""

import sys
import time

from Xlib import display


def wm_name(window):
    try:
        name = window.get_wm_name()
    except Exception:
        name = None
    if name:
        return name
    try:
        cls = window.get_wm_class()
    except Exception:
        cls = None
    return cls[1] if cls else None


def main():
    target = sys.argv[1] if len(sys.argv) > 1 else ":0"
    deadline = time.time() + 10
    d = display.Display(target)
    while time.time() < deadline:
        root = d.screen().root
        for window in root.query_tree().children:
            name = wm_name(window)
            if name and "Xephyr" in name:
                window.unmap()
                d.sync()
                print(f"unmapped host window {name!r}")
                return
        time.sleep(0.25)
    print("xephyr host window not found", file=sys.stderr)
    sys.exit(1)


if __name__ == "__main__":
    main()
