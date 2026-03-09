#!/bin/bash
# Installation script for ASUS Zenbook Duo Linux dual-screen management (Hyprland / Arch Linux).

INSTALL_LOCATION=/usr/local/bin/duo
DEV_MODE=false
DEV_INSTALL_LOCATION=$(cd "$(dirname "$0")" && pwd)/duo.sh
DEFAULT_BACKLIGHT=0
DEFAULT_SCALE=1.66
USB_MEDIA_REMAP_ENABLED=true

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dev-mode) DEV_MODE=true; INSTALL_LOCATION=${DEV_INSTALL_LOCATION}; shift ;;
        --usb-media-remap) USB_MEDIA_REMAP_ENABLED=true; shift ;;
        --no-usb-media-remap) USB_MEDIA_REMAP_ENABLED=false; shift ;;
        *) shift ;;
    esac
done

TARGET_USER="${USER}"
if [ "${EUID}" = "0" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
        TARGET_USER="${SUDO_USER}"
    else
        echo "ERROR: setup-hyprland.sh must be run from a real user session."
        exit 1
    fi
fi

TARGET_UID="$(id -u "${TARGET_USER}" 2>/dev/null || true)"
TARGET_HOME="$(getent passwd "${TARGET_USER}" 2>/dev/null | cut -d: -f6)"

function run_user_systemctl() {
    if [ "${TARGET_USER}" = "${USER}" ] && [ "${EUID}" != "0" ]; then
        systemctl --user "$@"
        return
    fi
    sudo -u "${TARGET_USER}" \
        XDG_RUNTIME_DIR="/run/user/${TARGET_UID}" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${TARGET_UID}/bus" \
        systemctl --user "$@"
}

if [ "${DEV_MODE}" = false ]; then
    read -p "What would you like to use for the default keyboard backlight brightness [0-3] (default: ${DEFAULT_BACKLIGHT})? " _input
    DEFAULT_BACKLIGHT="${_input:-${DEFAULT_BACKLIGHT}}"
    read -p "What would you like to use for monitor scale (1 = 100%, 1.5 = 150%) (default: ${DEFAULT_SCALE})? " _input
    DEFAULT_SCALE="${_input:-${DEFAULT_SCALE}}"

    # Installation des dépendances via pacman (Arch Linux)
    echo "Installing dependencies via pacman..."
    sudo pacman -S --needed inotify-tools usbutils hyprland iio-sensor-proxy python-pyusb evtest jq

    sudo mkdir -p /usr/local/bin
    sudo cp ./duo.sh ${INSTALL_LOCATION}
    sudo chmod a+x ${INSTALL_LOCATION}
    sudo sed -i "s/^DEFAULT_BACKLIGHT=.*/DEFAULT_BACKLIGHT=${DEFAULT_BACKLIGHT}/" ${INSTALL_LOCATION}
    sudo sed -i "s/^DEFAULT_SCALE=.*/DEFAULT_SCALE=${DEFAULT_SCALE}/" ${INSTALL_LOCATION}

    sudo mkdir -p /usr/local/libexec/zenbook-duo
    sudo cp ./libexec/backlight.py /usr/local/libexec/zenbook-duo/backlight.py
    sudo cp ./libexec/bt_backlight.py /usr/local/libexec/zenbook-duo/bt_backlight.py
    sudo cp ./libexec/inject_key.py /usr/local/libexec/zenbook-duo/inject_key.py
    # Utilisation du nouveau script hyprland
    sudo cp ./libexec/display-hyprland.sh /usr/local/libexec/zenbook-duo/display-hyprland.sh
    sudo chmod 0644 /usr/local/libexec/zenbook-duo/*.py /usr/local/libexec/zenbook-duo/*.sh
fi

function addSudoers() {
    RESULT=$(sudo grep "${1}" /etc/sudoers)
    if [ -z "${RESULT}" ]; then echo "${1}" | sudo tee -a /etc/sudoers; fi
}

PYTHON3=$(command -v python3 2>/dev/null || true)
if [ -n "${PYTHON3}" ] && [ -n "${TARGET_USER}" ]; then
    addSudoers "${TARGET_USER} ALL=NOPASSWD:${PYTHON3} /usr/local/libexec/zenbook-duo/backlight.py *"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:${PYTHON3} /usr/local/libexec/zenbook-duo/bt_backlight.py *"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/card1-eDP-2-backlight/brightness"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:${PYTHON3} /usr/local/libexec/zenbook-duo/inject_key.py *"
    addSudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/intel_backlight/brightness"
fi

if [ -n "${TARGET_USER}" ] && ! groups "${TARGET_USER}" | grep -q '\binput\b'; then
    sudo usermod -aG input "${TARGET_USER}"
fi

echo 'SUBSYSTEM=="input", ATTRS{name}=="*ASUS Zenbook Duo Keyboard", TAG+="uaccess", MODE="0660", GROUP="input"' | sudo tee /etc/udev/rules.d/90-zenbook-duo-keyboard.rules
sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb
sudo systemd-hwdb update
sudo udevadm trigger

sudo rm -f /usr/lib/systemd/system-sleep/duo
sudo ln -s ${INSTALL_LOCATION} /usr/lib/systemd/system-sleep/duo

echo "[Unit]
Description=Zenbook Duo Power Events Handler
Before=shutdown.target
After=multi-user.target
[Service]
Type=oneshot
ExecStart=${INSTALL_LOCATION} boot
ExecStop=${INSTALL_LOCATION} shutdown
RemainAfterExit=yes
[Install]
WantedBy=multi-user.target" | sudo tee /etc/systemd/system/zenbook-duo.service

echo "[Unit]
Description=Zenbook Duo User Handler
Wants=graphical-session.target
After=graphical-session.target
[Service]
Type=simple
ExecStart=${INSTALL_LOCATION}
Restart=on-failure
RestartSec=1
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus
[Install]
WantedBy=graphical-session.target" | sudo tee /etc/systemd/user/zenbook-duo-user.service

sudo systemctl daemon-reload
sudo systemctl enable zenbook-duo.service
run_user_systemctl daemon-reload
sudo systemctl --global disable zenbook-duo-user.service 2>/dev/null || true
run_user_systemctl enable zenbook-duo-user.service
run_user_systemctl restart zenbook-duo-user.service

echo "Install complete for Arch Linux (Hyprland)."
