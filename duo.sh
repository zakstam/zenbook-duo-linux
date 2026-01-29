#!/bin/bash
# Main runtime script for ASUS Zenbook Duo Linux dual-screen management.
# Handles keyboard backlight, display configuration, screen rotation,
# Wi-Fi/Bluetooth state preservation, and USB hotplug events.

# ============================================================================
# HEADER & CONFIGURATION
# ============================================================================

# Default keyboard backlight level (0=off, 1-3=brightness levels)
DEFAULT_BACKLIGHT=0

# Default display scale factor (1=100%, 2=200%)
DEFAULT_SCALE=1.66

# ============================================================================
# INITIALIZATION
# ============================================================================

# Trap Ctrl+C to cleanly terminate all child processes (watchers) before exiting
trap 'echo "Ctrl+C captured. Exiting..."; pkill -P $$; exit 1' INT

# Create temp directory for runtime state files (backlight level, status, logs, scripts)
mkdir -p /tmp/duo

# Use the configured default scale (dynamic detection via gdctl is commented out)
# SCALE=$(gdctl show |grep Scale: |sed 's/│//g' |awk '{print $2}' |head -n1)
# if [ -z "${SCALE}" ]; then
#     SCALE=1
# fi
SCALE=${DEFAULT_SCALE}

# Locate the python3 interpreter for running USB/HID helper scripts
PYTHON3=$(which python3)
# Detect if the Zenbook Duo keyboard is connected via USB and extract its vendor:product ID
KEYBOARD_DEV=$(lsusb | grep 'Zenbook Duo Keyboard' |awk '{print $6}')

# ============================================================================
# EMBEDDED PYTHON SCRIPTS
# ============================================================================

# Generate the USB backlight control Python script if keyboard is connected and script doesn't exist
if [ -n "${KEYBOARD_DEV}" ] && [ ! -f /tmp/duo/backlight.py ]; then
    # Extract vendor and product IDs from the "VVVV:PPPP" format
    VENDOR_ID=${KEYBOARD_DEV%:*}
    PRODUCT_ID=${KEYBOARD_DEV#*:}
    echo "#!/usr/bin/env python3

# BSD 2-Clause License
#
# Copyright (c) 2024, Alesya Huzik
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are met:
#
# 1. Redistributions of source code must retain the above copyright notice, this
#    list of conditions and the following disclaimer.

# 2. Redistributions in binary form must reproduce the above copyright notice,
#    this list of conditions and the following disclaimer in the documentation
#    and/or other materials provided with the distribution.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\"
# AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
# IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
# DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
# FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
# DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
# SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
# CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
# OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
# OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

import sys
import usb.core
import usb.util

# USB Parameters
VENDOR_ID = 0x${VENDOR_ID}
PRODUCT_ID = 0x${PRODUCT_ID}
REPORT_ID = 0x5A
WVALUE = 0x035A
WINDEX = 4
WLENGTH = 16

if len(sys.argv) != 2:
    print(f\"Usage: {sys.argv[0]} <level>\")
    sys.exit(1)

try:
    level = int(sys.argv[1])
    if level < 0 or level > 3:
        raise ValueError
except ValueError:
    print(\"Invalid level. Must be an integer between 0 and 3.\")
    sys.exit(1)

# Prepare the data packet
data = [0] * WLENGTH
data[0] = REPORT_ID
data[1] = 0xBA
data[2] = 0xC5
data[3] = 0xC4
data[4] = level

# Find the device
dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)

if dev is None:
    print(f\"Device not found (Vendor ID: 0x{VENDOR_ID:04X}, Product ID: 0x{PRODUCT_ID:04X})\")
    sys.exit(1)

# Detach kernel driver if necessary
if dev.is_kernel_driver_active(WINDEX):
    try:
        dev.detach_kernel_driver(WINDEX)
    except usb.core.USBError as e:
        print(f\"Could not detach kernel driver: {str(e)}\")
        sys.exit(1)

# try:
#     dev.set_configuration()
#     usb.util.claim_interface(dev, WINDEX)
# except usb.core.USBError as e:
#     print(f\"Could not set configuration or claim interface: {str(e)}\")
#     sys.exit(1)

# Send the control transfer
try:
    bmRequestType = 0x21  # Host to Device | Class | Interface
    bRequest = 0x09       # SET_REPORT
    wValue = WVALUE       # 0x035A
    wIndex = WINDEX       # Interface number
    ret = dev.ctrl_transfer(bmRequestType, bRequest, wValue, wIndex, data, timeout=1000)
    if ret != WLENGTH:
        print(f\"Warning: Only {ret} bytes sent out of {WLENGTH}.\")
    else:
        print(\"Data packet sent successfully.\")
except usb.core.USBError as e:
    print(f\"Control transfer failed: {str(e)}\")
    usb.util.release_interface(dev, WINDEX)
    sys.exit(1)

# Release the interface
usb.util.release_interface(dev, WINDEX)
# Reattach the kernel driver if necessary
try:
    dev.attach_kernel_driver(WINDEX)
except usb.core.USBError:
    pass  # Ignore if we can't reattach the driver

sys.exit(0)
" > /tmp/duo/backlight.py
fi

# Generate the virtual key injection script (for Wayland, uses /dev/uinput).
# This creates a virtual keyboard device to inject brightness key events,
# allowing GNOME to display its native on-screen brightness indicator.
if [ ! -f /tmp/duo/inject_key.py ]; then
    cat > /tmp/duo/inject_key.py << 'IKEOF'
#!/usr/bin/env python3
"""Inject a key event via /dev/uinput so GNOME handles it natively with OSD."""
import struct, os, sys, time, fcntl

# Map human-readable key names to Linux input event key codes
KEYS = {"brightnessdown": 224, "brightnessup": 225}
key = KEYS.get(sys.argv[1])
if key is None:
    sys.exit(1)

# ioctl constants for uinput device setup
UI_SET_EVBIT  = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_DEV_SETUP  = 0x405C5503
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502
# Linux input event types
EV_SYN = 0x00  # Synchronization event
EV_KEY = 0x01  # Key press/release event

def ev(typ, code, value):
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
os.write(fd, ev(EV_KEY, key, 1))   # Key press
os.write(fd, ev(EV_SYN, 0, 0))     # Sync
os.write(fd, ev(EV_KEY, key, 0))   # Key release
os.write(fd, ev(EV_SYN, 0, 0))     # Sync

time.sleep(0.1)  # Allow time for event processing
# Clean up: destroy virtual device and close file descriptor
fcntl.ioctl(fd, UI_DEV_DESTROY)
os.close(fd)
IKEOF
fi

# Generate the Bluetooth backlight control script (uses hidraw Feature Report).
# Unlike USB mode which uses libusb control transfers, Bluetooth uses the
# HID feature report interface via the hidraw device node.
if [ ! -f /tmp/duo/bt_backlight.py ]; then
    cat > /tmp/duo/bt_backlight.py << 'BTEOF'
#!/usr/bin/env python3
"""Send backlight feature report via hidraw (Bluetooth HID)."""
import sys, fcntl, os

def HIDIOCSFEATURE(length):
    """Build the HIDIOCSFEATURE ioctl number for the given data length."""
    return (3 << 30) | (length << 16) | (0x48 << 8) | 0x06

WLENGTH = 16  # Feature report packet length
level = int(sys.argv[1])   # Backlight level (0-3)
hidraw = sys.argv[2]       # Path to hidraw device (e.g., /dev/hidraw0)

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
BTEOF
fi

# ============================================================================
# STATE MANAGEMENT
# ============================================================================

# Capture current Wi-Fi and Bluetooth states on startup so they can be
# restored after keyboard attach/detach events (the keyboard dock controls
# airplane mode which may toggle these radios)
WIFI_BEFORE=$(nmcli radio wifi)
BLUETOOTH_BEFORE=$(rfkill -n -o SOFT list bluetooth |head -n1)

# Check if the keyboard dock is currently connected via USB
KEYBOARD_ATTACHED=false
if [ -n "$(lsusb | grep 'Zenbook Duo Keyboard')" ]; then
    KEYBOARD_ATTACHED=true
fi

# Count the number of active logical monitors (1=top only, 2=both screens)
MONITOR_COUNT=$(gdctl show | grep 'Logical monitor #' | wc -l)

# Write current state to a shared status file that other functions can source
function duo-set-status() {
    echo "
        BLUETOOTH_BEFORE=${BLUETOOTH_BEFORE}
        WIFI_BEFORE=${WIFI_BEFORE}
        KEYBOARD_ATTACHED=${KEYBOARD_ATTACHED}
        MONITOR_COUNT=${MONITOR_COUNT}
    " > /tmp/duo/status
}
duo-set-status

# Initialize the keyboard backlight level tracking file
echo "${DEFAULT_BACKLIGHT}" > /tmp/duo/kb_backlight_level

# ============================================================================
# DEVICE DISCOVERY
# ============================================================================

# Find the hidraw device node for the Bluetooth-connected keyboard.
# Scans /sys/class/hidraw/ entries to match the keyboard by name and
# bus type 0005 (Bluetooth), returning the /dev/hidrawN path.
function duo-find-kb-hidraw() {
    for h in /sys/class/hidraw/hidraw*/device/uevent; do
        if grep -q "HID_NAME=ASUS Zenbook Duo Keyboard" "$h" 2>/dev/null; then
            # Bus type 0005 = Bluetooth HID (vs 0003 = USB HID)
            if grep -q "HID_ID=0005:" "$h" 2>/dev/null; then
                local hidraw_name=$(basename $(dirname $(dirname "$h")))
                echo "/dev/${hidraw_name}"
                return
            fi
        fi
    done
}

# Find the /dev/input/eventN device for the keyboard that supports LED events.
# Filters by name match and checks that the event capability bitmask includes
# bit 20 (EV_LED = 0x100000), which identifies the correct keyboard sub-device.
function duo-find-kb-device() {
    for devpath in /sys/class/input/event*/device/name; do
        if grep -q "ASUS Zenbook Duo Keyboard$" "$devpath" 2>/dev/null; then
            local evname=$(basename $(dirname $(dirname "$devpath")))
            local evcaps=$(cat "/sys/class/input/${evname}/device/capabilities/ev" 2>/dev/null)
            # Check for EV_LED capability (bit 20) to identify the right sub-device
            if [ -n "${evcaps}" ] && (( 0x${evcaps} & 0x100000 )); then
                echo "/dev/input/${evname}"
                return
            fi
        fi
    done
}

# Find all ABS_MISC input devices belonging to the Bluetooth keyboard.
# These devices report Fn key presses as ABS_MISC events (rather than KEY events).
# Filters for devices that have ABS capability but NOT KEY capability (ev bit 1),
# which distinguishes the Fn-key consumer control sub-device from the main keyboard.
function duo-find-kb-abs-devices() {
    for devpath in /sys/class/input/event*/device/name; do
        if grep -q "ASUS Zenbook Duo Keyboard$" "$devpath" 2>/dev/null; then
            local evname=$(basename $(dirname $(dirname "$devpath")))
            local abscaps=$(cat "/sys/class/input/${evname}/device/capabilities/abs" 2>/dev/null)
            local evcaps=$(cat "/sys/class/input/${evname}/device/capabilities/ev" 2>/dev/null)
            # Match device with ABS capability but no KEY capability (ev bit 1)
            if [ -n "${abscaps}" ] && [ "${abscaps}" != "0" ] && [ -n "${evcaps}" ] && ! (( 0x${evcaps} & 0x2 )); then
                echo "/dev/input/${evname}"
            fi
        fi
    done
}

# ============================================================================
# KEYBOARD BACKLIGHT
# ============================================================================

# Set the keyboard backlight to a given level (0-3).
# Persists the level to a file and sends the command via USB or Bluetooth
# depending on how the keyboard is connected.
function duo-set-kb-backlight() {
    echo "${1}" > /tmp/duo/kb_backlight_level
    if [ -n "${KEYBOARD_DEV}" ]; then
        # USB mode: use libusb control transfer via backlight.py
        /usr/bin/sudo ${PYTHON3} /tmp/duo/backlight.py ${1} >/dev/null
    else
        # Bluetooth mode: use hidraw feature report via bt_backlight.py
        local BT_HIDRAW=$(duo-find-kb-hidraw)
        if [ -n "${BT_HIDRAW}" ]; then
            /usr/bin/sudo ${PYTHON3} /tmp/duo/bt_backlight.py ${1} ${BT_HIDRAW} >/dev/null
        fi
    fi
}

# Cycle the keyboard backlight to the next level (0->1->2->3->0).
# Uses a file lock to prevent concurrent execution from multiple event sources.
function duo-cycle-kb-backlight() {
    (
        flock -n 9 || return  # Non-blocking lock; skip if already cycling
        local LEVEL=$(cat /tmp/duo/kb_backlight_level 2>/dev/null || echo 0)
        LEVEL=$(( (LEVEL + 1) % 4 ))  # Wrap around from 3 back to 0
        echo "$(date) - KBLIGHT - Cycling to level ${LEVEL}"
        duo-set-kb-backlight ${LEVEL}
    ) 9>/tmp/duo/kb_backlight_lock
}

# ============================================================================
# DISPLAY BRIGHTNESS
# ============================================================================

# Track the primary display brightness to sync it to the secondary display
BRIGHTNESS=0

# Inject a brightness key event (up or down) via the virtual keyboard.
# This triggers GNOME's native brightness handling and OSD display.
function duo-step-brightness() {
    local DIRECTION=${1}  # "up" or "down"
    echo "$(date) - BRIGHTNESS - ${DIRECTION}"
    /usr/bin/sudo ${PYTHON3} /tmp/duo/inject_key.py "brightness${DIRECTION}" >/dev/null
}

# Sync the bottom display (eDP-2) brightness to match the top display (eDP-1/intel_backlight).
# Only runs when the keyboard is detached (both screens in use).
function duo-sync-display-backlight() {
    . /tmp/duo/status
    if [ "${KEYBOARD_ATTACHED}" = false ]; then
        CUR_BRIGHTNESS=$(cat /sys/class/backlight/intel_backlight/brightness)
        if [ "${CUR_BRIGHTNESS}" != "${BRIGHTNESS}" ]; then
            BRIGHTNESS=${CUR_BRIGHTNESS}
            echo "$(date) - DISPLAY - Setting brightness to $(echo ${BRIGHTNESS} |sudo tee /sys/class/backlight/card1-eDP-2-backlight/brightness)"
        fi
    fi
}

# ============================================================================
# MONITOR CONFIGURATION
# ============================================================================

# Core monitor configuration logic. Called on USB hotplug, lock/unlock, and boot events.
# Detects keyboard attach/detach state and configures the system accordingly:
#   - Keyboard attached: disable bottom screen, restore Wi-Fi/Bluetooth, set backlight
#   - Keyboard detached: enable bottom screen, enable Bluetooth, restore Wi-Fi
function duo-check-monitor() {
    . /tmp/duo/status

    # Re-detect keyboard connection state
    KEYBOARD_ATTACHED=false
    if [ -n "$(lsusb | grep 'Zenbook Duo Keyboard')" ]; then
        KEYBOARD_ATTACHED=true
    fi
    # Re-count active logical monitors
    MONITOR_COUNT=$(gdctl show | grep 'Logical monitor #' | wc -l)
    duo-set-status

    echo "$(date) - MONITOR - WIFI before: ${WIFI_BEFORE}, Bluetooth before: ${BLUETOOTH_BEFORE}"
    echo "$(date) - MONITOR - Keyboard attached: ${KEYBOARD_ATTACHED}, Monitor count: ${MONITOR_COUNT}"

    if [ ${KEYBOARD_ATTACHED} = true ]; then
        # --- Keyboard attached: laptop is in "laptop mode" with physical keyboard ---
        echo "$(date) - MONITOR - Keyboard attached"
        duo-set-kb-backlight ${DEFAULT_BACKLIGHT}

        # Restore Wi-Fi to whatever state it was in before detach
        if [ "${WIFI_BEFORE}" = enabled ]; then
            echo "$(date) - MONITOR - Turning on WIFI"
            nmcli radio wifi on
        fi
        # Restore Bluetooth to its previous state
        if [ "${BLUETOOTH_BEFORE}" = unblocked ]; then
            echo "$(date) - MONITOR - Turning on Bluetooth"
            rfkill unblock bluetooth
        else
            echo "$(date) - MONITOR - Turning off Bluetooth"
            rfkill block bluetooth
        fi
        # Disable the bottom screen (eDP-2) since keyboard covers it
        if ((${MONITOR_COUNT} > 1)); then
            echo "$(date) - MONITOR - Disabling bottom monitor"
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1
            NEW_MONITOR_COUNT=$(gdctl show | grep 'Logical monitor #' | wc -l)
            if ((${NEW_MONITOR_COUNT} == 1)); then
                MESSAGE="Disabled bottom display"
            else
                MESSAGE="ERROR: Bottom display still on"
            fi
            notify-send -a "Zenbook Duo" -t 1000 --hint=int:transient:1 -i "preferences-desktop-display" "${MESSAGE}"
        fi
    else
        # --- Keyboard detached: laptop is in "tablet/dual-screen mode" ---
        echo "$(date) - MONITOR - Keyboard detached"

        # Restore Wi-Fi to its previous state
        if [ "${WIFI_BEFORE}" = enabled ]; then
            echo "$(date) - MONITOR - Turning on WIFI"
            nmcli radio wifi on
        fi
        # Always enable Bluetooth when keyboard is detached (needed for BT keyboard)
        echo "$(date) - MONITOR - Turning on Bluetooth"
        rfkill unblock bluetooth

        # Enable the bottom screen (eDP-2) positioned below the top screen
        if ((${MONITOR_COUNT} < 2)); then
            echo "$(date) - MONITOR - Enabling bottom monitor"
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --logical-monitor --scale ${SCALE} --monitor eDP-2 --below eDP-1
            NEW_MONITOR_COUNT=$(gdctl show | grep 'Logical monitor #' | wc -l)
            if ((${NEW_MONITOR_COUNT} == 2)); then
                MESSAGE="Enabled bottom display"
            else
                MESSAGE="ERROR: Bottom display still off"
            fi
            notify-send -a "Zenbook Duo" -t 1000 --hint=int:transient:1 -i "preferences-desktop-display" "${MESSAGE}"
        fi
    fi
}

# ============================================================================
# EVENT WATCHERS
# ============================================================================

# Watch for USB bus attribute changes (keyboard attach/detach).
# Uses inotifywait on /dev/bus/usb/ to detect when USB devices are connected
# or disconnected, then triggers a full monitor configuration check.
function duo-watch-monitor() {
    while true; do
        echo "$(date) - MONITOR - Waiting for USB event"
        inotifywait -e attrib /dev/bus/usb/*/ >/dev/null 2>&1
        duo-check-monitor
    done
}

# Watch for changes to the primary display brightness and sync to the secondary display.
# Uses inotifywait to react to sysfs brightness file modifications.
function duo-watch-display-backlight() {
    while true; do
        inotifywait -e modify /sys/class/backlight/intel_backlight/brightness >/dev/null 2>&1
        duo-sync-display-backlight
    done
}

# Watch for keyboard backlight toggle key (F4) and Fn brightness keys.
# Monitors input events via evtest on both KEY and ABS_MISC sub-devices.
# Automatically reconnects when the keyboard is disconnected and reconnected.
function duo-watch-kb-backlight-key() {
    echo "$(date) - KBLIGHT - Watching for backlight key (F4)"
    while true; do
        # Locate keyboard input devices (KEY device for USB, ABS devices for Bluetooth Fn keys)
        local KB_DEV=$(duo-find-kb-device)
        local KB_ABS_DEVS=$(duo-find-kb-abs-devices)
        if [ -z "${KB_DEV}" ] && [ -z "${KB_ABS_DEVS}" ]; then
            echo "$(date) - KBLIGHT - Keyboard device not found, retrying in 5s"
            sleep 5
            continue
        fi
        local EVTEST_PIDS=""
        # Monitor the KEY sub-device for F4 key press (USB keyboard backlight toggle)
        if [ -n "${KB_DEV}" ]; then
            echo "$(date) - KBLIGHT - Monitoring ${KB_DEV} (KEY_F4)"
            stdbuf -oL evtest "${KB_DEV}" 2>/dev/null | while read -r line; do
                if echo "$line" | grep -q "KEY_F4.*value 1"; then
                    duo-cycle-kb-backlight
                fi
            done &
            EVTEST_PIDS="$!"
        fi
        # Monitor ABS_MISC sub-devices for Bluetooth Fn key events:
        #   value 199 = backlight cycle, value 16 = brightness down, value 32 = brightness up
        for KB_ABS_DEV in ${KB_ABS_DEVS}; do
            echo "$(date) - KBLIGHT - Monitoring ${KB_ABS_DEV} (ABS_MISC)"
            stdbuf -oL evtest "${KB_ABS_DEV}" 2>/dev/null | while read -r line; do
                if echo "$line" | grep -q "ABS_MISC.*value 199"; then
                    duo-cycle-kb-backlight
                elif echo "$line" | grep -q "ABS_MISC.*value 16$"; then
                    duo-step-brightness down
                elif echo "$line" | grep -q "ABS_MISC.*value 32$"; then
                    duo-step-brightness up
                fi
            done &
            EVTEST_PIDS="${EVTEST_PIDS} $!"
        done
        # Wait for any evtest process to exit (indicates device disconnected)
        wait -n ${EVTEST_PIDS} 2>/dev/null
        # Kill all remaining evtest processes and wait for cleanup
        kill ${EVTEST_PIDS} 2>/dev/null
        wait ${EVTEST_PIDS} 2>/dev/null
        echo "$(date) - KBLIGHT - Device lost, re-scanning in 2s"
        sleep 2
    done
}

# Monitor Wi-Fi state changes via D-Bus (NetworkManager).
# When the keyboard is attached, tracks the Wi-Fi enabled/disabled state
# so it can be restored after keyboard detach events.
function duo-watch-wifi() {
    while read -r LINE; do
        sleep 1  # Debounce to avoid rapid state changes
        . /tmp/duo/status
        if [ "${KEYBOARD_ATTACHED}" = true ]; then
            if [[ "${LINE}" = *"<true>"* ]]; then
                WIFI_BEFORE=enabled
            else
                WIFI_BEFORE=disabled
            fi
            echo "$(date) - NETWORK - WIFI: ${WIFI_BEFORE}"
            duo-set-status
        fi
    done < <(gdbus monitor -y -d org.freedesktop.NetworkManager | grep --line-buffered WirelessEnabled)
}

# Monitor Bluetooth power state changes via D-Bus (BlueZ).
# When the keyboard is attached, tracks the Bluetooth blocked/unblocked state
# so it can be restored after keyboard detach events.
function duo-watch-bluetooth() {
    while read -r LINE; do
        sleep 1  # Debounce to avoid rapid state changes
        . /tmp/duo/status
        if [ "${KEYBOARD_ATTACHED}" = true ]; then
            if [[ "${LINE}" = *"<true>"* ]]; then
                BLUETOOTH_BEFORE=unblocked
            else
                BLUETOOTH_BEFORE=blocked
            fi
            echo "$(date) - NETWORK - Bluetooth: ${BLUETOOTH_BEFORE}"
            duo-set-status
        fi
    done < <(gdbus monitor -y -d org.bluez | grep --line-buffered "'Powered':")
}

# Monitor screen lock/unlock events via D-Bus (logind).
# On lock state changes, updates Bluetooth state and re-checks the monitor
# configuration (the keyboard may have been attached/detached while locked).
function duo-watch-lock() {
    while read -r LINE; do
        sleep 1  # Debounce
        echo "$(date) - DEBUG - ${LINE}"
        . /tmp/duo/status
        if [ "${KEYBOARD_ATTACHED}" = true ]; then
            if [[ "${LINE}" = *"<true>"* ]]; then
                BLUETOOTH_BEFORE=unblocked
            else
                BLUETOOTH_BEFORE=blocked
            fi
            echo "$(date) - NETWORK - Bluetooth: ${BLUETOOTH_BEFORE}"
            duo-set-status
            duo-check-monitor
        fi
    done < <(gdbus monitor -y -d org.freedesktop.login1 | grep --line-buffered "LockedHint")
}

# Watch for accelerometer orientation changes using iio-sensor-proxy.
# Extracts the orientation keyword (left-up, right-up, bottom-up, normal)
# and re-invokes this script with that orientation as an argument.
function duo-watch-rotate() {
    echo "$(date) - ROTATE - Watching"
    monitor-sensor --accel |
        stdbuf -oL grep "Accelerometer orientation changed:" |
        stdbuf -oL awk '{print $4}' |
        xargs -I '{}' stdbuf -oL "$0" '{}' 2>/dev/null
}

# ============================================================================
# CLI HANDLER
# ============================================================================

# CLI command handler for external invocations (systemd sleep hooks, manual commands).
# Supports ACPI power events (pre/post/hibernate/thaw/boot/shutdown),
# manual keyboard backlight setting (kbb), and screen rotation commands.
function duo-cli() {
    . /tmp/duo/status
    case "${1}" in
    pre|hibernate|shutdown)
        # System going to sleep or shutting down: turn off keyboard backlight
        echo "$(date) - ACPI - $@"
        duo-set-kb-backlight 0
    ;;
    post|thaw|boot)
        # System waking up or booting: restore backlight and reconfigure monitors
        echo "$(date) - ACPI - $@"
        duo-set-kb-backlight ${DEFAULT_BACKLIGHT}
        duo-check-monitor
    ;;
    kbb)
        # Manually set keyboard backlight level (e.g., "duo kbb 2")
        echo "$(date) - KEYBOARD - Backlight = ${2}"
        duo-set-kb-backlight ${2}
    ;;
    left-up)
        # Accelerometer: device rotated with left side up (90° clockwise)
        echo "$(date) - ROTATE - Left-up"
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 90
        else
            # Dual-screen mode: rotate both displays, place eDP-2 to the left
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 90 --logical-monitor --scale ${SCALE} --monitor eDP-2 --left-of eDP-1 --transform 90
        fi

        ;;
    right-up)
        # Accelerometer: device rotated with right side up (270° clockwise)
        echo "$(date) - ROTATE - Right-up"
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 270
        else
            # Dual-screen mode: rotate both displays, place eDP-2 to the right
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 270 --logical-monitor --scale ${SCALE} --monitor eDP-2 --right-of eDP-1 --transform 270
        fi
        ;;
    bottom-up)
        # Accelerometer: device rotated upside down (180°)
        echo "$(date) - ROTATE - Bottom-up"
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 180
        else
            # Dual-screen mode: rotate both displays, place eDP-2 above
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 180 --logical-monitor --scale ${SCALE} --monitor eDP-2 --above eDP-1 --transform 180
        fi
        ;;
    normal)
        # Accelerometer: device in normal upright orientation (0°)
        echo "$(date) - ROTATE - Normal"
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1
        else
            # Dual-screen mode: standard layout with eDP-2 below eDP-1
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --logical-monitor --scale ${SCALE} --monitor eDP-2 --below eDP-1
        fi
        ;;
    *)
        echo "$(date) - UNKNOWN - $@"
        ;;
    esac
}

# ============================================================================
# ENTRY POINT
# ============================================================================

# Main entry point: initialize hardware state and launch all background watchers.
# Each watcher runs as a background process monitoring a specific subsystem.
function main() {
    duo-set-kb-backlight ${DEFAULT_BACKLIGHT}  # Set initial keyboard backlight
    duo-check-monitor                          # Configure displays based on current state
    duo-watch-monitor &                        # Watch USB hotplug (keyboard attach/detach)
    duo-watch-rotate &                         # Watch accelerometer for screen rotation
    duo-watch-display-backlight &              # Sync brightness between displays
    duo-watch-kb-backlight-key &               # Watch for Fn+F4 backlight key
    duo-watch-wifi &                           # Track Wi-Fi state changes
    duo-watch-bluetooth                        # Track Bluetooth state changes (runs in foreground)
}

# Dispatch: no arguments = run as daemon, with arguments = run as CLI command.
if [ -z "${1}" ]; then
    main | tee -a /tmp/duo/duo.log
else
    duo-cli $@ | tee -a /tmp/duo/duo.log
    # When run as root (e.g., from systemd sleep hook), fix permissions so
    # the user session can read/write the shared state files
    if [ "${USER}" = root ]; then
        chmod a+w /tmp/duo /tmp/duo/duo.log /tmp/duo/status
    fi
fi
