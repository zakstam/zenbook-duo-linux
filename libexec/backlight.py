#!/usr/bin/env python3

import sys

import usb.core
import usb.util

REPORT_ID = 0x5A
WVALUE = 0x035A
WINDEX = 4
WLENGTH = 16


def usage() -> None:
    print(f"Usage: {sys.argv[0]} <level 0-3> <vendor_id_hex> <product_id_hex>")
    sys.exit(1)


def parse_hex(s: str) -> int:
    s = s.strip().lower()
    if s.startswith("0x"):
        s = s[2:]
    return int(s, 16)


if len(sys.argv) != 4:
    usage()

try:
    level = int(sys.argv[1])
    if level < 0 or level > 3:
        raise ValueError
except ValueError:
    print("Invalid level. Must be an integer between 0 and 3.")
    sys.exit(1)

try:
    vendor_id = parse_hex(sys.argv[2])
    product_id = parse_hex(sys.argv[3])
except ValueError:
    print("Invalid vendor/product. Expected hex like 0B05 1B2C (or 0x0B05 0x1B2C).")
    sys.exit(1)

data = [0] * WLENGTH
data[0] = REPORT_ID
data[1] = 0xBA
data[2] = 0xC5
data[3] = 0xC4
data[4] = level

dev = usb.core.find(idVendor=vendor_id, idProduct=product_id)
if dev is None:
    print(
        f"Device not found (Vendor ID: 0x{vendor_id:04X}, Product ID: 0x{product_id:04X})"
    )
    sys.exit(1)

detached = False
try:
    if dev.is_kernel_driver_active(WINDEX):
        dev.detach_kernel_driver(WINDEX)
        detached = True

    bmRequestType = 0x21  # Host to Device | Class | Interface
    bRequest = 0x09  # SET_REPORT
    ret = dev.ctrl_transfer(
        bmRequestType, bRequest, WVALUE, WINDEX, data, timeout=1000
    )
    if ret != WLENGTH:
        print(f"Warning: Only {ret} bytes sent out of {WLENGTH}.")
except usb.core.USBError as e:
    print(f"Control transfer failed: {e}")
    sys.exit(1)
finally:
    try:
        usb.util.release_interface(dev, WINDEX)
    except Exception:
        pass
    if detached:
        try:
            dev.attach_kernel_driver(WINDEX)
        except Exception:
            pass

sys.exit(0)
