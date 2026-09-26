"""Activate a window whose title matches the given substring (EWMH _NET_ACTIVE_WINDOW)."""

import sys

from Xlib import X, display
from Xlib.protocol import event

target = sys.argv[1].lower()
d = display.Display()
root = d.screen().root

matches = []


def walk(win):
    try:
        children = win.query_tree().children
    except Exception:
        return
    for child in children:
        try:
            name = child.get_wm_name()
            if name and target in name.lower():
                matches.append((child, name))
        except Exception:
            pass
        walk(child)


walk(root)
if not matches:
    print(f"no window matching {target!r}")
    sys.exit(1)

win, name = matches[0]
atom = d.intern_atom('_NET_ACTIVE_WINDOW')
ev = event.ClientMessage(
    window=win, client_type=atom, data=(32, [1, X.CurrentTime, 0, 0, 0])
)
mask = X.SubstructureRedirectMask | X.SubstructureNotifyMask
root.send_event(ev, event_mask=mask)
d.sync()
print(f"activated: {name} (0x{win.id:x})")
