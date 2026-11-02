#!/usr/bin/env bash
# Installation wrapper for the Hyprland backend. The optional snippet only
# imports the session environment; it never replaces monitor or keybind config.
# --snippet-only writes just the snippet: install.sh uses it when Hyprland is
# installed but the install runs from another desktop.
set -euo pipefail

DUO_SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-${0}}")" && pwd)"
SETUP_SCRIPT_NAME="setup-hyprland.sh"
DNF_DESKTOP_PACKAGES=(hyprland)
APT_DESKTOP_PACKAGES=(hyprland)
PACMAN_DESKTOP_PACKAGES=(hyprland)
MANUAL_DESKTOP_DEPENDENCIES_HINT="hyprland"
INSTALL_HYPR_SNIPPET=true
SNIPPET_ONLY=false

for arg in "$@"; do
  case "${arg}" in
    --no-hyprland-snippet) INSTALL_HYPR_SNIPPET=false ;;
    --hyprland-snippet) INSTALL_HYPR_SNIPPET=true ;;
    --snippet-only) SNIPPET_ONLY=true ;;
  esac
done

# shellcheck source=setup-common.sh
source "${DUO_SETUP_DIR}/setup-common.sh"

# Appends include_line to config unless it is already there, keeping a backup.
add_hyprland_include() {
  config="$1"
  include_line="$2"
  if ! "${run_as_target[@]}" grep -Fqx "${include_line}" "${config}"; then
    "${run_as_target[@]}" cp -a "${config}" "${config}.bak.$(date +%Y%m%d%H%M%S)"
    printf '\n%s\n' "${include_line}" | "${run_as_target[@]}" tee -a "${config}" >/dev/null
  fi
}

# Writes the snippet in both config languages: hyprland.lua (Hyprland 0.57+,
# which dropped hyprland.conf) and hyprland.conf (older Hyprland, minimaLinux).
install_hyprland_snippet() {
  hypr_dir="${TARGET_HOME}/.config/hypr"
  conf_config="${hypr_dir}/hyprland.conf"
  lua_config="${hypr_dir}/hyprland.lua"
  conf_snippet="${hypr_dir}/zenbook-duo.conf"
  lua_snippet="${hypr_dir}/zenbook-duo.lua"
  run_as_target=()
  if [ "${TARGET_USER}" != "${USER:-}" ] || [ "${EUID}" = 0 ]; then
    run_as_target=(sudo -u "${TARGET_USER}")
  fi

  "${run_as_target[@]}" mkdir -p "${hypr_dir}"
  for snippet in "${conf_snippet}" "${lua_snippet}"; do
    if [ -e "${snippet}" ]; then
      "${run_as_target[@]}" cp -a "${snippet}" "${snippet}.bak.$(date +%Y%m%d%H%M%S)"
    fi
  done
  "${run_as_target[@]}" tee "${conf_snippet}" >/dev/null <<'EOF'
# Zenbook Duo session environment. Safe for Hyprland and minimaLinux configs.
exec-once = systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION HYPRLAND_INSTANCE_SIGNATURE XDG_SESSION_TYPE
exec-once = dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION HYPRLAND_INSTANCE_SIGNATURE XDG_SESSION_TYPE
EOF
  "${run_as_target[@]}" tee "${lua_snippet}" >/dev/null <<'EOF'
-- Zenbook Duo session environment for Hyprland 0.57+ (Lua config).
local vars = "WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION HYPRLAND_INSTANCE_SIGNATURE XDG_SESSION_TYPE"

hl.on("hyprland.start", function()
    hl.exec_cmd("systemctl --user import-environment " .. vars)
    hl.exec_cmd("dbus-update-activation-environment --systemd " .. vars)
end)
EOF

  if [ -f "${lua_config}" ]; then
    add_hyprland_include "${lua_config}" 'require("zenbook-duo")'
  fi
  if [ -f "${conf_config}" ]; then
    add_hyprland_include "${conf_config}" 'source = ~/.config/hypr/zenbook-duo.conf'
  fi
  if [ ! -f "${lua_config}" ] && [ ! -f "${conf_config}" ]; then
    echo "Hyprland snippets created at ${lua_snippet} and ${conf_snippet}; load one from your active config."
  fi
}

if [ "${SNIPPET_ONLY}" = true ]; then
  resolve_duo_target_user
  install_hyprland_snippet
  echo "Hyprland session snippet installed."
  exit 0
fi

run_duo_setup "$@"

if [ "${INSTALL_HYPR_SNIPPET}" = true ]; then
  install_hyprland_snippet
fi
