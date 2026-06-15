#!/usr/bin/env bash
# Installation script for ASUS Zenbook Duo Linux dual-screen management (GNOME).
# Installs dependencies, configures sudoers and udev rules, and installs the
# Rust runtime services.
set -euo pipefail

DUO_SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-${0}}")" && pwd)"
SETUP_SCRIPT_NAME="setup-gnome.sh"
DNF_DESKTOP_PACKAGES=(mutter dconf)
APT_DESKTOP_PACKAGES=(mutter-common-bin dconf-cli)
PACMAN_DESKTOP_PACKAGES=(mutter dconf)
MANUAL_DESKTOP_DEPENDENCIES_HINT="mutter/gdctl, dconf"

# shellcheck source=setup-common.sh
source "${DUO_SETUP_DIR}/setup-common.sh"
run_duo_setup "$@"
