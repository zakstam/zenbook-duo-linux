#!/usr/bin/env bash
# Installation script for ASUS Zenbook Duo Linux dual-screen management (GNOME).
# Installs dependencies, configures sudoers and udev rules, and installs the
# Rust runtime services.
set -euo pipefail

DUO_SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-${0}}")" && pwd)"
SETUP_SCRIPT_NAME="setup-gnome.sh"
DEFAULT_BACKLIGHT=0
DEFAULT_SCALE=1.66
USB_MEDIA_REMAP_ENABLED=true
DNF_PACKAGES=(usbutils mutter iio-sensor-proxy systemd)
APT_PACKAGES=(usbutils mutter-common-bin iio-sensor-proxy systemd)
PACMAN_PACKAGES=(usbutils mutter iio-sensor-proxy systemd)
MANUAL_DEPENDENCIES_HINT="usbutils, mutter/gdctl, iio-sensor-proxy, systemd"

# shellcheck source=setup-common.sh
source "${DUO_SETUP_DIR}/setup-common.sh"
run_duo_setup "$@"
