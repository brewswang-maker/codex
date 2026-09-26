import sys
import time

from Xlib import X, display
from Xlib.ext import xtest

d = display.Display()
x, y, clicks = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])
direction = int(sys.argv[4]) if len(sys.argv) > 4 else 5  # 5=down, 4=up

xtest.fake_input(d, X.MotionNotify, x=x, y=y)
d.sync()
time.sleep(0.2)
for _ in range(clicks):
    xtest.fake_input(d, X.ButtonPress, direction)
    d.sync()
    time.sleep(0.04)
    xtest.fake_input(d, X.ButtonRelease, direction)
    d.sync()
    time.sleep(0.08)
print(f"scrolled {clicks} clicks (button {direction}) at ({x},{y})")
