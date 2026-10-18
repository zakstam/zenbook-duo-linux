#!/usr/bin/env bash
# Shared setup flow for Zenbook Duo desktop backends.
# Desktop-specific wrappers declare only their backend dependency deltas and then
# call run_duo_setup.

DUO_COMMON_DNF_PACKAGES=(usbutils iio-sensor-proxy systemd upower pulseaudio-utils)
DUO_COMMON_APT_PACKAGES=(usbutils iio-sensor-proxy systemd upower pulseaudio-utils)
DUO_COMMON_PACMAN_PACKAGES=(usbutils iio-sensor-proxy systemd upower libpulse)
DUO_COMMON_MANUAL_DEPENDENCIES_HINT="usbutils, iio-sensor-proxy, systemd, upower, pactl (pulseaudio-utils)"

# Keep these defaults aligned with DuoSettings::default in the Rust runtime and
# DEFAULT_DUO_SETTINGS in the frontend.
DUO_DEFAULT_BACKLIGHT=0
# 1.67 stands for 5/3 (1728x1080 on the 2880x1800 panels); the app stores it as 5/3.
# Offer only scales on the 1/120 grid that divide 2880x1800 evenly, or desktops change them.
DUO_DEFAULT_SCALE=1.67
DUO_DEFAULT_USB_MEDIA_REMAP_ENABLED=true
DUO_DEFAULT_START_ON_BOOT_MINIMIZED=false
DUO_INSTALLER_MARKS_SETUP_COMPLETED=true

build_duo_setup_package_lists() {
  DNF_DESKTOP_PACKAGES=("${DNF_DESKTOP_PACKAGES[@]:-}")
  APT_DESKTOP_PACKAGES=("${APT_DESKTOP_PACKAGES[@]:-}")
  PACMAN_DESKTOP_PACKAGES=("${PACMAN_DESKTOP_PACKAGES[@]:-}")

  DNF_PACKAGES=("${DUO_COMMON_DNF_PACKAGES[@]}" "${DNF_DESKTOP_PACKAGES[@]}")
  APT_PACKAGES=("${DUO_COMMON_APT_PACKAGES[@]}" "${APT_DESKTOP_PACKAGES[@]}")
  PACMAN_PACKAGES=("${DUO_COMMON_PACMAN_PACKAGES[@]}" "${PACMAN_DESKTOP_PACKAGES[@]}")

  if [ -n "${MANUAL_DESKTOP_DEPENDENCIES_HINT:-}" ]; then
    MANUAL_DEPENDENCIES_HINT="${DUO_COMMON_MANUAL_DEPENDENCIES_HINT}, ${MANUAL_DESKTOP_DEPENDENCIES_HINT}"
  else
    MANUAL_DEPENDENCIES_HINT="${DUO_COMMON_MANUAL_DEPENDENCIES_HINT}"
  fi
}

run_duo_setup() {
  : "${SETUP_SCRIPT_NAME:?SETUP_SCRIPT_NAME is required}"

  # Values given in the environment win over the saved ones (see load_duo_saved_defaults).
  DUO_GIVEN_BACKLIGHT="${DEFAULT_BACKLIGHT:+1}"
  DUO_GIVEN_SCALE="${DEFAULT_SCALE:+1}"
  DUO_GIVEN_REMAP="${USB_MEDIA_REMAP_ENABLED:+1}"
  DEFAULT_BACKLIGHT="${DEFAULT_BACKLIGHT:-${DUO_DEFAULT_BACKLIGHT}}"
  DEFAULT_SCALE="${DEFAULT_SCALE:-${DUO_DEFAULT_SCALE}}"
  USB_MEDIA_REMAP_ENABLED="${USB_MEDIA_REMAP_ENABLED:-${DUO_DEFAULT_USB_MEDIA_REMAP_ENABLED}}"
  START_ON_BOOT_MINIMIZED="${START_ON_BOOT_MINIMIZED:-${DUO_DEFAULT_START_ON_BOOT_MINIMIZED}}"

  build_duo_setup_package_lists

  parse_duo_setup_args "$@"
  resolve_duo_target_user
  load_duo_saved_defaults
  prompt_duo_defaults
  install_duo_dependencies
  configure_duo_sudoers
  configure_duo_input_access
  configure_duo_libinput_quirks
  write_duo_settings_defaults
  install_duo_rust_runtime

  echo "Install complete."
}

parse_duo_setup_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --usb-media-remap)
        USB_MEDIA_REMAP_ENABLED=true
        DUO_GIVEN_REMAP=1
        shift
        ;;
      --no-usb-media-remap)
        USB_MEDIA_REMAP_ENABLED=false
        DUO_GIVEN_REMAP=1
        shift
        ;;
      *)
        # Unknown flag/arg - ignore to keep backwards compatibility.
        shift
        ;;
    esac
  done
}

resolve_duo_target_user() {
  TARGET_USER="${USER:-}"
  if [ -z "${TARGET_USER}" ]; then
    TARGET_USER="$(id -un 2>/dev/null || true)"
  fi

  if [ "${EUID}" = "0" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
      TARGET_USER="${SUDO_USER}"
    else
      echo "ERROR: ${SETUP_SCRIPT_NAME} must be run from a real user session."
      echo "Run: ./${SETUP_SCRIPT_NAME}"
      exit 1
    fi
  fi

  TARGET_UID="$(id -u "${TARGET_USER}" 2>/dev/null || true)"
  TARGET_HOME="$(getent passwd "${TARGET_USER}" 2>/dev/null | cut -d: -f6)"
  if [ -z "${TARGET_UID}" ] || [ -z "${TARGET_HOME}" ]; then
    echo "ERROR: failed to resolve TARGET_USER=${TARGET_USER}"
    exit 1
  fi
}

# On a reinstall the questions default to the saved settings, so pressing
# Enter keeps them; the installer's own defaults only apply to a first
# install. Values given in the environment or as flags still win.
load_duo_saved_defaults() {
  local settings_file="${TARGET_HOME:-}/.config/zenbook-duo/settings.json"
  [ -r "${settings_file}" ] || return 0
  command -v python3 >/dev/null 2>&1 || return 0
  local saved
  saved="$(python3 - "${settings_file}" <<'PY' 2>/dev/null
import json, sys
try:
    data = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(0)
def show(key, value):
    if value is not None:
        print(f"{key}={value}")
backlight = data.get("defaultBacklight")
show("backlight", backlight if isinstance(backlight, int) and 0 <= backlight <= 3 else None)
scale = data.get("defaultScale")
if isinstance(scale, (int, float)) and 0.5 <= scale <= 4:
    # Saved as 5/3 or 4/3; asked as 1.67 or 1.33. Retired 1.66 and 1.75 become 1.67.
    scale = round(scale, 2)
    if abs(scale - 1.66) < 0.005 or abs(scale - 1.75) < 0.005:
        scale = 1.67
    show("scale", f"{scale:g}")
remap = data.get("usbMediaRemapEnabled")
show("remap", ("true" if remap else "false") if isinstance(remap, bool) else None)
PY
)" || return 0
  local line
  while IFS= read -r line; do
    case "${line}" in
      backlight=*) [ -n "${DUO_GIVEN_BACKLIGHT:-}" ] || DEFAULT_BACKLIGHT="${line#*=}" ;;
      scale=*) [ -n "${DUO_GIVEN_SCALE:-}" ] || DEFAULT_SCALE="${line#*=}" ;;
      remap=*) [ -n "${DUO_GIVEN_REMAP:-}" ] || USB_MEDIA_REMAP_ENABLED="${line#*=}" ;;
    esac
  done <<< "${saved}"
}

duo_prompt() {
  local prompt="$1"
  local reply_var="$2"
  local value=""
  if [ -r /dev/tty ]; then
    read -r -p "${prompt}" value </dev/tty
  else
    read -r -p "${prompt}" value
  fi
  printf -v "${reply_var}" '%s' "${value}"
}

prompt_duo_defaults() {
  local input=""
  duo_prompt "What would you like to use for the default keyboard backlight brightness [0-3] (default: ${DEFAULT_BACKLIGHT})? " input
  DEFAULT_BACKLIGHT="${input:-${DEFAULT_BACKLIGHT}}"

  duo_prompt "What would you like to use for monitor scale (1, 1.25, 1.33, 1.5, 1.67 or 2) (default: ${DEFAULT_SCALE})? " input
  DEFAULT_SCALE="${input:-${DEFAULT_SCALE}}"

  local remap_answer=""
  local remap_prompt="Enable USB Media Remap by default? [Y/n] "
  if [ "${USB_MEDIA_REMAP_ENABLED}" != true ]; then
    remap_prompt="Enable USB Media Remap by default? [y/N] "
  fi
  duo_prompt "${remap_prompt}" remap_answer
  case "${remap_answer}" in
    [yY]|[yY][eE][sS])
      USB_MEDIA_REMAP_ENABLED=true
      ;;
    [nN]|[nN][oO])
      USB_MEDIA_REMAP_ENABLED=false
      ;;
  esac
}

install_duo_dependencies() {
  if command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y "${DNF_PACKAGES[@]}"
  elif command -v apt >/dev/null 2>&1; then
    sudo apt install -y "${APT_PACKAGES[@]}"
  elif command -v pacman >/dev/null 2>&1; then
    sudo pacman -S --needed --noconfirm "${PACMAN_PACKAGES[@]}"
  else
    echo "Unsupported package manager. Please install dependencies manually:"
    echo "  ${MANUAL_DEPENDENCIES_HINT}"
    exit 1
  fi
}

# Older installs appended a wildcard rule to /etc/sudoers. Remove it on
# upgrade; the edited copy is checked with visudo before it replaces the file.
# Temp files for sudoers are created and written as root (sudo mktemp + sudo
# tee): a user-owned one could be swapped between the visudo check and the
# install, letting any program of the user install its own sudoers rules.
remove_legacy_duo_sudoers() {
  local legacy='/usr/bin/tee /sys/class/backlight/\*/brightness'
  sudo grep -q "${legacy}" /etc/sudoers 2>/dev/null || return 0
  local tmp
  tmp="$(sudo mktemp)" || return 0
  sudo sed "\|${legacy}|d" /etc/sudoers | sudo tee "${tmp}" >/dev/null
  if sudo visudo -cqf "${tmp}"; then
    sudo install -m 0440 -o root -g root "${tmp}" /etc/sudoers
    echo "Removed the old wildcard brightness rule from /etc/sudoers"
  else
    echo "WARNING: could not remove the old brightness rule from /etc/sudoers; remove it with visudo" >&2
  fi
  sudo rm -f "${tmp}"
}

configure_duo_sudoers() {
  # Exact brightness paths only (a '*' in sudoers args also matches
  # spaces, so the earlier wildcard rule allowed writing any file as root).
  # Written to sudoers.d and syntax-checked with visudo instead of appending to /etc/sudoers.
  remove_legacy_duo_sudoers
  if [ -n "${TARGET_USER}" ]; then
    local tmp dir rules=""
    for dir in /sys/class/backlight/*; do
      [ -e "${dir}/brightness" ] || continue
      rules+="${TARGET_USER} ALL=(root) NOPASSWD: /usr/bin/tee ${dir}/brightness"$'\n'
    done
    tmp="$(sudo mktemp)" || return 0
    printf '%s' "${rules}" | sudo tee "${tmp}" >/dev/null
    if sudo visudo -cqf "${tmp}"; then
      sudo install -m 0440 -o root -g root "${tmp}" /etc/sudoers.d/zenbook-duo
    else
      echo "WARNING: generated sudoers rule failed validation; skipped" >&2
    fi
    sudo rm -f "${tmp}"
  fi
}

configure_duo_input_access() {
  # Not adding the user to the 'input' group (it grants read access
  # to every keyboard). The uaccess udev rule below covers the Duo keyboard.
  if [ -n "${TARGET_USER}" ] && id -nG "${TARGET_USER}" | grep -qw input; then
    echo "Note: ${TARGET_USER} is in the 'input' group (added by older versions). It is no"
    echo "longer needed; remove it with: sudo gpasswd -d ${TARGET_USER} input"
  fi

  echo 'SUBSYSTEM=="input", ATTRS{name}=="*ASUS Zenbook Duo Keyboard", TAG+="uaccess", MODE="0660", GROUP="input"' | sudo tee /etc/udev/rules.d/90-zenbook-duo-keyboard.rules >/dev/null
  # Stale: niri ignores WL_OUTPUT for touch, so this rule never took effect.
  sudo rm -f /etc/udev/rules.d/91-zenbook-duo-touch-output.rules

  # Do not install a hwdb remap for KEYBOARD_KEY_7003*: it overrides the USB Fn layer.
  sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb
  sudo systemd-hwdb update
  sudo udevadm trigger
}

DUO_LIBINPUT_QUIRKS_FILE=/etc/libinput/local-overrides.quirks
DUO_LIBINPUT_QUIRKS_BEGIN="# BEGIN zenbook-duo"
DUO_LIBINPUT_QUIRKS_END="# END zenbook-duo"

remove_duo_libinput_quirks() {
  [ -f "${DUO_LIBINPUT_QUIRKS_FILE}" ] || return 0
  sudo sed -i "\|^${DUO_LIBINPUT_QUIRKS_BEGIN}\$|,\|^${DUO_LIBINPUT_QUIRKS_END}\$|d" "${DUO_LIBINPUT_QUIRKS_FILE}"
}

configure_duo_libinput_quirks() {
  # The keyboard touchpad is on USB, so libinput treats it as external and
  # offers no disable-while-typing. Declaring it a keyboard combo enables DWT,
  # paired with keyboards sharing its vid/pid (including the USB remap device).
  # libinput only reads this one file from /etc, so manage a marked block in it.
  sudo mkdir -p "$(dirname "${DUO_LIBINPUT_QUIRKS_FILE}")"
  remove_duo_libinput_quirks
  sudo tee -a "${DUO_LIBINPUT_QUIRKS_FILE}" >/dev/null <<EOF
${DUO_LIBINPUT_QUIRKS_BEGIN}
[ASUS Zenbook Duo Keyboard Touchpad]
MatchUdevType=touchpad
MatchName=*ASUS Zenbook Duo Keyboard Touchpad*
AttrTPKComboLayout=below
${DUO_LIBINPUT_QUIRKS_END}
EOF
}

write_duo_settings_defaults() {
  local python3=""
  python3="$(command -v python3 || true)"
  [ -n "${python3}" ] || return 0
  [ -n "${TARGET_HOME}" ] || return 0

  local config_dir="${TARGET_HOME}/.config/zenbook-duo"
  local settings_file="${config_dir}/settings.json"
  local start_on_boot_minimized="${START_ON_BOOT_MINIMIZED:-${DUO_DEFAULT_START_ON_BOOT_MINIMIZED}}"

  local python_cmd=("${python3}")
  if [ "${TARGET_USER}" != "${USER:-}" ] || [ "${EUID}" = "0" ]; then
    sudo -u "${TARGET_USER}" mkdir -p "${config_dir}" 2>/dev/null || true
    python_cmd=(sudo -u "${TARGET_USER}" "${python3}")
  else
    mkdir -p "${config_dir}" 2>/dev/null || true
  fi

  "${python_cmd[@]}" - "${settings_file}" "${DEFAULT_BACKLIGHT}" "${DEFAULT_SCALE}" "${USB_MEDIA_REMAP_ENABLED}" "${start_on_boot_minimized}" "${DUO_INSTALLER_MARKS_SETUP_COMPLETED}" <<'PY'
import json
import sys
from pathlib import Path

settings_file = Path(sys.argv[1])
default_backlight = int(sys.argv[2])
default_scale = float(sys.argv[3])
usb_media_remap_enabled = sys.argv[4].strip().lower() in ("1", "true", "yes", "y", "on")
start_on_boot_minimized = sys.argv[5].strip().lower() in ("1", "true", "yes", "y", "on")
setup_completed = sys.argv[6].strip().lower() in ("1", "true", "yes", "y", "on")

settings_file.parent.mkdir(parents=True, exist_ok=True)

data = {}
if settings_file.exists():
    try:
        loaded = json.loads(settings_file.read_text())
        if isinstance(loaded, dict):
            data = loaded
    except Exception:
        data = {}

data.setdefault("autoDualScreen", True)
data.setdefault("syncBrightness", True)
data.setdefault("theme", "system")
data.setdefault("invertSensorRotation", False)

data["defaultBacklight"] = default_backlight
data["defaultScale"] = default_scale
data["usbMediaRemapEnabled"] = usb_media_remap_enabled
data["startOnBootMinimized"] = start_on_boot_minimized
data["setupCompleted"] = setup_completed

settings_file.write_text(json.dumps(data, indent=2) + "\n")
PY
}

install_duo_rust_runtime() {
  echo "Installing Rust runtime..."
  "${DUO_SETUP_DIR}/install-rust-runtime.sh"
}
