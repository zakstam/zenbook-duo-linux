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

DUO_GNOME_EXTENSION_UUID="zenbook-duo-snap@zenbook-duo-linux"

# Optional GNOME Shell extension (gnome-extension/): snap windows to either
# screen or across both while dragging, on-screen keyboard placement, and
# pop-ups for the Duo keys. Asked during setup; --gnome-extension or
# --no-gnome-extension answer it up front.
install_duo_desktop_extras() {
  local source_dir="${DUO_SETUP_DIR}/gnome-extension/${DUO_GNOME_EXTENSION_UUID}"
  [ -d "${source_dir}" ] || return 0

  if [ -z "${DUO_GNOME_EXTENSION:-}" ]; then
    local answer=""
    duo_prompt "Install the Zenbook Duo Snap GNOME extension (window snapping across the screens, touch keyboard placement, key pop-ups)? [Y/n] " answer
    case "${answer}" in
      [nN]|[nN][oO]) DUO_GNOME_EXTENSION=false ;;
      *) DUO_GNOME_EXTENSION=true ;;
    esac
  fi
  [ "${DUO_GNOME_EXTENSION}" = true ] || return 0

  local as_user=()
  if [ "${TARGET_USER}" != "${USER:-}" ] || [ "${EUID}" = "0" ]; then
    as_user=(sudo -u "${TARGET_USER}")
  fi
  local extensions_dir="${TARGET_HOME}/.local/share/gnome-shell/extensions"
  local target_dir="${extensions_dir}/${DUO_GNOME_EXTENSION_UUID}"
  "${as_user[@]}" mkdir -p "${extensions_dir}"
  "${as_user[@]}" rm -rf "${target_dir}"
  "${as_user[@]}" cp -r "${source_dir}" "${target_dir}"
  if command -v glib-compile-schemas >/dev/null 2>&1; then
    "${as_user[@]}" glib-compile-schemas "${target_dir}/schemas"
  fi

  # Enabled through the user's settings, so it loads at the next login.
  local bus="unix:path=/run/user/${TARGET_UID}/bus"
  if "${as_user[@]}" env DBUS_SESSION_BUS_ADDRESS="${bus}" \
    gnome-extensions enable "${DUO_GNOME_EXTENSION_UUID}" >/dev/null 2>&1 ||
    "${as_user[@]}" env DBUS_SESSION_BUS_ADDRESS="${bus}" python3 - "${DUO_GNOME_EXTENSION_UUID}" <<'PY'
import ast, subprocess, sys
uuid = sys.argv[1]
def get(key):
    out = subprocess.run(["gsettings", "get", "org.gnome.shell", key],
                         capture_output=True, text=True, check=True).stdout.strip()
    return list(ast.literal_eval(out.removeprefix("@as "))) if out != "@as []" else []
enabled, disabled = get("enabled-extensions"), get("disabled-extensions")
if uuid not in enabled:
    subprocess.run(["gsettings", "set", "org.gnome.shell", "enabled-extensions", str(enabled + [uuid])], check=True)
if uuid in disabled:
    subprocess.run(["gsettings", "set", "org.gnome.shell", "disabled-extensions", str([d for d in disabled if d != uuid])], check=True)
PY
  then
    echo "Zenbook Duo Snap extension installed; it loads after you log out and back in."
  else
    echo "Zenbook Duo Snap extension installed. Enable it after logging in:"
    echo "  gnome-extensions enable ${DUO_GNOME_EXTENSION_UUID}"
  fi
}

# shellcheck source=setup-common.sh
source "${DUO_SETUP_DIR}/setup-common.sh"
run_duo_setup "$@"
