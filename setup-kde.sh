#!/bin/bash
# Installation script for ASUS Zenbook Duo Linux dual-screen management (KDE).
# Installs dependencies, configures sudoers and udev rules, and installs the
# Rust runtime services.

# ============================================================================
# CONFIGURATION & ARGUMENT PARSING
# ============================================================================

# Default configuration values
DEFAULT_BACKLIGHT=0
DEFAULT_SCALE=1.66
USB_MEDIA_REMAP_ENABLED=true

# Flags:
# --usb-media-remap / --no-usb-media-remap: default setting written for the UI (default: enabled)
while [ "$#" -gt 0 ]; do
    case "$1" in
        --usb-media-remap)
            USB_MEDIA_REMAP_ENABLED=true
            shift
            ;;
        --no-usb-media-remap)
            USB_MEDIA_REMAP_ENABLED=false
            shift
            ;;
        *)
            # Unknown flag/arg - ignore to keep backwards compatibility.
            shift
            ;;
    esac
done

# ============================================================================
# USER CONTEXT
# ============================================================================

# setup-kde.sh is frequently run with `sudo`. In that case we still want all
# per-user config (sudoers, input group, settings.json) to apply to the *real*
# user session, not root.
TARGET_USER="${USER}"
if [ "${EUID}" = "0" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
        TARGET_USER="${SUDO_USER}"
    else
        echo "ERROR: setup-kde.sh must be run from a real user session."
        echo "Run: ./setup-kde.sh"
        exit 1
    fi
fi

TARGET_UID="$(id -u "${TARGET_USER}" 2>/dev/null || true)"
TARGET_HOME="$(getent passwd "${TARGET_USER}" 2>/dev/null | cut -d: -f6)"
if [ -z "${TARGET_UID}" ] || [ -z "${TARGET_HOME}" ]; then
    echo "ERROR: failed to resolve TARGET_USER=${TARGET_USER}"
    exit 1
fi

function duo_prompt() {
    local prompt="${1}"
    local reply_var="${2}"
    local value=""
    if [ -r /dev/tty ]; then
        read -r -p "${prompt}" value </dev/tty
    else
        read -r -p "${prompt}" value
    fi
    printf -v "${reply_var}" '%s' "${value}"
}

# ============================================================================
# PACKAGE INSTALLATION & SCRIPT DEPLOYMENT
# ============================================================================

# Prompt user for configuration preferences (Enter accepts the default)
duo_prompt "What would you like to use for the default keyboard backlight brightness [0-3] (default: ${DEFAULT_BACKLIGHT})? " _input
DEFAULT_BACKLIGHT="${_input:-${DEFAULT_BACKLIGHT}}"
duo_prompt "What would you like to use for monitor scale (1 = 100%, 1.5 = 150%, 1.66 = 166%, 2=200%) (default: ${DEFAULT_SCALE})? " _input
DEFAULT_SCALE="${_input:-${DEFAULT_SCALE}}"
duo_prompt "Enable USB Media Remap by default? [Y/n] " ENABLE_USB_MEDIA_REMAP_ANSWER
case "${ENABLE_USB_MEDIA_REMAP_ANSWER}" in
    [nN]|[nN][oO])
        USB_MEDIA_REMAP_ENABLED=false
        ;;
    *)
        USB_MEDIA_REMAP_ENABLED=true
        ;;
esac

# Detect distro package manager and install required dependencies
if command -v dnf &>/dev/null; then
    sudo dnf install -y \
        usbutils \
        kscreen \
        iio-sensor-proxy \
        systemd
elif command -v pacman &>/dev/null; then
    sudo pacman -S --needed --noconfirm \
        usbutils \
        kscreen \
        iio-sensor-proxy \
        systemd
elif command -v apt &>/dev/null; then
    sudo apt install -y \
        usbutils \
        kscreen \
        iio-sensor-proxy \
        systemd
else
    echo "Unsupported package manager. Please install dependencies manually:"
    echo "  usbutils, kscreen/kscreen-doctor, iio-sensor-proxy, systemd"
    exit 1
fi

# ============================================================================
# SUDOERS CONFIGURATION
# ============================================================================

# Helper: add a sudoers entry if it doesn't already exist.
# Allows passwordless sudo for specific commands needed by the duo script.
function addSudoers() {
    RESULT=$(sudo grep "${1}" /etc/sudoers)
    if [ -z "${RESULT}" ]; then
        echo "${1}" | sudo tee -a /etc/sudoers
    fi
}

# Configure passwordless sudo for sysfs brightness writes used by the Rust session agent.
if [ -n "${TARGET_USER}" ]; then
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/card1-eDP-2-backlight/brightness"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/intel_backlight/brightness"
fi

# ============================================================================
# USER & GROUP SETUP
# ============================================================================

# Add user to the "input" group so evtest can read keyboard input events
# without requiring root (requires logout/login to take effect)
if [ -n "${TARGET_USER}" ] && ! groups "${TARGET_USER}" | grep -q '\binput\b'; then
    sudo usermod -aG input "${TARGET_USER}"
    echo "Added ${TARGET_USER} to input group (logout/login required for full effect)"
fi

# ============================================================================
# UDEV & HWDB RULES
# ============================================================================

# Install udev rule:
# - Grant the active local user access via logind ACLs (TAG+="uaccess")
# - Also keep input group access (for setups that rely on group membership)
echo 'SUBSYSTEM=="input", ATTRS{name}=="*ASUS Zenbook Duo Keyboard", TAG+="uaccess", MODE="0660", GROUP="input"' | sudo tee /etc/udev/rules.d/90-zenbook-duo-keyboard.rules

# NOTE: We intentionally do NOT install a hwdb key remap for the Zenbook Duo
# keyboard. On USB, the keyboard exposes both consumer (media) scancodes and
# keyboard-page (F1-F12) scancodes depending on whether Fn is held. Installing
# a hwdb remap for KEYBOARD_KEY_7003* would override the Fn layer and make
# Fn+F keys behave like media keys.
#
# If you have an old version installed, remove its hwdb remap.
sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb

# Rebuild the hardware database and trigger udev to apply the new rules immediately
sudo systemd-hwdb update
sudo udevadm trigger

# ============================================================================
# UI DEFAULTS (settings.json)
# ============================================================================

# The optional Tauri UI reads settings from ~/.config/zenbook-duo/settings.json.
# We write defaults here so the UI is ready immediately after install.
PYTHON3=$(command -v python3 || true)
CONFIG_DIR="${TARGET_HOME}/.config/zenbook-duo"
SETTINGS_FILE="${CONFIG_DIR}/settings.json"

if [ -n "${PYTHON3}" ] && [ -n "${TARGET_HOME}" ]; then
    mkdir -p "${CONFIG_DIR}" 2>/dev/null || true

    # Write settings as the target user (avoid sudo dropping env vars).
    if [ "${TARGET_USER}" = "${USER}" ] && [ "${EUID}" != "0" ]; then
        "${PYTHON3}" - "${SETTINGS_FILE}" "${DEFAULT_BACKLIGHT}" "${DEFAULT_SCALE}" "${USB_MEDIA_REMAP_ENABLED}" <<'PY'
import json
import sys
from pathlib import Path

settings_file = Path(sys.argv[1])
default_backlight = int(sys.argv[2])
default_scale = float(sys.argv[3])
usb_media_remap_enabled = sys.argv[4].strip().lower() in ("1", "true", "yes", "y", "on")

settings_file.parent.mkdir(parents=True, exist_ok=True)

data = {}
if settings_file.exists():
    try:
        loaded = json.loads(settings_file.read_text())
        if isinstance(loaded, dict):
            data = loaded
    except Exception:
        data = {}

data.setdefault("autoDualScreen", True)
data.setdefault("syncBrightness", True)
data.setdefault("theme", "system")

data["defaultBacklight"] = default_backlight
data["defaultScale"] = default_scale
data["usbMediaRemapEnabled"] = usb_media_remap_enabled
data["setupCompleted"] = True

settings_file.write_text(json.dumps(data, indent=2) + "\n")
PY
    else
        sudo -u "${TARGET_USER}" "${PYTHON3}" - "${SETTINGS_FILE}" "${DEFAULT_BACKLIGHT}" "${DEFAULT_SCALE}" "${USB_MEDIA_REMAP_ENABLED}" <<'PY'
import json
import sys
from pathlib import Path

settings_file = Path(sys.argv[1])
default_backlight = int(sys.argv[2])
default_scale = float(sys.argv[3])
usb_media_remap_enabled = sys.argv[4].strip().lower() in ("1", "true", "yes", "y", "on")

settings_file.parent.mkdir(parents=True, exist_ok=True)

data = {}
if settings_file.exists():
    try:
        loaded = json.loads(settings_file.read_text())
        if isinstance(loaded, dict):
            data = loaded
    except Exception:
        data = {}

data.setdefault("autoDualScreen", True)
data.setdefault("syncBrightness", True)
data.setdefault("theme", "system")

data["defaultBacklight"] = default_backlight
data["defaultScale"] = default_scale
data["usbMediaRemapEnabled"] = usb_media_remap_enabled
data["setupCompleted"] = True

settings_file.write_text(json.dumps(data, indent=2) + "\n")
PY
    fi
fi

echo "Installing Rust runtime..."
"$(cd "$(dirname "$0")" && pwd)/install-rust-runtime.sh"

echo "Install complete."
