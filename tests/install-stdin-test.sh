#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

output="$(cat "${ROOT_DIR}/install.sh" | bash -s -- --help 2>&1)" || {
  echo "FAIL: piped install.sh --help should succeed" >&2
  exit 1
}

if [[ "${output}" != *"install.sh - unified installer for Zenbook Duo Linux"* ]]; then
  echo "FAIL: help output missing expected banner" >&2
  exit 1
fi

empty_array_output="$(bash --noprofile --norc -c 'set -euo pipefail; BASH_SOURCE=(); source <(head -n 9 "'"${ROOT_DIR}"'/install.sh"); printf "%s\n" "${SCRIPT_DIR}"' 2>&1)" || {
  echo "FAIL: init should survive an empty BASH_SOURCE array" >&2
  exit 1
}

if [[ "${empty_array_output}" != "$(pwd)" ]]; then
  echo "FAIL: empty-array fallback should use current working directory" >&2
  exit 1
fi

temp_root="$(mktemp -d)"
trap 'rm -rf "${temp_root}"' EXIT
fake_bin="${temp_root}/bin"
mkdir -p "${fake_bin}"

cat > "${fake_bin}/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "clone" ]]; then
  echo "unexpected git args: $*" >&2
  exit 1
fi
target="${@: -1}"
mkdir -p "${target}"
touch "${target}/setup-common.sh" "${target}/setup-gnome.sh" "${target}/setup-kde.sh" "${target}/setup-niri.sh" "${target}/install-ui.sh"
EOF
chmod +x "${fake_bin}/git"

checkout_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  export PATH="'"${fake_bin}"':/usr/bin:/bin"
  source <(sed -n "1,139p" "'"${ROOT_DIR}"'/install.sh")
  SCRIPT_DIR="'"${temp_root}"'/missing-checkout"
  ensure_repo_checkout
' 2>/dev/null)" || {
  echo "FAIL: ensure_repo_checkout should succeed with fake git" >&2
  exit 1
}

if [[ "${checkout_output}" == *"Downloading zenbook-duo-linux..."* ]]; then
  echo "FAIL: ensure_repo_checkout should not mix status logs into stdout" >&2
  exit 1
fi

if [[ ! -f "${checkout_output}/setup-niri.sh" ]]; then
  echo "FAIL: ensure_repo_checkout should print the checkout path" >&2
  exit 1
fi

desktop_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  source <(sed -n "1,139p" "'"${ROOT_DIR}"'/install.sh")
  XDG_CURRENT_DESKTOP=GNOME DESKTOP_SESSION= XDG_SESSION_DESKTOP= pick_desktop
  XDG_CURRENT_DESKTOP="KDE Plasma" DESKTOP_SESSION= XDG_SESSION_DESKTOP= pick_desktop
  XDG_CURRENT_DESKTOP= DESKTOP_SESSION= XDG_SESSION_DESKTOP=niri pick_desktop
  XDG_CURRENT_DESKTOP=Hyprland DESKTOP_SESSION= XDG_SESSION_DESKTOP= pick_desktop
  XDG_CURRENT_DESKTOP=Hyprland DESKTOP_SESSION=niri XDG_SESSION_DESKTOP=niri pick_desktop
' 2>/dev/null)" || {
  echo "FAIL: pick_desktop should detect all supported desktops" >&2
  exit 1
}

if [[ "${desktop_output}" != $'gnome\nkde\nniri\nhyprland\nhyprland' ]]; then
  echo "FAIL: pick_desktop should map GNOME, KDE, Niri, and Hyprland consistently, trusting XDG_CURRENT_DESKTOP over leftovers from an earlier login" >&2
  exit 1
fi

assert_setup_packages() {
  local label="$1"
  local dnf_desktop="$2"
  local apt_desktop="$3"
  local pacman_desktop="$4"
  local manual_hint="$5"
  local output=""

  output="$(bash --noprofile --norc -c '
    set -euo pipefail
    source "$1"
    DNF_DESKTOP_PACKAGES=("$2")
    APT_DESKTOP_PACKAGES=("$3")
    PACMAN_DESKTOP_PACKAGES=("$4")
    MANUAL_DESKTOP_DEPENDENCIES_HINT="$5"
    build_duo_setup_package_lists
    printf "dnf:%s\napt:%s\npacman:%s\nhint:%s\n" \
      "${DNF_PACKAGES[*]}" \
      "${APT_PACKAGES[*]}" \
      "${PACMAN_PACKAGES[*]}" \
      "${MANUAL_DEPENDENCIES_HINT}"
  ' _ "${ROOT_DIR}/setup-common.sh" "${dnf_desktop}" "${apt_desktop}" "${pacman_desktop}" "${manual_hint}")" || {
    echo "FAIL: setup package matrix should build for ${label}" >&2
    exit 1
  }

  local expected=$'dnf:usbutils iio-sensor-proxy systemd upower pulseaudio-utils '"${dnf_desktop}"$'\napt:usbutils iio-sensor-proxy systemd upower pulseaudio-utils '"${apt_desktop}"$'\npacman:usbutils iio-sensor-proxy systemd upower libpulse '"${pacman_desktop}"$'\nhint:usbutils, iio-sensor-proxy, systemd, upower, pactl (pulseaudio-utils), '"${manual_hint}"
  if [[ "${output}" != "${expected}" ]]; then
    echo "FAIL: setup package matrix mismatch for ${label}" >&2
    echo "Got:" >&2
    echo "${output}" >&2
    exit 1
  fi
}

assert_setup_packages gnome 'mutter dconf' 'mutter-common-bin dconf-cli' 'mutter dconf' 'mutter/gdctl, dconf'
assert_setup_packages kde kscreen kscreen kscreen kscreen/kscreen-doctor
assert_setup_packages niri niri niri niri niri

if ! grep -q 'WantedBy=default.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should be enabled from default.target" >&2
  exit 1
fi

if grep -q 'Wants=graphical-session.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should not pull in graphical-session.target" >&2
  exit 1
fi

# PartOf/WantedBy graphical-session.target stops the agent at a niri logout and
# never restarts it under a desktop that skips that target (Hyprland without
# uwsm) while the user manager lingers. The agent follows the live desktop itself.
if grep -Eq 'PartOf=graphical-session.target|WantedBy=.*graphical-session.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service must not be bound to graphical-session.target (breaks switching desktops with linger)" >&2
  exit 1
fi

if ! grep -q 'After=graphical-session.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should still order itself after graphical-session.target" >&2
  exit 1
fi

if ! grep -q 'StartLimitIntervalSec=0' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should keep retrying while the graphical session environment is not ready" >&2
  exit 1
fi

if grep -q 'ExecStartPre=' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should start without a session-variable gate; the agent waits for a live desktop itself" >&2
  exit 1
fi

if ! grep -q 'setup-hyprland.sh" --snippet-only' "${ROOT_DIR}/install.sh" || ! grep -q -- '--snippet-only) SNIPPET_ONLY=true' "${ROOT_DIR}/setup-hyprland.sh"; then
  echo "FAIL: install.sh should add the Hyprland session snippet whenever Hyprland is installed" >&2
  exit 1
fi

if ! grep -q 'RestartSec=2' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should retry startup at a bounded cadence" >&2
  exit 1
fi

if ! grep -q 'import-environment DISPLAY WAYLAND_DISPLAY NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: installer should import graphical session environment for the user manager" >&2
  exit 1
fi

if ! grep -q 'ensure_system_manager_available' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should preflight the system service manager before building" >&2
  exit 1
fi

if ! grep -q 'run_system_systemctl daemon-reload' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should wrap required system daemon reloads with clear errors" >&2
  exit 1
fi

# A failing service must not be reported as "systemd manager is unavailable".
systemctl_fns="$(sed -n '/^systemctl_output_means_manager_unreachable() {/,/^}/p; /^emit_systemctl_failure() {/,/^}/p' "${ROOT_DIR}/install-rust-runtime.sh")"
job_failure_message="$(bash -c "${systemctl_fns}"'
emit_systemctl_failure "The system" "sudo systemctl --system restart x.service" "Job for x.service failed because the control process exited with error code."' 2>&1)"
if printf '%s' "${job_failure_message}" | grep -q 'manager is unavailable' \
  || ! printf '%s' "${job_failure_message}" | grep -q 'command failed: sudo systemctl --system restart x.service'; then
  echo "FAIL: a failing service restart should say the command failed, not that systemd is unavailable" >&2
  printf '%s\n' "${job_failure_message}" >&2
  exit 1
fi
bus_failure_message="$(bash -c "${systemctl_fns}"'
emit_systemctl_failure "The system" "sudo systemctl --system daemon-reload" "Failed to connect to bus: No such file or directory"' 2>&1)"
if ! printf '%s' "${bus_failure_message}" | grep -q 'manager is unavailable'; then
  echo "FAIL: an unreachable systemd bus should still be reported as the manager being unavailable" >&2
  exit 1
fi

# A lifecycle restart failure must not stop the session agent from being restarted.
lifecycle_line="$(grep -n 'restart_system_service_reporting "${LIFECYCLE_SERVICE_NAME}" || LIFECYCLE_RESTART_FAILED=1' "${ROOT_DIR}/install-rust-runtime.sh" | cut -d: -f1)"
agent_line="$(grep -n 'run_user_systemctl_checked restart "${USER_SERVICE_NAME}"' "${ROOT_DIR}/install-rust-runtime.sh" | cut -d: -f1)"
if [ -z "${lifecycle_line}" ] || [ -z "${agent_line}" ] || [ "${lifecycle_line}" -ge "${agent_line}" ]; then
  echo "FAIL: runtime installer should restart the session agent even when the lifecycle service fails" >&2
  exit 1
fi

if ! grep -q 'ensure_user_manager_available' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should preflight the target user service manager" >&2
  exit 1
fi

if ! grep -q 'run_user_systemctl_checked daemon-reload' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should hard-fail required user daemon reloads with clear errors" >&2
  exit 1
fi

if ! grep -Fq 'local bus_address="unix:path=${runtime_dir}/bus"' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should avoid stale user bus environment when calling systemctl --user" >&2
  exit 1
fi

if ! grep -Fq 'DBUS_SESSION_BUS_ADDRESS="${bus_address}"' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should pass the stable user bus to systemctl --user" >&2
  exit 1
fi

for service_name in \
  'SYSTEM_SERVICE_NAME="zenbook-duo-rust-daemon.service"' \
  'LIFECYCLE_SERVICE_NAME="zenbook-duo-rust-lifecycle.service"' \
  'USER_SERVICE_NAME="zenbook-duo-session-agent.service"'; do
  if ! grep -q "${service_name}" "${ROOT_DIR}/install-rust-runtime.sh"; then
    echo "FAIL: runtime installer should keep service name ${service_name}" >&2
    exit 1
  fi
done

if ! grep -q 'Installed Rust runtime versions:' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should print installed runtime versions" >&2
  exit 1
fi

if ! grep -Fq '"${RUNTIME_INSTALL_DIR}/${binary}" --version' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: runtime installer should call installed binaries with --version" >&2
  exit 1
fi

for binary_hook in \
  'src/bin/zenbook-duo-daemon.rs:print_and_exit_if_requested("zenbook-duo-daemon")' \
  'src/bin/zenbook-duo-session-agent.rs:print_and_exit_if_requested("zenbook-duo-session-agent")' \
  'src/bin/zenbook-duo-lifecycle.rs:print_and_exit_if_requested("zenbook-duo-lifecycle")' \
  'src/bin/zenbook-duo-usb-remap-helper.rs:print_and_exit_if_requested("zenbook-duo-usb-remap-helper")' \
  'src/bin/usb-media-remap.rs:print_and_exit_if_requested("usb-media-remap")'; do
  binary_path="${binary_hook%%:*}"
  hook="${binary_hook#*:}"
  compact_binary="$(tr -d '[:space:]' < "${ROOT_DIR}/ui-tauri-react/src-tauri/${binary_path}" | sed 's/,);/);/g')"
  if [[ "${compact_binary}" != *"${hook}"* ]]; then
    echo "FAIL: runtime binary ${binary_path} should support --version" >&2
    exit 1
  fi
done

if ! grep -q 'SYSTEM_SLEEP_HOOK_PATH="/usr/lib/systemd/system-sleep/zenbook-duo-rust-lifecycle"' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: installer should install the Rust lifecycle sleep hook" >&2
  exit 1
fi

if ! grep -q 'ln -sfn "${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle" "${SYSTEM_SLEEP_HOOK_PATH}"' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: lifecycle binary should be the single system-sleep entrypoint" >&2
  exit 1
fi

if ! grep -q 'run_user_systemctl_best_effort daemon-reload' "${ROOT_DIR}/uninstall.sh"; then
  echo "FAIL: uninstaller should not fail cleanup when the user manager daemon reload is unavailable" >&2
  exit 1
fi

if ! grep -q 'run_user_systemctl_best_effort stop zenbook-duo-session-agent.service' "${ROOT_DIR}/uninstall.sh"; then
  echo "FAIL: uninstaller should treat user service cleanup as best-effort" >&2
  exit 1
fi

if grep -q 'ExecStart=.*\(pre\|post\|thaw\|hibernate\)' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: lifecycle service should not duplicate suspend/resume handling" >&2
  exit 1
fi

if ! grep -q 'command -v pacman' "${ROOT_DIR}/setup-common.sh"; then
  echo "FAIL: shared setup helper should support pacman-based systems" >&2
  exit 1
fi

for common_packages in \
  'DUO_COMMON_DNF_PACKAGES=(usbutils iio-sensor-proxy systemd upower pulseaudio-utils)' \
  'DUO_COMMON_APT_PACKAGES=(usbutils iio-sensor-proxy systemd upower pulseaudio-utils)' \
  'DUO_COMMON_PACMAN_PACKAGES=(usbutils iio-sensor-proxy systemd upower libpulse)'; do
  if ! grep -q "${common_packages}" "${ROOT_DIR}/setup-common.sh"; then
    echo "FAIL: shared setup helper should centralize common dependency list ${common_packages}" >&2
    exit 1
  fi
done

remap_default_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  source "'"${ROOT_DIR}"'/setup-common.sh"
  duo_prompt() { printf -v "$2" ""; }

  DEFAULT_BACKLIGHT=0
  DEFAULT_SCALE=1.67
  USB_MEDIA_REMAP_ENABLED=false
  prompt_duo_defaults
  printf "%s\n" "${USB_MEDIA_REMAP_ENABLED}"

  DEFAULT_BACKLIGHT=0
  DEFAULT_SCALE=1.67
  USB_MEDIA_REMAP_ENABLED=true
  prompt_duo_defaults
  printf "%s\n" "${USB_MEDIA_REMAP_ENABLED}"
' 2>&1)" || {
  echo "FAIL: prompt_duo_defaults should be callable with mocked prompts" >&2
  exit 1
}

if [[ "${remap_default_output}" != $'false\ntrue' ]]; then
  echo "FAIL: empty USB remap prompt answer should preserve the current default" >&2
  printf 'Got:\n%s\n' "${remap_default_output}" >&2
  exit 1
fi

# Brightness goes through logind: the installer grants no passwordless sudo
# and a reinstall removes the rules earlier versions added. Temp files for
# /etc/sudoers edits must be created and written as root: a user-owned one
# could be swapped between the visudo check and the install.
if grep -q 'NOPASSWD' "${ROOT_DIR}/setup-common.sh"; then
  echo "FAIL: the installer should not grant passwordless sudo" >&2
  exit 1
fi

sudoers_tmp_output="$(FAKE_SUDOERS="${temp_root}/fake-sudoers" FAKE_SUDOERS_D="${temp_root}/fake-sudoers.d-zenbook-duo" bash --noprofile --norc -c '
  set -euo pipefail
  source "'"${ROOT_DIR}"'/setup-common.sh"
  printf "root ALL=(ALL) ALL\nzaks ALL=NOPASSWD:/usr/bin/tee /sys/class/backlight/*/brightness\n" > "${FAKE_SUDOERS}"
  echo "testuser ALL=(root) NOPASSWD: /usr/bin/tee /sys/class/backlight/x/brightness" > "${FAKE_SUDOERS_D}"
  mktemp() { echo "user-mktemp"; command mktemp "$@"; }
  sudo() {
    local args=() a
    for a in "$@"; do
      [ "${a}" = /etc/sudoers ] && a="${FAKE_SUDOERS}"
      [ "${a}" = /etc/sudoers.d/zenbook-duo ] && a="${FAKE_SUDOERS_D}"
      args+=("${a}")
    done
    case "$1" in
      mktemp) echo "root-mktemp" >&2; command mktemp ;;
      visudo) return 0 ;;
      install) echo "installed $(grep -c backlight "${args[-2]}" || true) -> ${args[-1]}" >&2 ;;
      *) command "${args[@]}" ;;
    esac
  }
  TARGET_USER=testuser
  remove_duo_sudoers
  [ -e "${FAKE_SUDOERS_D}" ] && echo "sudoers.d-left" >&2
  true
' 2>&1)" || {
  echo "FAIL: remove_duo_sudoers should run" >&2
  printf '%s\n' "${sudoers_tmp_output}" >&2
  exit 1
}

if grep -q 'user-mktemp' <<< "${sudoers_tmp_output}" \
  || [[ "$(grep -c 'root-mktemp' <<< "${sudoers_tmp_output}")" != 1 ]] \
  || [[ "$(grep -c 'installed' <<< "${sudoers_tmp_output}")" != 1 ]] \
  || ! grep -q 'installed 0 -> .*fake-sudoers$' <<< "${sudoers_tmp_output}" \
  || grep -q 'sudoers.d-left' <<< "${sudoers_tmp_output}"; then
  echo "FAIL: a reinstall should remove the old brightness sudoers rules, editing /etc/sudoers through root-owned temp files" >&2
  printf 'Got:\n%s\n' "${sudoers_tmp_output}" >&2
  exit 1
fi

cat > "${temp_root}/settings-defaults-test.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
source "$1"
temp_home="$2/home"
mkdir -p "${temp_home}"
TARGET_HOME="${temp_home}"
TARGET_USER="${USER:-$(id -un)}"
DEFAULT_BACKLIGHT="${DUO_DEFAULT_BACKLIGHT}"
DEFAULT_SCALE="${DUO_DEFAULT_SCALE}"
USB_MEDIA_REMAP_ENABLED="${DUO_DEFAULT_USB_MEDIA_REMAP_ENABLED}"
write_duo_settings_defaults
python3 - "${temp_home}/.config/zenbook-duo/settings.json" <<'PY'
import json
import sys
from pathlib import Path
settings = json.loads(Path(sys.argv[1]).read_text())
expected = {
    "defaultBacklight": 0,
    "defaultScale": 1.67,
    "usbMediaRemapEnabled": True,
    "startOnBootMinimized": False,
    "setupCompleted": True,
    "autoDualScreen": True,
    "syncBrightness": True,
    "theme": "system",
}
for key, value in expected.items():
    if settings.get(key) != value:
        raise SystemExit(f"{key} expected {value!r}, got {settings.get(key)!r}")
PY
EOF

bash "${temp_root}/settings-defaults-test.sh" "${ROOT_DIR}/setup-common.sh" "${temp_root}" || {
  echo "FAIL: installer-written settings defaults should match the compatibility contract" >&2
  exit 1
}

if ! grep -q 'pub const DEFAULT_BACKLIGHT_LEVEL: u8 = 0;' "${ROOT_DIR}/ui-tauri-react/src-tauri/src/models/settings.rs"; then
  echo "FAIL: Rust settings default backlight should match installer defaults" >&2
  exit 1
fi

if ! grep -q 'defaultBacklight: 0,' "${ROOT_DIR}/ui-tauri-react/src/lib/defaults.ts"; then
  echo "FAIL: frontend settings default backlight should match installer defaults" >&2
  exit 1
fi

if ! grep -q 'startOnBootMinimized: false,' "${ROOT_DIR}/ui-tauri-react/src/lib/defaults.ts"; then
  echo "FAIL: frontend settings default startup preference should match installer defaults" >&2
  exit 1
fi

for setup_script in setup-gnome.sh setup-kde.sh setup-niri.sh setup-hyprland.sh; do
  expected_name="${setup_script}"
  if ! grep -q "SETUP_SCRIPT_NAME=\"${expected_name}\"" "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare its setup script name for shared errors" >&2
    exit 1
  fi
  if ! grep -q 'source "${DUO_SETUP_DIR}/setup-common.sh"' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should delegate to setup-common.sh" >&2
    exit 1
  fi
  if ! grep -q 'DNF_DESKTOP_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare only desktop-specific dnf dependencies" >&2
    exit 1
  fi
  if ! grep -q 'APT_DESKTOP_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare only desktop-specific apt dependencies" >&2
    exit 1
  fi
  if ! grep -q 'PACMAN_DESKTOP_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare only desktop-specific pacman dependencies" >&2
    exit 1
  fi
  if grep -q '^DNF_PACKAGES=' "${ROOT_DIR}/${setup_script}" || grep -q '^APT_PACKAGES=' "${ROOT_DIR}/${setup_script}" || grep -q '^PACMAN_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should not duplicate common package arrays" >&2
    exit 1
  fi
done

saved_defaults_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  source "'"${ROOT_DIR}"'/setup-common.sh"
  home="$(mktemp -d)"
  mkdir -p "${home}/.config/zenbook-duo"
  echo "{\"defaultBacklight\": 2, \"defaultScale\": 1.6666666666666667, \"usbMediaRemapEnabled\": false}" \
    > "${home}/.config/zenbook-duo/settings.json"
  TARGET_HOME="${home}"
  DEFAULT_BACKLIGHT=0 DEFAULT_SCALE=1.67 USB_MEDIA_REMAP_ENABLED=true
  load_duo_saved_defaults
  printf "%s %s %s\n" "${DEFAULT_BACKLIGHT}" "${DEFAULT_SCALE}" "${USB_MEDIA_REMAP_ENABLED}"
  DEFAULT_BACKLIGHT=0 DEFAULT_SCALE=1.25 USB_MEDIA_REMAP_ENABLED=true DUO_GIVEN_SCALE=1
  load_duo_saved_defaults
  printf "%s %s %s\n" "${DEFAULT_BACKLIGHT}" "${DEFAULT_SCALE}" "${USB_MEDIA_REMAP_ENABLED}"
  rm -rf "${home}"
' 2>&1)" || {
  echo "FAIL: load_duo_saved_defaults should be callable" >&2
  exit 1
}

if [[ "${saved_defaults_output}" != $'2 1.67 false\n2 1.25 false' ]]; then
  echo "FAIL: a reinstall should default to the saved settings, unless given explicitly" >&2
  printf 'Got:\n%s\n' "${saved_defaults_output}" >&2
  exit 1
fi

if ! grep -q 'PKG_MGR="pacman"' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should detect pacman-based systems" >&2
  exit 1
fi

if ! grep -q 'install_ui_direct' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should have a direct-install path for pacman systems" >&2
  exit 1
fi

if ! grep -q 'command -v pkexec' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should prefer pkexec when a graphical auth prompt is available" >&2
  exit 1
fi

if ! grep -q -- '--root-install-prereqs' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should expose a root-only prereq helper entrypoint" >&2
  exit 1
fi

if ! grep -q -- '--root-install-package' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should expose a root-only package install helper entrypoint" >&2
  exit 1
fi

if ! grep -q -- '--root-install-direct' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should expose a root-only direct install helper entrypoint" >&2
  exit 1
fi

if ! grep -q 'sudo pacman -Rns --noconfirm zenbook-duo-control' "${ROOT_DIR}/uninstall.sh"; then
  echo "FAIL: uninstall.sh should try pacman removal when available" >&2
  exit 1
fi

echo "PASS"
