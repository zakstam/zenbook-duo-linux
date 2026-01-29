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

# Enable dev mode when --dev-mode flag is passed
if [ "${1}" = "--dev-mode" ]; then
    DEV_MODE=true
    INSTALL_LOCATION=${DEV_INSTALL_LOCATION}
fi

# ============================================================================
# PACKAGE INSTALLATION & SCRIPT DEPLOYMENT
# ============================================================================

# In normal (non-dev) mode: prompt for settings, install packages, and copy the script
if [ "${DEV_MODE}" = false ]; then
    # Prompt user for configuration preferences
    read -p "What would you like to use for the default keyboard backlight brightness [0-3]? " DEFAULT_BRIGHTNESS
    read -p "What would you like to use for monitor scale (1 = 100%, 1.5 = 150%, 1.66 = 166%, 2=200%) [1-2]? " DEFAULT_SCALE

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
PYTHON3=$(which python3)
if [ -n "${PYTHON3}" ] && [ -n "${USER}" ]; then
    addSudoers "${USER} ALL=NOPASSWD:${PYTHON3} /tmp/duo/backlight.py *"
    addSudoers "${USER} ALL=NOPASSWD:${PYTHON3} /tmp/duo/bt_backlight.py *"
    addSudoers "${USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/card1-eDP-2-backlight/brightness"
    addSudoers "${USER} ALL=NOPASSWD:${PYTHON3} /tmp/duo/inject_key.py *"
    addSudoers "${USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/intel_backlight/brightness"
fi

# ============================================================================
# USER & GROUP SETUP
# ============================================================================

# Add user to the "input" group so evtest can read keyboard input events
# without requiring root (requires logout/login to take effect)
if [ -n "${USER}" ] && ! groups "${USER}" | grep -q '\binput\b'; then
    sudo usermod -aG input "${USER}"
    echo "Added ${USER} to input group (logout/login required for full effect)"
fi

# ============================================================================
# UDEV & HWDB RULES
# ============================================================================

# Install udev rule: grant input group read/write access to the Zenbook Duo keyboard device nodes
echo 'SUBSYSTEM=="input", ATTRS{name}=="*ASUS Zenbook Duo Keyboard", MODE="0660", GROUP="input"' | sudo tee /etc/udev/rules.d/90-zenbook-duo-keyboard.rules

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

[Install]
WantedBy=multi-user.target
" | sudo tee /etc/systemd/system/zenbook-duo.service

# Install user-level systemd service for the main daemon.
# Runs after the graphical session starts and launches all background watchers.
echo "[Unit]
Description=Zenbook Duo User Handler
After=graphical-session.target

[Service]
ExecStart=${INSTALL_LOCATION}
Restart=no
Environment=XDG_CURRENT_DESKTOP=GNOME

[Install]
WantedBy=default.target
" | sudo tee /etc/systemd/user/zenbook-duo-user.service

# ============================================================================
# ENABLE & FINISH
# ============================================================================

# Reload systemd and enable both services
sudo systemctl daemon-reexec      # Reload system systemd manager
sudo systemctl daemon-reload      # Reload system unit files
sudo systemctl enable zenbook-duo.service  # Enable system-level boot/shutdown service
systemctl --user daemon-reexec    # Reload user systemd manager
systemctl --user daemon-reload    # Reload user unit files
sudo systemctl --global enable zenbook-duo-user.service  # Enable user-level service for all users

echo "Install complete."
