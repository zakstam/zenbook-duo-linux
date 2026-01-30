#!/usr/bin/env python3
"""Inject a key event via /dev/uinput so GNOME handles it natively with OSD."""

import fcntl
import os
import struct
import sys
import time

# Map human-readable key names to Linux input event key codes
KEYS = {"brightnessdown": 224, "brightnessup": 225}
key = KEYS.get(sys.argv[1])
if key is None:
    sys.exit(1)

# ioctl constants for uinput device setup
UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_DEV_SETUP = 0x405C5503
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502

# Linux input event types
EV_SYN = 0x00  # Synchronization event
EV_KEY = 0x01  # Key press/release event


def ev(typ: int, code: int, value: int) -> bytes:
    """Build a raw input_event struct with current timestamp."""
    t = time.time()
    return struct.pack("llHHi", int(t), int((t % 1) * 1e6), typ, code, value)


# Open the uinput device for writing
fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)

# Enable key events and register the specific key code
fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
fcntl.ioctl(fd, UI_SET_KEYBIT, key)

# Configure the virtual device identity (bus type 0x06 = virtual)
# struct uinput_setup { struct input_id { u16 bustype, vendor, product, version }; char name[80]; u32 ff_effects_max; }
setup = struct.pack("HHHH80sI", 0x06, 0, 0, 0, b"duo-virtual-kbd", 0)
fcntl.ioctl(fd, UI_DEV_SETUP, setup)
fcntl.ioctl(fd, UI_DEV_CREATE)
time.sleep(0.1)  # Allow time for device registration

# Simulate a full key press: key down -> sync -> key up -> sync
os.write(fd, ev(EV_KEY, key, 1))  # Key press
os.write(fd, ev(EV_SYN, 0, 0))  # Sync
os.write(fd, ev(EV_KEY, key, 0))  # Key release
os.write(fd, ev(EV_SYN, 0, 0))  # Sync

time.sleep(0.1)  # Allow time for event processing

# Clean up: destroy virtual device and close file descriptor
fcntl.ioctl(fd, UI_DEV_DESTROY)
os.close(fd)
