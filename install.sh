#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
if [ -n "${SCRIPT_PATH}" ] && [ "${SCRIPT_PATH}" != "bash" ] && [ "${SCRIPT_PATH}" != "-" ] && [ -f "${SCRIPT_PATH}" ]; then
  SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
else
  SCRIPT_DIR="$(pwd)"
fi
REPO_URL="https://github.com/zakstam/zenbook-duo-linux.git"
ARCHIVE_URL="https://github.com/zakstam/zenbook-duo-linux/archive/refs/heads/main.tar.gz"

usage() {
  cat <<'EOF'
install.sh - unified installer for Zenbook Duo Linux

Usage:
  ./install.sh [setup-script-args...]

Examples:
  ./install.sh
  ./install.sh --no-usb-media-remap
  sudo -E ./install.sh

This script auto-detects GNOME, KDE Plasma, Hyprland, or Niri, runs the
matching setup script, then installs the optional Zenbook Duo Control UI.
EOF
}

ensure_repo_checkout() {
  if [ -f "${SCRIPT_DIR}/setup-gnome.sh" ] && [ -f "${SCRIPT_DIR}/install-ui.sh" ]; then
    printf '%s\n' "${SCRIPT_DIR}"
    return 0
  fi

  local temp_dir
  temp_dir="$(mktemp -d -t zenbook-duo-linux.XXXXXX)"

  if command -v git >/dev/null 2>&1; then
    echo "Downloading zenbook-duo-linux..."
    git clone --depth 1 "${REPO_URL}" "${temp_dir}" >/dev/null 2>&1
    printf '%s\n' "${temp_dir}"
    return 0
  fi

  if command -v curl >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
    echo "Downloading zenbook-duo-linux..."
    curl -fsSL "${ARCHIVE_URL}" | tar -xz -C "${temp_dir}" --strip-components=1
    printf '%s\n' "${temp_dir}"
    return 0
  fi

  echo "ERROR: Could not download the repository automatically." >&2
  echo "Install one of these first, then try again: git or (curl and tar)." >&2
  exit 1
}

lower() {
  printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]'
}

contains_token() {
  local haystack=" ${1:-} "
  local needle="${2:-}"
  [[ "${haystack}" == *" ${needle} "* ]]
}

pick_desktop() {
  local current_desktop_raw="${XDG_CURRENT_DESKTOP:-}"
  local desktop_session_raw="${DESKTOP_SESSION:-}"
  local session_desktop_raw="${XDG_SESSION_DESKTOP:-}"

  local current_desktop
  local desktop_session
  local session_desktop
  current_desktop="$(lower "${current_desktop_raw}")"
  desktop_session="$(lower "${desktop_session_raw}")"
  session_desktop="$(lower "${session_desktop_raw}")"

  local detected=()
  for value in "${current_desktop//:/ }" "${desktop_session}" "${session_desktop}"; do
    [ -n "${value}" ] || continue
    if contains_token "${value}" "gnome"; then
      detected+=("gnome")
    fi
    if contains_token "${value}" "kde" || contains_token "${value}" "plasma"; then
      detected+=("kde")
    fi
    if [[ "${value}" == *hyprland* ]]; then
      detected+=("hyprland")
    fi
    if contains_token "${value}" "niri"; then
      detected+=("niri")
    fi
  done

  local unique=()
  local item
  for item in "${detected[@]:-}"; do
    local seen=false
    local existing
    for existing in "${unique[@]:-}"; do
      if [ "${existing}" = "${item}" ]; then
        seen=true
        break
      fi
    done
    if [ "${seen}" = false ]; then
      unique+=("${item}")
    fi
  done

  if [ "${#unique[@]}" -eq 1 ]; then
    printf '%s\n' "${unique[0]}"
    return 0
  fi

  echo "ERROR: Could not confidently detect a supported desktop session." >&2
  echo "Detected environment values:" >&2
  echo "  XDG_CURRENT_DESKTOP=${current_desktop_raw:-<empty>}" >&2
  echo "  DESKTOP_SESSION=${desktop_session_raw:-<empty>}" >&2
  echo "  XDG_SESSION_DESKTOP=${session_desktop_raw:-<empty>}" >&2
  echo >&2
  echo "Run one of these manually instead:" >&2
  echo "  ./setup-gnome.sh" >&2
  echo "  ./setup-kde.sh" >&2
  echo "  ./setup-hyprland.sh" >&2
  echo "  ./setup-niri.sh" >&2
  return 1
}

pick_setup_script() {
  local repo_dir="${1:-}"
  local desktop="${2:-}"
  local setup_script=""

  case "${desktop}" in
    gnome)
      setup_script="${repo_dir}/setup-gnome.sh"
      ;;
    kde)
      setup_script="${repo_dir}/setup-kde.sh"
      ;;
    hyprland)
      setup_script="${repo_dir}/setup-hyprland.sh"
      ;;
    niri)
      setup_script="${repo_dir}/setup-niri.sh"
      ;;
    *)
      echo "ERROR: Unsupported desktop target: ${desktop}" >&2
      return 1
      ;;
  esac

  if [ ! -f "${setup_script}" ]; then
    echo "ERROR: Missing setup script for ${desktop}: ${setup_script}" >&2
    return 1
  fi

  printf '%s\n' "${setup_script}"
}

main() {
  if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
  fi

  desktop="$(pick_desktop)"
  repo_dir="$(ensure_repo_checkout)"
  setup_script="$(pick_setup_script "${repo_dir}" "${desktop}")"

  echo "Detected desktop: ${desktop}"
  echo "Running $(basename "${setup_script}")..."
  if [ -r /dev/tty ]; then
    "${setup_script}" "$@" </dev/tty
  else
    "${setup_script}" "$@"
  fi

  echo "Running install-ui.sh..."
  if [ -r /dev/tty ]; then
    "${repo_dir}/install-ui.sh" </dev/tty
  else
    "${repo_dir}/install-ui.sh"
  fi

  echo
  echo "If this is a fresh install, you may need to log out and back in."
  echo "If anything looks stale after updating, run: systemctl --user restart zenbook-duo-session-agent.service"
}

if [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then
  main "$@"
fi
