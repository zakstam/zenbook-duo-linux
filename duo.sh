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

DUO_CLEANED_UP=false

function duo-cleanup() {
    # Best-effort: avoid double cleanup.
    if [ "${DUO_CLEANED_UP}" = "true" ]; then
        return
    fi
    DUO_CLEANED_UP=true

    # Stop any background jobs started by this shell (watchers, retry helpers).
    local JOB_PIDS
    JOB_PIDS="$(jobs -pr 2>/dev/null || true)"
    if [ -n "${JOB_PIDS}" ]; then
        kill -TERM ${JOB_PIDS} 2>/dev/null || true
    fi

    # Also kill any remaining direct children (process substitutions, helpers).
    pkill -TERM -P $$ 2>/dev/null || true

    # Give processes a moment to exit cleanly, then hard-kill stragglers.
    sleep 0.2
    if [ -n "${JOB_PIDS}" ]; then
        kill -KILL ${JOB_PIDS} 2>/dev/null || true
    fi
    pkill -KILL -P $$ 2>/dev/null || true
}

function duo-handle-signal() {
    echo "$(date) - SERVICE - Signal received, shutting down..."
    duo-cleanup
    exit 0
}

trap duo-handle-signal INT TERM HUP QUIT
trap duo-cleanup EXIT

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

# Locate the python3 interpreter for running USB/HID helper scripts.
# Prefer /usr/bin/python3 to match typical sudoers entries.
if [ -x /usr/bin/python3 ]; then
    PYTHON3=/usr/bin/python3
else
    PYTHON3=$(command -v python3 2>/dev/null || true)
fi

function duo-timeout() {
    local DURATION="${1}"
    shift
    if command -v timeout >/dev/null 2>&1; then
        timeout --preserve-status "${DURATION}" "$@"
    else
        "$@"
    fi
}

function duo-has-graphical-session() {
    # Avoid doing GNOME session work from root/system services (boot/shutdown hooks).
    if [ "${EUID}" = "0" ]; then
        return 1
    fi

    # Systemd user services often don't have DISPLAY/WAYLAND env set, but they
    # can still talk to the user D-Bus. Prefer that as a signal.
    if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "${XDG_RUNTIME_DIR}/bus" ]; then
        return 0
    fi

    # Best-effort fallback: if we can't prove it, still allow trying (all GNOME
    # calls are wrapped with timeouts and should fail fast).
    return 0
}

function duo-usb-keyboard-id() {
    lsusb 2>/dev/null | awk '/Zenbook Duo Keyboard/ {print $6; exit}'
}

function duo-usb-keyboard-present() {
    [ -n "$(duo-usb-keyboard-id)" ]
}

# ============================================================================
# USB MEDIA REMAP (optional UI helper)
# ============================================================================

function duo-usb-media-remap-pidfile() {
    echo "/tmp/duo-${UID}/usb_media_remap.pid"
}

function duo-usb-media-remap-enabled() {
    # Default to enabled if not configured (matches UI default).
    local settings_file="${XDG_CONFIG_HOME:-$HOME/.config}/zenbook-duo/settings.json"
    if [ -z "${PYTHON3}" ] || [ ! -f "${settings_file}" ]; then
        echo "true"
        return 0
    fi

    "${PYTHON3}" - <<PY 2>/dev/null || echo "true"
import json
from pathlib import Path

p = Path("${settings_file}")
try:
    data = json.loads(p.read_text())
    v = data.get("usbMediaRemapEnabled", True)
    print("true" if bool(v) else "false")
except Exception:
    print("true")
PY
}

function duo-usb-media-remap-running() {
    local pid_file
    pid_file="$(duo-usb-media-remap-pidfile)"
    if [ ! -f "${pid_file}" ]; then
        return 1
    fi
    local pid
    pid="$(cat "${pid_file}" 2>/dev/null || true)"
    if ! [[ "${pid}" =~ ^[0-9]+$ ]]; then
        return 1
    fi
    kill -0 "${pid}" 2>/dev/null
}

function duo-usb-media-remap-start() {
    local helper=()
    if command -v usb-media-remap >/dev/null 2>&1; then
        helper=(usb-media-remap)
    elif command -v zenbook-duo-control >/dev/null 2>&1; then
        helper=(zenbook-duo-control --usb-media-remap-helper)
    else
        return 0
    fi
    if duo-usb-media-remap-running; then
        return 0
    fi

    local pid_file
    pid_file="$(duo-usb-media-remap-pidfile)"
    mkdir -p "$(dirname "${pid_file}")" 2>/dev/null || true

    echo "$(date) - USB-REMAP - Starting (pid file: ${pid_file})"
    "${helper[@]}" --pid-file "${pid_file}" --user "${USER}" >/dev/null 2>&1 &
}

function duo-usb-media-remap-stop() {
    local helper=()
    if command -v usb-media-remap >/dev/null 2>&1; then
        helper=(usb-media-remap)
    elif command -v zenbook-duo-control >/dev/null 2>&1; then
        helper=(zenbook-duo-control --usb-media-remap-helper)
    else
        return 0
    fi
    if ! duo-usb-media-remap-running; then
        return 0
    fi

    local pid_file
    pid_file="$(duo-usb-media-remap-pidfile)"
    echo "$(date) - USB-REMAP - Stopping (pid file: ${pid_file})"
    duo-timeout 3s "${helper[@]}" --stop --pid-file "${pid_file}" >/dev/null 2>&1 || true
}

# ============================================================================
# HELPER SCRIPTS
# ============================================================================

if [ -z "${DUO_LIBEXEC_DIR:-}" ]; then
    DUO_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    if [ -d "${DUO_SCRIPT_DIR}/libexec" ]; then
        DUO_LIBEXEC_DIR="${DUO_SCRIPT_DIR}/libexec"
    else
        DUO_LIBEXEC_DIR="/usr/local/libexec/zenbook-duo"
    fi
fi
DUO_BACKLIGHT_USB_PY="${DUO_LIBEXEC_DIR}/backlight.py"
DUO_BACKLIGHT_BT_PY="${DUO_LIBEXEC_DIR}/bt_backlight.py"
DUO_INJECT_KEY_PY="${DUO_LIBEXEC_DIR}/inject_key.py"

if [ -z "${PYTHON3}" ]; then
    echo "$(date) - INIT - ERROR: python3 not found in PATH"
fi

# ============================================================================
# DISPLAY BACKEND (GNOME/KDE)
# ============================================================================

DUO_DISPLAY_BACKEND=""
DUO_DISPLAY_BACKEND_FILE=""

function duo-detect-display-backend() {
    local kde_ok=false
    local gnome_ok=false

    if command -v kscreen-doctor >/dev/null 2>&1; then
        if duo-timeout 2s kscreen-doctor -j >/dev/null 2>&1; then
            kde_ok=true
        fi
    fi

    if command -v gdctl >/dev/null 2>&1; then
        if duo-has-graphical-session; then
            if duo-timeout 2s gdctl show >/dev/null 2>&1; then
                gnome_ok=true
            fi
        fi
    fi

    if [[ "${XDG_CURRENT_DESKTOP:-}" =~ KDE ]] || [[ "${XDG_SESSION_DESKTOP:-}" =~ KDE ]]; then
        if [ "${kde_ok}" = true ]; then
            DUO_DISPLAY_BACKEND="kde"
        fi
    elif [[ "${XDG_CURRENT_DESKTOP:-}" =~ GNOME ]] || [[ "${XDG_SESSION_DESKTOP:-}" =~ GNOME ]]; then
        if [ "${gnome_ok}" = true ]; then
            DUO_DISPLAY_BACKEND="gnome"
        fi
    fi

    if [ -z "${DUO_DISPLAY_BACKEND}" ]; then
        if [ "${kde_ok}" = true ]; then
            DUO_DISPLAY_BACKEND="kde"
        elif [ "${gnome_ok}" = true ]; then
            DUO_DISPLAY_BACKEND="gnome"
        fi
    fi

    if [ -n "${DUO_DISPLAY_BACKEND}" ]; then
        DUO_DISPLAY_BACKEND_FILE="${DUO_LIBEXEC_DIR}/display-${DUO_DISPLAY_BACKEND}.sh"
    fi
}

duo-detect-display-backend
if [ -n "${DUO_DISPLAY_BACKEND_FILE}" ] && [ -f "${DUO_DISPLAY_BACKEND_FILE}" ]; then
    # shellcheck source=/dev/null
    . "${DUO_DISPLAY_BACKEND_FILE}"
else
    echo "$(date) - INIT - WARN: display backend not available (backend=${DUO_DISPLAY_BACKEND})"
fi

# ============================================================================
# STATE MANAGEMENT
# ============================================================================

# Capture current Wi-Fi and Bluetooth states on startup so they can be
# restored after keyboard attach/detach events (the keyboard dock controls
# airplane mode which may toggle these radios)
WIFI_BEFORE=$(duo-timeout 2s nmcli radio wifi 2>/dev/null | head -n1 || true)
if [ "${WIFI_BEFORE}" != "enabled" ] && [ "${WIFI_BEFORE}" != "disabled" ]; then
    WIFI_BEFORE=unknown
fi

BLUETOOTH_BEFORE=$(duo-timeout 2s rfkill -n -o SOFT list bluetooth 2>/dev/null | head -n1 || true)
if [ "${BLUETOOTH_BEFORE}" != "blocked" ] && [ "${BLUETOOTH_BEFORE}" != "unblocked" ]; then
    BLUETOOTH_BEFORE=unknown
fi

# Check if the keyboard dock is currently connected via USB
KEYBOARD_ATTACHED=false
if [ -n "$(lsusb | grep 'Zenbook Duo Keyboard')" ]; then
    KEYBOARD_ATTACHED=true
fi

# Count the number of active logical monitors (1=top only, 2=both screens)
MONITOR_COUNT=0
if declare -F duo-display-count >/dev/null 2>&1; then
    MONITOR_COUNT=$(duo-display-count 2>/dev/null || echo 0)
fi

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
        if [ -z "${PYTHON3}" ] || [ ! -f "${DUO_BACKLIGHT_USB_PY}" ]; then
            echo "$(date) - KBLIGHT - ERROR: Missing helper script ${DUO_BACKLIGHT_USB_PY}"
        else
            local RUNNER=()
            if [ "${EUID}" = "0" ]; then
                RUNNER=("${PYTHON3}")
            else
                RUNNER=("/usr/bin/sudo" "${PYTHON3}")
            fi
            if BL_OUTPUT=$(duo-timeout 3s "${RUNNER[@]}" "${DUO_BACKLIGHT_USB_PY}" ${LEVEL} ${VENDOR_ID} ${PRODUCT_ID} 2>&1); then
                APPLIED=true
            else
                echo "$(date) - KBLIGHT - ERROR: Failed to set USB backlight to ${LEVEL}: ${BL_OUTPUT}"
            fi
        fi
    fi

    if [ "${APPLIED}" != "true" ]; then
        # Bluetooth mode: use hidraw feature report via bt_backlight.py
        local BT_HIDRAW=$(duo-find-kb-hidraw)
        if [ -n "${BT_HIDRAW}" ]; then
            local BL_OUTPUT
            if [ -z "${PYTHON3}" ] || [ ! -f "${DUO_BACKLIGHT_BT_PY}" ]; then
                echo "$(date) - KBLIGHT - ERROR: Missing helper script ${DUO_BACKLIGHT_BT_PY}"
            else
                local RUNNER=()
                if [ "${EUID}" = "0" ]; then
                    RUNNER=("${PYTHON3}")
                else
                    RUNNER=("/usr/bin/sudo" "${PYTHON3}")
                fi
                if ! BL_OUTPUT=$(duo-timeout 3s "${RUNNER[@]}" "${DUO_BACKLIGHT_BT_PY}" ${LEVEL} ${BT_HIDRAW} 2>&1); then
                    echo "$(date) - KBLIGHT - ERROR: Failed to set BT backlight to ${LEVEL} via ${BT_HIDRAW}: ${BL_OUTPUT}"
                fi
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
    if [ -z "${PYTHON3}" ] || [ ! -f "${DUO_INJECT_KEY_PY}" ]; then
        echo "$(date) - BRIGHTNESS - ERROR: Missing helper script ${DUO_INJECT_KEY_PY}"
        return 0
    fi
    local RUNNER=()
    if [ "${EUID}" = "0" ]; then
        RUNNER=("${PYTHON3}")
    else
        RUNNER=("/usr/bin/sudo" "${PYTHON3}")
    fi
    if ! KEY_OUTPUT=$(duo-timeout 3s "${RUNNER[@]}" "${DUO_INJECT_KEY_PY}" "brightness${DIRECTION}" 2>&1); then
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

    # Don't try to drive GNOME from root/system hooks.
    if [ "${EUID}" = "0" ]; then
        echo "$(date) - MONITOR - Running as root; skipping monitor configuration"
        return 0
    fi
    if ! declare -F duo-display-count >/dev/null 2>&1; then
        echo "$(date) - MONITOR - Display backend not available; skipping monitor configuration"
        return 0
    fi

    # Re-detect keyboard connection state
    KEYBOARD_ATTACHED=false
    if [ -n "$(lsusb | grep 'Zenbook Duo Keyboard')" ]; then
        KEYBOARD_ATTACHED=true
    fi

    # Start/stop USB media remap helper (optional; provided by the UI package).
    # This is responsible for making the top-row keys behave as intended on USB
    # and also handles the keyboard backlight toggle key in USB mode.
    if [ "${KEYBOARD_ATTACHED}" = true ]; then
        if [ "$(duo-usb-media-remap-enabled)" = "true" ]; then
            duo-usb-media-remap-start
        else
            duo-usb-media-remap-stop
        fi
    else
        duo-usb-media-remap-stop
    fi

    # Re-count active logical monitors
    MONITOR_COUNT=$(duo-display-count 2>/dev/null || echo 0)
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
            if ! duo-timeout 3s nmcli radio wifi on 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to turn on WIFI"
            fi
        fi
        # Restore Bluetooth to its previous state
        if [ "${BLUETOOTH_BEFORE}" = unblocked ]; then
            echo "$(date) - MONITOR - Turning on Bluetooth"
            if ! duo-timeout 3s rfkill unblock bluetooth 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to unblock Bluetooth"
            fi
        elif [ "${BLUETOOTH_BEFORE}" = blocked ]; then
            echo "$(date) - MONITOR - Turning off Bluetooth"
            if ! duo-timeout 3s rfkill block bluetooth 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to block Bluetooth"
            fi
        fi
        # Disable the bottom screen (eDP-2) since keyboard covers it
        if ((${MONITOR_COUNT} > 1)); then
            echo "$(date) - MONITOR - Disabling bottom monitor"
            local DISPLAY_OUTPUT
            if ! DISPLAY_OUTPUT=$(duo-display-set-single 2>&1); then
                echo "$(date) - MONITOR - ERROR: display set failed: ${DISPLAY_OUTPUT}"
            fi
            NEW_MONITOR_COUNT=$(duo-display-count 2>/dev/null || echo 0)
            MONITOR_COUNT=${NEW_MONITOR_COUNT}
            duo-set-status
            if ((${NEW_MONITOR_COUNT} == 1)); then
                MESSAGE="Disabled bottom display"
            else
                MESSAGE="ERROR: Bottom display still on"
            fi
            echo "$(date) - MONITOR - ${MESSAGE}"
            duo-timeout 2s notify-send -a "Zenbook Duo" -t 1000 --hint=int:transient:1 -i "preferences-desktop-display" "${MESSAGE}" || true
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
            if ! duo-timeout 3s nmcli radio wifi on 2>&1; then
                echo "$(date) - MONITOR - ERROR: Failed to turn on WIFI"
            fi
        fi
        # Always enable Bluetooth when keyboard is detached (needed for BT keyboard)
        echo "$(date) - MONITOR - Turning on Bluetooth"
        if ! duo-timeout 3s rfkill unblock bluetooth 2>&1; then
            echo "$(date) - MONITOR - ERROR: Failed to unblock Bluetooth"
        fi

        # Enable the bottom screen (eDP-2) positioned below the top screen
        if ((${MONITOR_COUNT} < 2)); then
            echo "$(date) - MONITOR - Enabling bottom monitor"
            local DISPLAY_OUTPUT
            if ! DISPLAY_OUTPUT=$(duo-display-set-dual-below 2>&1); then
                echo "$(date) - MONITOR - ERROR: display set failed: ${DISPLAY_OUTPUT}"
            fi
            NEW_MONITOR_COUNT=$(duo-display-count 2>/dev/null || echo 0)
            MONITOR_COUNT=${NEW_MONITOR_COUNT}
            duo-set-status
            if ((${NEW_MONITOR_COUNT} == 2)); then
                MESSAGE="Enabled bottom display"
            else
                MESSAGE="ERROR: Bottom display still off"
            fi
            echo "$(date) - MONITOR - ${MESSAGE}"
            duo-timeout 2s notify-send -a "Zenbook Duo" -t 1000 --hint=int:transient:1 -i "preferences-desktop-display" "${MESSAGE}" || true
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
    echo "$(date) - MONITOR - Watching keyboard dock state"
    local last_present="unknown"
    while true; do
        local present="false"
        if duo-usb-keyboard-present; then
            present="true"
        fi
        if [ "${last_present}" != "${present}" ]; then
            last_present="${present}"
            duo-check-monitor
        fi
        sleep 1
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
    # Use a per-user lock file to avoid conflicts with other sessions/users (notably `gdm`).
    exec 8>"/tmp/duo/watch_kb_backlight_key.${UID}.lock"
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
                if echo "$line" | grep -qE "KEY_F4.*value 1|KEY_KBDILLUMTOGGLE.*value 1"; then
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
        if ! declare -F duo-display-rotate-single >/dev/null 2>&1; then
            echo "$(date) - ROTATE - ERROR: display backend not available"
        elif [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! ROT_OUTPUT=$(duo-display-rotate-single left 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        else
            # Dual-screen mode: rotate both displays, place eDP-2 to the left
            if ! ROT_OUTPUT=$(duo-display-rotate-dual left 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        fi

        ;;
    right-up)
        # Accelerometer: device rotated with right side up (270° clockwise)
        echo "$(date) - ROTATE - Right-up"
        if ! declare -F duo-display-rotate-single >/dev/null 2>&1; then
            echo "$(date) - ROTATE - ERROR: display backend not available"
        elif [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! ROT_OUTPUT=$(duo-display-rotate-single right 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        else
            # Dual-screen mode: rotate both displays, place eDP-2 to the right
            if ! ROT_OUTPUT=$(duo-display-rotate-dual right 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        fi
        ;;
    bottom-up)
        # Accelerometer: device rotated upside down (180°)
        echo "$(date) - ROTATE - Bottom-up"
        if ! declare -F duo-display-rotate-single >/dev/null 2>&1; then
            echo "$(date) - ROTATE - ERROR: display backend not available"
        elif [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! ROT_OUTPUT=$(duo-display-rotate-single bottom 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        else
            # Dual-screen mode: rotate both displays, place eDP-2 above
            if ! ROT_OUTPUT=$(duo-display-rotate-dual bottom 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        fi
        ;;
    normal)
        # Accelerometer: device in normal upright orientation (0°)
        echo "$(date) - ROTATE - Normal"
        if ! declare -F duo-display-rotate-single >/dev/null 2>&1; then
            echo "$(date) - ROTATE - ERROR: display backend not available"
        elif [ ${KEYBOARD_ATTACHED} = true ]; then
            if ! ROT_OUTPUT=$(duo-display-rotate-single normal 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
            fi
        else
            # Dual-screen mode: standard layout with eDP-2 below eDP-1
            if ! ROT_OUTPUT=$(duo-display-rotate-dual normal 2>&1); then
                echo "$(date) - ROTATE - ERROR: display set failed: ${ROT_OUTPUT}"
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
    exec > >(tee -a /tmp/duo/duo.log) 2>&1
    main
else
    exec > >(tee -a /tmp/duo/duo.log) 2>&1
    duo-cli "$@"
    # When run as root (e.g., from systemd sleep hook), fix permissions so
    # the user session can read/write the shared state files
    if [ "${USER}" = root ]; then
        chmod 1777 /tmp/duo
        chmod 666 /tmp/duo/duo.log /tmp/duo/status /tmp/duo/kb_backlight_level \
            /tmp/duo/kb_backlight_lock /tmp/duo/kb_backlight_last_cycle 2>/dev/null || true
    fi
fi
