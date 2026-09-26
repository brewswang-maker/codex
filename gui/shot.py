"""Grab the whole root window via Xlib and save a PNG (needs $DISPLAY)."""

import sys

from PIL import Image
from Xlib import X, display

d = display.Display()
root = d.screen().root
geo = root.get_geometry()
raw = root.get_image(0, 0, geo.width, geo.height, X.ZPixmap, 0xFFFFFFFF)
img = Image.frombytes("RGB", (geo.width, geo.height), raw.data, "raw", "BGRX")
out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/shot.png"
img.save(out)
print(f"saved {out} ({geo.width}x{geo.height})")
