#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

source "${ROOT_DIR}/install.sh"

hyprland_output="$(
  (
    unset DESKTOP_SESSION XDG_SESSION_DESKTOP
    XDG_CURRENT_DESKTOP=vendor-hyprland-session pick_desktop
  )
)" || {
  echo "FAIL: Hyprland desktop detection should succeed" >&2
  exit 1
}

if [[ "${hyprland_output}" != "hyprland" ]]; then
  echo "FAIL: Hyprland desktop detection should output hyprland" >&2
  exit 1
fi

ambiguous_output="$(
  unset DESKTOP_SESSION XDG_SESSION_DESKTOP
  if XDG_CURRENT_DESKTOP=GNOME:Hyprland pick_desktop 2>&1; then
    echo "unexpected success"
    exit 1
  fi
)"

if [[ "${ambiguous_output}" == *"unexpected success"* ]]; then
  echo "FAIL: GNOME:Hyprland desktop detection should be ambiguous" >&2
  exit 1
fi

if [[ "${ambiguous_output}" != *"./setup-hyprland.sh"* ]]; then
  echo "FAIL: ambiguous desktop guidance should mention ./setup-hyprland.sh" >&2
  exit 1
fi

help_output="$(bash "${ROOT_DIR}/install.sh" --help 2>&1)" || {
  echo "FAIL: install.sh --help should succeed" >&2
  exit 1
}

if [[ "${help_output}" != *"Hyprland"* ]]; then
  echo "FAIL: help output should mention Hyprland support" >&2
  exit 1
fi

if [[ ! -f "${ROOT_DIR}/setup-hyprland.sh" ]]; then
  echo "FAIL: Hyprland detection should not be advertised without setup-hyprland.sh" >&2
  exit 1
fi

setup_script="$(pick_setup_script "${ROOT_DIR}" "hyprland")" || {
  echo "FAIL: pick_setup_script should accept hyprland" >&2
  exit 1
}

if [[ "${setup_script}" != "${ROOT_DIR}/setup-hyprland.sh" ]]; then
  echo "FAIL: hyprland should dispatch to setup-hyprland.sh" >&2
  exit 1
fi

echo "PASS"
