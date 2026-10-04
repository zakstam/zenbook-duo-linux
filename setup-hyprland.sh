#!/usr/bin/env bash
# Installation wrapper for the Hyprland backend. The optional snippet only
# imports the session environment; it never replaces monitor or keybind config.
set -euo pipefail

DUO_SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-${0}}")" && pwd)"
SETUP_SCRIPT_NAME="setup-hyprland.sh"
DNF_DESKTOP_PACKAGES=(hyprland)
APT_DESKTOP_PACKAGES=(hyprland)
PACMAN_DESKTOP_PACKAGES=(hyprland)
MANUAL_DESKTOP_DEPENDENCIES_HINT="hyprland"
INSTALL_HYPR_SNIPPET=true

for arg in "$@"; do
  case "${arg}" in
    --no-hyprland-snippet) INSTALL_HYPR_SNIPPET=false ;;
    --hyprland-snippet) INSTALL_HYPR_SNIPPET=true ;;
  esac
done

# shellcheck source=setup-common.sh
source "${DUO_SETUP_DIR}/setup-common.sh"
run_duo_setup "$@"

if [ "${INSTALL_HYPR_SNIPPET}" = true ]; then
  hypr_dir="${TARGET_HOME}/.config/hypr"
  main_config="${hypr_dir}/hyprland.conf"
  snippet="${hypr_dir}/zenbook-duo.conf"
  include_line='source = ~/.config/hypr/zenbook-duo.conf'
  run_as_target=()
  if [ "${TARGET_USER}" != "${USER:-}" ] || [ "${EUID}" = 0 ]; then
    run_as_target=(sudo -u "${TARGET_USER}")
  fi

  "${run_as_target[@]}" mkdir -p "${hypr_dir}"
  if [ -e "${snippet}" ]; then
    "${run_as_target[@]}" cp -a "${snippet}" "${snippet}.bak.$(date +%Y%m%d%H%M%S)"
  fi
  "${run_as_target[@]}" tee "${snippet}" >/dev/null <<'EOF'
# Zenbook Duo session environment. Safe for Hyprland and minimaLinux configs.
exec-once = systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE XDG_SESSION_TYPE
exec-once = dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE XDG_SESSION_TYPE
EOF

  if [ -f "${main_config}" ] && ! "${run_as_target[@]}" grep -Fqx "${include_line}" "${main_config}"; then
    "${run_as_target[@]}" cp -a "${main_config}" "${main_config}.bak.$(date +%Y%m%d%H%M%S)"
    printf '\n%s\n' "${include_line}" | "${run_as_target[@]}" tee -a "${main_config}" >/dev/null
  elif [ ! -f "${main_config}" ]; then
    echo "Hyprland snippet created at ${snippet}; source it once from your active config."
  fi
fi
