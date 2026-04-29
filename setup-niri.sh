#!/usr/bin/env bash
# Installation script for ASUS Zenbook Duo Linux dual-screen management (Niri).
# Installs dependencies, configures sudoers and udev rules, and installs the
# Rust runtime services.
set -euo pipefail

DUO_SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-${0}}")" && pwd)"
SETUP_SCRIPT_NAME="setup-niri.sh"
DNF_DESKTOP_PACKAGES=(niri)
APT_DESKTOP_PACKAGES=(niri)
PACMAN_DESKTOP_PACKAGES=(niri)
MANUAL_DESKTOP_DEPENDENCIES_HINT="niri"

# shellcheck source=setup-common.sh
source "${DUO_SETUP_DIR}/setup-common.sh"
run_duo_setup "$@"
