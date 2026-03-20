#!/bin/bash
set -euo pipefail

# Installation script for ASUS Zenbook Duo Linux dual-screen management (Hyprland).
# Installs dependencies, configures sudoers and udev rules, writes UI defaults,
# and installs the Rust runtime services.

# ============================================================================
# CONFIGURATION & ARGUMENT PARSING
# ============================================================================

DEFAULT_BACKLIGHT=0
DEFAULT_SCALE=1.66
USB_MEDIA_REMAP_ENABLED=true
INSTALL_HYPR_SNIPPET=true

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
            shift
            ;;
    esac
done

# ============================================================================
# USER CONTEXT
# ============================================================================

TARGET_USER="${USER}"
if [ "${EUID}" = "0" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
        TARGET_USER="${SUDO_USER}"
    else
        echo "ERROR: setup-hyprland.sh must be run from a real user session."
        echo "Run: ./setup-hyprland.sh"
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

duo_prompt "What would you like to use for the default keyboard backlight brightness [0-3] (default: ${DEFAULT_BACKLIGHT})? " _input
DEFAULT_BACKLIGHT="${_input:-${DEFAULT_BACKLIGHT}}"
duo_prompt "What would you like to use for monitor scale (1 = 100%, 1.5 = 150%, 1.66 = 166%, 2=200%) (default: ${DEFAULT_SCALE})? " _input
DEFAULT_SCALE="${_input:-${DEFAULT_SCALE}}"
if [ "${USB_MEDIA_REMAP_ENABLED}" = "true" ]; then
    USB_MEDIA_REMAP_PROMPT='Enable USB Media Remap by default? [Y/n] '
else
    USB_MEDIA_REMAP_PROMPT='Enable USB Media Remap by default? [y/N] '
fi
duo_prompt "${USB_MEDIA_REMAP_PROMPT}" ENABLE_USB_MEDIA_REMAP_ANSWER
case "${ENABLE_USB_MEDIA_REMAP_ANSWER}" in
    "")
        # Keep the current value so explicit CLI flags survive an Enter press.
        ;;
    [nN]|[nN][oO])
        USB_MEDIA_REMAP_ENABLED=false
        ;;
    *)
        USB_MEDIA_REMAP_ENABLED=true
        ;;
esac
duo_prompt "Install the optional Hyprland session helper snippet? [Y/n] " INSTALL_HYPR_SNIPPET_ANSWER
case "${INSTALL_HYPR_SNIPPET_ANSWER}" in
    [nN]|[nN][oO])
        INSTALL_HYPR_SNIPPET=false
        ;;
    *)
        INSTALL_HYPR_SNIPPET=true
        ;;
esac

if command -v dnf &>/dev/null; then
    sudo dnf install -y \
        usbutils \
        hyprland \
        iio-sensor-proxy \
        systemd
elif command -v pacman &>/dev/null; then
    sudo pacman -S --needed --noconfirm \
        usbutils \
        hyprland \
        iio-sensor-proxy \
        systemd
elif command -v apt &>/dev/null; then
    sudo apt install -y \
        usbutils \
        hyprland \
        iio-sensor-proxy \
        systemd
else
    echo "Unsupported package manager. Please install dependencies manually:"
    echo "  usbutils, hyprland, iio-sensor-proxy, systemd"
    exit 1
fi

# ============================================================================
# SUDOERS CONFIGURATION
# ============================================================================

function addSudoers() {
    RESULT=$(sudo grep "${1}" /etc/sudoers || true)
    if [ -z "${RESULT}" ]; then
        echo "${1}" | sudo tee -a /etc/sudoers
    fi
}

if [ -n "${TARGET_USER}" ]; then
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/card1-eDP-2-backlight/brightness"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/intel_backlight/brightness"
fi

# ============================================================================
# USER & GROUP SETUP
# ============================================================================

if [ -n "${TARGET_USER}" ] && ! groups "${TARGET_USER}" | grep -q '\binput\b'; then
    sudo usermod -aG input "${TARGET_USER}"
    echo "Added ${TARGET_USER} to input group (logout/login required for full effect)"
fi

# ============================================================================
# UDEV & HWDB RULES
# ============================================================================

echo 'SUBSYSTEM=="input", ATTRS{name}=="*ASUS Zenbook Duo Keyboard", TAG+="uaccess", MODE="0660", GROUP="input"' | sudo tee /etc/udev/rules.d/90-zenbook-duo-keyboard.rules

sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb
sudo systemd-hwdb update
sudo udevadm trigger

# ============================================================================
# UI DEFAULTS (settings.json)
# ============================================================================

PYTHON3=$(command -v python3 || true)
CONFIG_DIR="${TARGET_HOME}/.config/zenbook-duo"
SETTINGS_FILE="${CONFIG_DIR}/settings.json"

if [ -n "${PYTHON3}" ] && [ -n "${TARGET_HOME}" ]; then
    mkdir -p "${CONFIG_DIR}" 2>/dev/null || true

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

# ============================================================================
# OPTIONAL HYPRLAND SESSION SNIPPET
# ============================================================================

if [ "${INSTALL_HYPR_SNIPPET}" = "true" ]; then
    HYPR_DIR="${TARGET_HOME}/.config/hypr"
    HYPR_MAIN_CONFIG="${HYPR_DIR}/hyprland.conf"
    HYPR_SNIPPET="${HYPR_DIR}/zenbook-duo.conf"
    HYPR_INCLUDE_LINE="source = ~/.config/hypr/zenbook-duo.conf"

    if sudo -u "${TARGET_USER}" mkdir -p "${HYPR_DIR}"; then
        if sudo -u "${TARGET_USER}" tee "${HYPR_SNIPPET}" >/dev/null <<'EOF'; then
# Zenbook Duo Hyprland session glue.
# Safe to remove if you prefer to manage the session agent another way.
exec-once = systemctl --user import-environment WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION
exec-once = dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION
exec-once = systemctl --user start zenbook-duo-session-agent.service
EOF
            if [ -f "${HYPR_MAIN_CONFIG}" ]; then
                if ! sudo -u "${TARGET_USER}" grep -Fqx "${HYPR_INCLUDE_LINE}" "${HYPR_MAIN_CONFIG}"; then
                    printf '\n%s\n' "${HYPR_INCLUDE_LINE}" | sudo -u "${TARGET_USER}" tee -a "${HYPR_MAIN_CONFIG}" >/dev/null
                fi
            else
                echo "WARNING: ${HYPR_MAIN_CONFIG} was not found; created ${HYPR_SNIPPET} but did not add an include line."
            fi
        else
            echo "WARNING: failed to install optional Hyprland snippet at ${HYPR_SNIPPET}."
        fi
    else
        echo "WARNING: failed to create ${HYPR_DIR}; skipping optional Hyprland snippet."
    fi
fi

echo "Installing Rust runtime..."
"$(cd "$(dirname "$0")" && pwd)/install-rust-runtime.sh"

echo "Install complete."
