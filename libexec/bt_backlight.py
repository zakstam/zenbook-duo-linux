#!/usr/bin/env python3
"""Send backlight feature report via hidraw (Bluetooth HID)."""

import fcntl
import os
import sys


def HIDIOCSFEATURE(length: int) -> int:
    """Build the HIDIOCSFEATURE ioctl number for the given data length."""
    return (3 << 30) | (length << 16) | (0x48 << 8) | 0x06


WLENGTH = 16  # Feature report packet length
level = int(sys.argv[1])  # Backlight level (0-3)
hidraw = sys.argv[2]  # Path to hidraw device (e.g., /dev/hidraw0)

# Build the feature report data packet (same format as USB control transfer)
data = bytearray(WLENGTH)
data[0] = 0x5A  # Report ID
data[1] = 0xBA  # Command bytes
data[2] = 0xC5
data[3] = 0xC4
data[4] = level  # Brightness level

# Send the feature report via ioctl on the hidraw device
fd = os.open(hidraw, os.O_RDWR)
fcntl.ioctl(fd, HIDIOCSFEATURE(WLENGTH), bytes(data))
os.close(fd)
