#!/bin/bash
# Installation script for ASUS Zenbook Duo Linux dual-screen management.
# Installs dependencies, configures sudoers, udev rules, hwdb key remapping,
# and systemd services for automatic startup.

# ============================================================================
# CONFIGURATION & ARGUMENT PARSING
# ============================================================================

# Path where the main duo script will be installed
INSTALL_LOCATION=/usr/local/bin/duo

# Dev mode skips package installation and uses the local script directly
DEV_MODE=false
DEV_INSTALL_LOCATION=$(cd "$(dirname "$0")" && pwd)/duo.sh

# Default configuration values
DEFAULT_BACKLIGHT=0
DEFAULT_SCALE=1.66
USB_MEDIA_REMAP_ENABLED=true

# Flags:
# --dev-mode: skip package installation and use the local duo.sh directly
# --usb-media-remap / --no-usb-media-remap: default setting written for the UI (default: enabled)
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dev-mode)
            DEV_MODE=true
            INSTALL_LOCATION=${DEV_INSTALL_LOCATION}
            shift
            ;;
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

# setup.sh is frequently run with `sudo`. In that case we still want all
# per-user config (sudoers, input group, systemctl --user enable, settings.json)
# to apply to the *real* user session, not root.
TARGET_USER="${USER}"
if [ "${EUID}" = "0" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
        TARGET_USER="${SUDO_USER}"
    else
        echo "ERROR: setup.sh must be run from a real user session."
        echo "Run: ./setup.sh"
        exit 1
    fi
fi

TARGET_UID="$(id -u "${TARGET_USER}" 2>/dev/null || true)"
TARGET_HOME="$(getent passwd "${TARGET_USER}" 2>/dev/null | cut -d: -f6)"
if [ -z "${TARGET_UID}" ] || [ -z "${TARGET_HOME}" ]; then
    echo "ERROR: failed to resolve TARGET_USER=${TARGET_USER}"
    exit 1
fi

function run_user_systemctl() {
    if [ "${TARGET_USER}" = "${USER}" ] && [ "${EUID}" != "0" ]; then
        systemctl --user "$@"
        return
    fi

    # Best-effort: use the login session bus.
    sudo -u "${TARGET_USER}" \
        XDG_RUNTIME_DIR="/run/user/${TARGET_UID}" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${TARGET_UID}/bus" \
        systemctl --user "$@"
}

# ============================================================================
# PACKAGE INSTALLATION & SCRIPT DEPLOYMENT
# ============================================================================

# In normal (non-dev) mode: prompt for settings, install packages, and copy the script
if [ "${DEV_MODE}" = false ]; then
    # Prompt user for configuration preferences
    read -p "What would you like to use for the default keyboard backlight brightness [0-3]? " DEFAULT_BACKLIGHT
    read -p "What would you like to use for monitor scale (1 = 100%, 1.5 = 150%, 1.66 = 166%, 2=200%) [1-2]? " DEFAULT_SCALE
    read -p "Enable USB Media Remap by default? [Y/n] " ENABLE_USB_MEDIA_REMAP_ANSWER
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
        # Fedora / RHEL-based
        sudo dnf install -y \
            inotify-tools \
            usbutils \
            mutter \
            iio-sensor-proxy \
            python3-pyusb \
            evtest
    elif command -v apt &>/dev/null; then
        # Debian / Ubuntu-based
        sudo apt install -y \
            inotify-tools \
            usbutils \
            mutter-common-bin \
            iio-sensor-proxy \
            python3-usb \
            evtest
    else
        echo "Unsupported package manager. Please install dependencies manually:"
        echo "  inotify-tools, usbutils, mutter/gdctl, iio-sensor-proxy, python3-usb/pyusb"
        exit 1
    fi

	    # Copy the main script to the install location and apply user-chosen defaults
	    sudo mkdir -p /usr/local/bin
	    sudo cp ./duo.sh ${INSTALL_LOCATION}
	    sudo chmod a+x ${INSTALL_LOCATION}
	    sudo sed -i "s/^DEFAULT_BACKLIGHT=.*/DEFAULT_BACKLIGHT=${DEFAULT_BACKLIGHT}/" ${INSTALL_LOCATION}
	    sudo sed -i "s/^DEFAULT_SCALE=.*/DEFAULT_SCALE=${DEFAULT_SCALE}/" ${INSTALL_LOCATION}

	    # Install helper scripts (invoked via sudoers) to a stable root-owned location.
	    sudo mkdir -p /usr/local/libexec/zenbook-duo
	    sudo cp ./libexec/backlight.py /usr/local/libexec/zenbook-duo/backlight.py
	    sudo cp ./libexec/bt_backlight.py /usr/local/libexec/zenbook-duo/bt_backlight.py
	    sudo cp ./libexec/inject_key.py /usr/local/libexec/zenbook-duo/inject_key.py
	    sudo chmod 0644 /usr/local/libexec/zenbook-duo/backlight.py \
	        /usr/local/libexec/zenbook-duo/bt_backlight.py \
	        /usr/local/libexec/zenbook-duo/inject_key.py
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

# Configure passwordless sudo for commands that require root access:
# - USB/BT backlight control scripts
# - Display brightness sysfs writes
# - Virtual key injection script
if [ -x /usr/bin/python3 ]; then
    PYTHON3=/usr/bin/python3
else
    PYTHON3=$(command -v python3 2>/dev/null || true)
fi
if [ -n "${PYTHON3}" ] && [ -n "${TARGET_USER}" ]; then
    addSudoers "${TARGET_USER} ALL=NOPASSWD:${PYTHON3} /usr/local/libexec/zenbook-duo/backlight.py *"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:${PYTHON3} /usr/local/libexec/zenbook-duo/bt_backlight.py *"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/card1-eDP-2-backlight/brightness"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:${PYTHON3} /usr/local/libexec/zenbook-duo/inject_key.py *"
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
# SYSTEMD SERVICES
# ============================================================================

# Create a symlink in the systemd sleep hook directory so duo.sh is called
# with "pre"/"post" arguments on suspend/resume events.
# Uses /usr/lib path (works on both Fedora and distros where /lib -> /usr/lib)
sudo rm -f /usr/lib/systemd/system-sleep/duo
sudo ln -s ${INSTALL_LOCATION} /usr/lib/systemd/system-sleep/duo

# Install system-level systemd service for boot/shutdown events.
# Runs "duo boot" on startup and "duo shutdown" on system halt.
echo "[Unit]
Description=Zenbook Duo Power Events Handler (boot & shutdown)
DefaultDependencies=no
Before=shutdown.target
After=multi-user.target

[Service]
Type=oneshot
ExecStart=${INSTALL_LOCATION} boot
ExecStop=${INSTALL_LOCATION} shutdown
RemainAfterExit=yes
TimeoutStartSec=10
TimeoutStopSec=10

[Install]
WantedBy=multi-user.target
" | sudo tee /etc/systemd/system/zenbook-duo.service

# Install user-level systemd service for the main daemon.
# Runs after the graphical session starts and launches all background watchers.
echo "[Unit]
Description=Zenbook Duo User Handler
ConditionUser=!gdm
Wants=graphical-session.target
After=graphical-session.target

[Service]
Type=simple
ExecStart=${INSTALL_LOCATION}
Restart=on-failure
RestartSec=1
TimeoutStopSec=5
KillMode=control-group
KillSignal=SIGTERM
Environment=XDG_CURRENT_DESKTOP=GNOME
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus

[Install]
WantedBy=graphical-session.target
" | sudo tee /etc/systemd/user/zenbook-duo-user.service

# ============================================================================
# ENABLE & FINISH
# ============================================================================

# Reload systemd and enable both services
sudo systemctl daemon-reexec      # Reload system systemd manager
sudo systemctl daemon-reload      # Reload system unit files
sudo systemctl enable zenbook-duo.service  # Enable system-level boot/shutdown service
run_user_systemctl daemon-reexec  # Reload user systemd manager
run_user_systemctl daemon-reload  # Reload user unit files
# Older installs enabled the user service globally, which also starts it under `gdm`.
# Disable that so only the real logged-in user runs the daemon.
sudo systemctl --global disable zenbook-duo-user.service 2>/dev/null || true
run_user_systemctl enable --now zenbook-duo-user.service  # Enable + start user-level service for the current user

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

echo "Install complete."
