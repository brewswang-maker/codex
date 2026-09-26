"""XTEST single click at (x, y) on the current $DISPLAY."""

import sys
import time

from Xlib import X, display
from Xlib.ext import xtest

d = display.Display()
x, y = int(sys.argv[1]), int(sys.argv[2])
xtest.fake_input(d, X.MotionNotify, x=x, y=y)
d.sync()
time.sleep(0.3)
xtest.fake_input(d, X.ButtonPress, 1)
d.sync()
time.sleep(0.08)
xtest.fake_input(d, X.ButtonRelease, 1)
d.sync()
time.sleep(0.3)
print(f"clicked ({x}, {y})")
