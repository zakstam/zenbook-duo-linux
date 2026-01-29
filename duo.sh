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

# Make /tmp/duo usable across multiple users/processes (daemon + UI).
# Best-effort: may fail if owned by a different user.
chmod 1777 /tmp/duo 2>/dev/null || true
touch /tmp/duo/duo.log /tmp/duo/status /tmp/duo/kb_backlight_level \
    /tmp/duo/kb_backlight_lock /tmp/duo/kb_backlight_last_cycle 2>/dev/null || true
chmod 666 /tmp/duo/status /tmp/duo/kb_backlight_level \
    /tmp/duo/kb_backlight_lock /tmp/duo/kb_backlight_last_cycle 2>/dev/null || true
chmod 666 /tmp/duo/duo.log 2>/dev/null || true

# Use the configured default scale (dynamic detection via gdctl is commented out)
# SCALE=$(gdctl show |grep Scale: |sed 's/│//g' |awk '{print $2}' |head -n1)
# if [ -z "${SCALE}" ]; then
#     SCALE=1
# fi
SCALE=${DEFAULT_SCALE}

# Locate the python3 interpreter for running USB/HID helper scripts
PYTHON3=$(which python3)

function duo-usb-keyboard-id() {
    lsusb 2>/dev/null | awk '/Zenbook Duo Keyboard/ {print $6; exit}'
}

function duo-usb-keyboard-present() {
    [ -n "$(duo-usb-keyboard-id)" ]
}

# ============================================================================
# EMBEDDED PYTHON SCRIPTS
# ============================================================================

# Generate the USB backlight control Python script.
# (We pass vendor/product IDs at runtime so it works across attach/detach.)
cat > /tmp/duo/backlight.py << 'PYEOF'
#!/usr/bin/env python3

import sys
import usb.core
import usb.util

REPORT_ID = 0x5A
WVALUE = 0x035A
WINDEX = 4
WLENGTH = 16


def usage():
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
    print(f"Device not found (Vendor ID: 0x{vendor_id:04X}, Product ID: 0x{product_id:04X})")
    sys.exit(1)

detached = False
try:
    if dev.is_kernel_driver_active(WINDEX):
        dev.detach_kernel_driver(WINDEX)
        detached = True

    bmRequestType = 0x21  # Host to Device | Class | Interface
    bRequest = 0x09  # SET_REPORT
    ret = dev.ctrl_transfer(bmRequestType, bRequest, WVALUE, WINDEX, data, timeout=1000)
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
PYEOF
chmod a+x /tmp/duo/backlight.py 2>/dev/null || true

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

# Find all /dev/input/eventN devices for the keyboard that report EV_KEY.
# The Zenbook Duo keyboard exposes multiple USB interfaces; some keys (notably
# Fn-layer behavior) can appear on different event nodes.
function duo-find-kb-key-devices() {
    declare -A seen
    for devpath in /sys/class/input/event*/device/name; do
        if grep -q "ASUS Zenbook Duo Keyboard$" "$devpath" 2>/dev/null; then
            local evname=$(basename $(dirname $(dirname "$devpath")))
            local evcaps=$(cat "/sys/class/input/${evname}/device/capabilities/ev" 2>/dev/null)
            if [ -n "${evcaps}" ] && (( 0x${evcaps} & 0x2 )); then
                local node="/dev/input/${evname}"
                if [ -z "${seen[${node}]}" ]; then
                    echo "${node}"
                    seen[${node}]=1
                fi
            fi
        fi
    done
}

# Back-compat: return the first key device.
function duo-find-kb-device() {
    duo-find-kb-key-devices | head -n1
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

# Read the persisted keyboard backlight level from disk.
# Falls back to DEFAULT_BACKLIGHT if missing/invalid.
function duo-read-kb-backlight-level() {
    local fallback=${1:-${DEFAULT_BACKLIGHT}}
    local level
    level=$(cat /tmp/duo/kb_backlight_level 2>/dev/null || true)
    if [[ "${level}" =~ ^[0-3]$ ]]; then
        echo "${level}"
    else
        echo "${fallback}"
    fi
}

# Set the keyboard backlight to a given level (0-3).
# Persists the level to a file and sends the command via USB or Bluetooth
# depending on how the keyboard is connected.
function duo-set-kb-backlight() {
    local LEVEL="${1}"
    if ! [[ "${LEVEL}" =~ ^[0-3]$ ]]; then
        LEVEL=${DEFAULT_BACKLIGHT}
    fi

    if ! echo "${LEVEL}" > /tmp/duo/kb_backlight_level 2>/dev/null; then
        echo "$(date) - KBLIGHT - ERROR: Failed to write /tmp/duo/kb_backlight_level"
    fi

    local APPLIED=false

    # Prefer USB when docked; fall back to Bluetooth if USB fails.
    local USB_ID=$(duo-usb-keyboard-id)
    if [ -n "${USB_ID}" ]; then
        local VENDOR_ID=${USB_ID%:*}
        local PRODUCT_ID=${USB_ID#*:}
        local BL_OUTPUT
        if BL_OUTPUT=$(/usr/bin/sudo ${PYTHON3} /tmp/duo/backlight.py ${LEVEL} ${VENDOR_ID} ${PRODUCT_ID} 2>&1); then
            APPLIED=true
        else
            echo "$(date) - KBLIGHT - ERROR: Failed to set USB backlight to ${LEVEL}: ${BL_OUTPUT}"
        fi
    fi

    if [ "${APPLIED}" != "true" ]; then
        # Bluetooth mode: use hidraw feature report via bt_backlight.py
        local BT_HIDRAW=$(duo-find-kb-hidraw)
        if [ -n "${BT_HIDRAW}" ]; then
            local BL_OUTPUT
            if ! BL_OUTPUT=$(/usr/bin/sudo ${PYTHON3} /tmp/duo/bt_backlight.py ${LEVEL} ${BT_HIDRAW} 2>&1); then
                echo "$(date) - KBLIGHT - ERROR: Failed to set BT backlight to ${LEVEL} via ${BT_HIDRAW}: ${BL_OUTPUT}"
            fi
        fi
    fi
}

# Cycle the keyboard backlight to the next level (0->1->2->3->0).
# Uses a file lock for atomicity and a timestamp-based debounce to
# deduplicate events from multiple input devices for the same key press,
# while still allowing real subsequent presses through quickly.
function duo-cycle-kb-backlight() {
    local LEVEL
    {
        flock -n 9 || return

        # Debounce: ignore if last cycle was less than 600ms ago
        local NOW=$(date +%s%3N)
        local LAST=$(cat /tmp/duo/kb_backlight_last_cycle 2>/dev/null || echo 0)
        if (( NOW - LAST < 600 )) && (( NOW - LAST >= 0 )); then
            return
        fi
        echo "${NOW}" > /tmp/duo/kb_backlight_last_cycle

        LEVEL=$(cat /tmp/duo/kb_backlight_level 2>/dev/null || echo 0)
        if ! [[ "${LEVEL}" =~ ^[0-3]$ ]]; then
            LEVEL=0
        fi
        LEVEL=$(( (LEVEL + 1) % 4 ))
        echo "${LEVEL}" > /tmp/duo/kb_backlight_level
    } 9>/tmp/duo/kb_backlight_lock

    echo "$(date) - KBLIGHT - Cycling to level ${LEVEL}"
    duo-set-kb-backlight ${LEVEL}
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
    local KEY_OUTPUT
    if ! KEY_OUTPUT=$(/usr/bin/sudo ${PYTHON3} /tmp/duo/inject_key.py "brightness${DIRECTION}" 2>&1); then
        echo "$(date) - BRIGHTNESS - ERROR: Failed to inject brightness ${DIRECTION} key: ${KEY_OUTPUT}"
    fi
}

# Open an emoji picker.
# On GNOME this is `gnome-characters`.
function duo-open-emoji-picker() {
    if command -v gnome-characters >/dev/null 2>&1; then
        # Avoid spawning multiple windows on repeat key events
        if pgrep -x gnome-characters >/dev/null 2>&1; then
            return
        fi
        nohup gnome-characters >/dev/null 2>&1 &
    fi
}

# Sync the bottom display (eDP-2) brightness to match the top display (eDP-1/intel_backlight).
# Only runs when the keyboard is detached (both screens in use).
function duo-sync-display-backlight() {
    . /tmp/duo/status
    if [ "${KEYBOARD_ATTACHED}" = false ]; then
        CUR_BRIGHTNESS=$(cat /sys/class/backlight/intel_backlight/brightness)
        if [ "${CUR_BRIGHTNESS}" != "${BRIGHTNESS}" ]; then
            BRIGHTNESS=${CUR_BRIGHTNESS}
            if ! echo "${BRIGHTNESS}" | sudo tee /sys/class/backlight/card1-eDP-2-backlight/brightness >/dev/null 2>&1; then
                echo "$(date) - DISPLAY - ERROR: Failed to set eDP-2 brightness to ${BRIGHTNESS}"
            else
                echo "$(date) - DISPLAY - Setting brightness to ${BRIGHTNESS}"
            fi
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
    local PREV_KEYBOARD_ATTACHED=${KEYBOARD_ATTACHED}

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
        # Restore the last requested backlight on a fresh keyboard attach.
        # The keyboard can briefly disconnect/reconnect during backlight commands.
        if [ "${PREV_KEYBOARD_ATTACHED}" != "true" ]; then
            local RESTORE_LEVEL=$(duo-read-kb-backlight-level "${DEFAULT_BACKLIGHT}")
            duo-set-kb-backlight ${RESTORE_LEVEL}
        fi

        # Restore Wi-Fi to whatever state it was in before detach
        if [ "${WIFI_BEFORE}" = enabled ]; then
            echo "$(date) - MONITOR - Turning on WIFI"
            if ! nmcli radio wifi on 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to turn on WIFI"
            fi
        fi
        # Restore Bluetooth to its previous state
        if [ "${BLUETOOTH_BEFORE}" = unblocked ]; then
            echo "$(date) - MONITOR - Turning on Bluetooth"
            if ! rfkill unblock bluetooth 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to unblock Bluetooth"
            fi
        else
            echo "$(date) - MONITOR - Turning off Bluetooth"
            if ! rfkill block bluetooth 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to block Bluetooth"
            fi
        fi
        # Disable the bottom screen (eDP-2) since keyboard covers it
        if ((${MONITOR_COUNT} > 1)); then
            echo "$(date) - MONITOR - Disabling bottom monitor"
            local GDCTL_OUTPUT
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 2>&1); then
                echo "$(date) - MONITOR - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
            NEW_MONITOR_COUNT=$(gdctl show | grep 'Logical monitor #' | wc -l)
            MONITOR_COUNT=${NEW_MONITOR_COUNT}
            duo-set-status
            if ((${NEW_MONITOR_COUNT} == 1)); then
                MESSAGE="Disabled bottom display"
            else
                MESSAGE="ERROR: Bottom display still on"
            fi
            echo "$(date) - MONITOR - ${MESSAGE}"
            notify-send -a "Zenbook Duo" -t 1000 --hint=int:transient:1 -i "preferences-desktop-display" "${MESSAGE}"
        fi
    else
        # --- Keyboard detached: laptop is in "tablet/dual-screen mode" ---
        echo "$(date) - MONITOR - Keyboard detached"

        # On a fresh detach, the keyboard can power-cycle and reset its backlight.
        # Re-apply the last requested level once Bluetooth is ready.
        if [ "${PREV_KEYBOARD_ATTACHED}" = "true" ]; then
            local RESTORE_LEVEL=$(duo-read-kb-backlight-level "${DEFAULT_BACKLIGHT}")
            if [ "${RESTORE_LEVEL}" != "0" ]; then
                (
                    local tries=0
                    while (( tries < 20 )); do
                        # Stop retrying if the keyboard is docked again
                        if duo-usb-keyboard-present; then
                            exit 0
                        fi
                        # Wait until BT hidraw appears
                        if [ -n "$(duo-find-kb-hidraw)" ]; then
                            echo "$(date) - KBLIGHT - Re-applying level ${RESTORE_LEVEL} after detach"
                            duo-set-kb-backlight ${RESTORE_LEVEL}
                            exit 0
                        fi
                        tries=$((tries + 1))
                        sleep 0.25
                    done
                    echo "$(date) - KBLIGHT - WARN: Bluetooth not ready; could not re-apply backlight after detach"
                ) &
            fi
        fi

        # Restore Wi-Fi to its previous state
        if [ "${WIFI_BEFORE}" = enabled ]; then
            echo "$(date) - MONITOR - Turning on WIFI"
            if ! nmcli radio wifi on 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to turn on WIFI"
            fi
        fi
        # Always enable Bluetooth when keyboard is detached (needed for BT keyboard)
        echo "$(date) - MONITOR - Turning on Bluetooth"
        if ! rfkill unblock bluetooth 2>&1; then
            echo "$(date) - MONITOR - ERROR: Failed to unblock Bluetooth"
        fi

        # Enable the bottom screen (eDP-2) positioned below the top screen
        if ((${MONITOR_COUNT} < 2)); then
            echo "$(date) - MONITOR - Enabling bottom monitor"
            local GDCTL_OUTPUT
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --logical-monitor --scale ${SCALE} --monitor eDP-2 --below eDP-1 2>&1); then
                echo "$(date) - MONITOR - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
            NEW_MONITOR_COUNT=$(gdctl show | grep 'Logical monitor #' | wc -l)
            MONITOR_COUNT=${NEW_MONITOR_COUNT}
            duo-set-status
            if ((${NEW_MONITOR_COUNT} == 2)); then
                MESSAGE="Enabled bottom display"
            else
                MESSAGE="ERROR: Bottom display still off"
            fi
            echo "$(date) - MONITOR - ${MESSAGE}"
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
    # Ensure only one watcher instance runs (prevents duplicate evtest consumers)
    exec 8>/tmp/duo/watch_kb_backlight_key.lock
    flock -n 8 || {
        echo "$(date) - KBLIGHT - Watcher already running"
        return
    }

    echo "$(date) - KBLIGHT - Watching for backlight key (F4)"
    while true; do
        # Locate keyboard input devices (KEY devices for USB, ABS devices for Bluetooth Fn keys)
        local KB_DEVS=$(duo-find-kb-key-devices)
        local KB_ABS_DEVS=$(duo-find-kb-abs-devices)
        if [ -z "${KB_DEVS}" ] && [ -z "${KB_ABS_DEVS}" ]; then
            echo "$(date) - KBLIGHT - Keyboard device not found, retrying in 5s"
            sleep 5
            continue
        fi
        local EVTEST_PIDS=""
        # Monitor KEY sub-devices for F4 key press (USB keyboard backlight toggle)
        for KB_DEV in ${KB_DEVS}; do
            echo "$(date) - KBLIGHT - Monitoring ${KB_DEV} (EV_KEY)"
            stdbuf -oL evtest "${KB_DEV}" 2>/dev/null | while read -r line; do
                if echo "$line" | grep -q "KEY_F4.*value 1"; then
                    duo-cycle-kb-backlight
                elif echo "$line" | grep -q "KEY_F11.*value 1"; then
                    duo-open-emoji-picker
                fi
            done &
            EVTEST_PIDS="${EVTEST_PIDS} $!"
        done
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
        local RESTORE_LEVEL=$(duo-read-kb-backlight-level "${DEFAULT_BACKLIGHT}")
        duo-set-kb-backlight ${RESTORE_LEVEL}
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
        local GDCTL_OUTPUT
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 90 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        else
            # Dual-screen mode: rotate both displays, place eDP-2 to the left
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 90 --logical-monitor --scale ${SCALE} --monitor eDP-2 --left-of eDP-1 --transform 90 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        fi

        ;;
    right-up)
        # Accelerometer: device rotated with right side up (270° clockwise)
        echo "$(date) - ROTATE - Right-up"
        local GDCTL_OUTPUT
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 270 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        else
            # Dual-screen mode: rotate both displays, place eDP-2 to the right
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 270 --logical-monitor --scale ${SCALE} --monitor eDP-2 --right-of eDP-1 --transform 270 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        fi
        ;;
    bottom-up)
        # Accelerometer: device rotated upside down (180°)
        echo "$(date) - ROTATE - Bottom-up"
        local GDCTL_OUTPUT
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 180 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        else
            # Dual-screen mode: rotate both displays, place eDP-2 above
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 180 --logical-monitor --scale ${SCALE} --monitor eDP-2 --above eDP-1 --transform 180 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        fi
        ;;
    normal)
        # Accelerometer: device in normal upright orientation (0°)
        echo "$(date) - ROTATE - Normal"
        local GDCTL_OUTPUT
        if [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
        else
            # Dual-screen mode: standard layout with eDP-2 below eDP-1
            if ! GDCTL_OUTPUT=$(gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --logical-monitor --scale ${SCALE} --monitor eDP-2 --below eDP-1 2>&1); then
                echo "$(date) - ROTATE - ERROR: gdctl set failed: ${GDCTL_OUTPUT}"
            fi
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
    local INITIAL_LEVEL=$(duo-read-kb-backlight-level "${DEFAULT_BACKLIGHT}")
    duo-set-kb-backlight ${INITIAL_LEVEL}      # Set initial keyboard backlight
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
        chmod 1777 /tmp/duo
        chmod 666 /tmp/duo/duo.log /tmp/duo/status /tmp/duo/kb_backlight_level \
            /tmp/duo/kb_backlight_lock /tmp/duo/kb_backlight_last_cycle 2>/dev/null || true
    fi
fi
