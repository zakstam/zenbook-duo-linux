#!/usr/bin/env bash
# Shared setup flow for Zenbook Duo desktop backends.
# Desktop-specific wrappers declare only their backend dependency deltas and then
# call run_duo_setup.

DUO_COMMON_DNF_PACKAGES=(usbutils iio-sensor-proxy systemd)
DUO_COMMON_APT_PACKAGES=(usbutils iio-sensor-proxy systemd)
DUO_COMMON_PACMAN_PACKAGES=(usbutils iio-sensor-proxy systemd)
DUO_COMMON_MANUAL_DEPENDENCIES_HINT="usbutils, iio-sensor-proxy, systemd"

# Keep these defaults aligned with DuoSettings::default in the Rust runtime and
# DEFAULT_DUO_SETTINGS in the frontend.
DUO_DEFAULT_BACKLIGHT=0
DUO_DEFAULT_SCALE=1.66
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

  DEFAULT_BACKLIGHT="${DEFAULT_BACKLIGHT:-${DUO_DEFAULT_BACKLIGHT}}"
  DEFAULT_SCALE="${DEFAULT_SCALE:-${DUO_DEFAULT_SCALE}}"
  USB_MEDIA_REMAP_ENABLED="${USB_MEDIA_REMAP_ENABLED:-${DUO_DEFAULT_USB_MEDIA_REMAP_ENABLED}}"
  START_ON_BOOT_MINIMIZED="${START_ON_BOOT_MINIMIZED:-${DUO_DEFAULT_START_ON_BOOT_MINIMIZED}}"

  build_duo_setup_package_lists

  parse_duo_setup_args "$@"
  resolve_duo_target_user
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
        shift
        ;;
      --no-usb-media-remap)
        USB_MEDIA_REMAP_ENABLED=false
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

  duo_prompt "What would you like to use for monitor scale (1 = 100%, 1.5 = 150%, 1.66 = 166%, 2=200%) (default: ${DEFAULT_SCALE})? " input
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

add_duo_sudoers() {
  local entry="$1"
  if ! sudo grep -Fq "${entry}" /etc/sudoers 2>/dev/null; then
    echo "${entry}" | sudo tee -a /etc/sudoers >/dev/null
  fi
}

configure_duo_sudoers() {
  if [ -n "${TARGET_USER}" ]; then
    add_duo_sudoers "${TARGET_USER} ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/*/brightness"
  fi
}

configure_duo_input_access() {
  if [ -n "${TARGET_USER}" ] && ! groups "${TARGET_USER}" | grep -q '\binput\b'; then
    sudo usermod -aG input "${TARGET_USER}"
    echo "Added ${TARGET_USER} to input group (logout/login required for full effect)"
  fi

  echo 'SUBSYSTEM=="input", ATTRS{name}=="*ASUS Zenbook Duo Keyboard", TAG+="uaccess", MODE="0660", GROUP="input"' | sudo tee /etc/udev/rules.d/90-zenbook-duo-keyboard.rules >/dev/null

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
