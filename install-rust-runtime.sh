#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]:-${0}}"
SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
TAURI_DIR="${SCRIPT_DIR}/ui-tauri-react/src-tauri"
RUNTIME_INSTALL_DIR="/usr/local/libexec/zenbook-duo"
SYSTEM_SERVICE_NAME="zenbook-duo-rust-daemon.service"
LIFECYCLE_SERVICE_NAME="zenbook-duo-rust-lifecycle.service"
USER_SERVICE_NAME="zenbook-duo-session-agent.service"
SYSTEM_SLEEP_HOOK_PATH="/usr/lib/systemd/system-sleep/zenbook-duo-rust-lifecycle"
SYSTEM_SLEEP_HOOK_DIR="$(dirname "${SYSTEM_SLEEP_HOOK_PATH}")"

TARGET_USER="${USER:-}"
if [ "${EUID}" = "0" ]; then
  if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
    TARGET_USER="${SUDO_USER}"
  else
    echo "ERROR: install-rust-runtime.sh must be launched from a real user session." >&2
    exit 1
  fi
fi

TARGET_UID="$(id -u "${TARGET_USER}" 2>/dev/null || true)"
if [ -z "${TARGET_UID}" ]; then
  echo "ERROR: failed to resolve target user '${TARGET_USER}'" >&2
  exit 1
fi
TARGET_GID="$(id -g "${TARGET_USER}" 2>/dev/null || true)"
if [ -z "${TARGET_GID}" ]; then
  echo "ERROR: failed to resolve primary group for '${TARGET_USER}'" >&2
  exit 1
fi
TARGET_HOME="$(getent passwd "${TARGET_USER}" 2>/dev/null | cut -d: -f6)"
if [ -z "${TARGET_HOME}" ]; then
  echo "ERROR: failed to resolve home directory for '${TARGET_USER}'" >&2
  exit 1
fi

die() {
  echo "ERROR: $*" >&2
  exit 1
}

systemctl_output_means_manager_unreachable() {
  case "$1" in
    *"Failed to connect to bus"*|*"System has not been booted with systemd"*|*"Failed to connect to system scope bus"*|*"Failed to connect to user scope bus"*)
      return 0
      ;;
  esac
  return 1
}

emit_systemctl_failure() {
  local manager="$1"
  local command_display="$2"
  local output="${3:-}"

  if systemctl_output_means_manager_unreachable "${output}"; then
    echo "ERROR: ${manager} systemd manager is unavailable; cannot run: ${command_display}" >&2
  else
    echo "ERROR: command failed: ${command_display}" >&2
  fi
  if [ -n "${output}" ]; then
    printf '%s\n' "${output}" | sed 's/^/  systemctl: /' >&2
  fi
}

run_system_systemctl() {
  local output=""
  if ! output="$(sudo systemctl --system "$@" 2>&1)"; then
    emit_systemctl_failure "The system" "sudo systemctl --system $*" "${output}"
    if systemctl_output_means_manager_unreachable "${output}"; then
      echo "Make sure this machine is booted with systemd and that 'sudo systemctl status' works, then rerun ./install.sh." >&2
    fi
    exit 1
  fi
  if [ -n "${output}" ]; then
    printf '%s\n' "${output}"
  fi
}

# Restart a system service. On failure, print what the service itself logged and
# return non-zero instead of exiting, so the installer still restarts the session agent.
restart_system_service_reporting() {
  local unit="$1"
  local output=""
  if output="$(sudo systemctl --system restart "${unit}" 2>&1)"; then
    return 0
  fi
  emit_systemctl_failure "The system" "sudo systemctl --system restart ${unit}" "${output}"
  if systemctl_output_means_manager_unreachable "${output}"; then
    echo "Make sure this machine is booted with systemd and that 'sudo systemctl status' works, then rerun ./install.sh." >&2
    exit 1
  fi
  echo "${unit} failed to start. Its last log lines:" >&2
  sudo journalctl --system -u "${unit}" -n 15 --no-pager -o cat 2>&1 | sed 's/^/  journal: /' >&2 || true
  return 1
}

run_user_systemctl() {
  local runtime_dir="/run/user/${TARGET_UID}"
  local bus_address="unix:path=${runtime_dir}/bus"

  if [ "${TARGET_USER}" = "${USER:-}" ] && [ "${EUID}" != "0" ]; then
    XDG_RUNTIME_DIR="${runtime_dir}" \
      DBUS_SESSION_BUS_ADDRESS="${bus_address}" \
      systemctl --user "$@"
    return
  fi

  sudo -u "${TARGET_USER}" \
    XDG_RUNTIME_DIR="${runtime_dir}" \
    DBUS_SESSION_BUS_ADDRESS="${bus_address}" \
    systemctl --user "$@"
}

run_user_systemctl_checked() {
  local output=""
  if ! output="$(run_user_systemctl "$@" 2>&1)"; then
    emit_systemctl_failure "The ${TARGET_USER} user" "systemctl --user $*" "${output}"
    echo "Run the installer from an active desktop login where 'systemctl --user status' works, then rerun ./install.sh." >&2
    exit 1
  fi
  if [ -n "${output}" ]; then
    printf '%s\n' "${output}"
  fi
}

ensure_system_manager_available() {
  local output=""
  if ! output="$(sudo systemctl --system show --property=Version --value 2>&1)"; then
    emit_systemctl_failure "The system" "sudo systemctl --system show --property=Version --value" "${output}"
    echo "Make sure this machine is booted with systemd and that 'sudo systemctl status' works, then rerun ./install.sh." >&2
    exit 1
  fi
}

ensure_user_manager_available() {
  local runtime_dir="/run/user/${TARGET_UID}"
  local bus_path="${runtime_dir}/bus"
  local output=""

  if [ ! -d "${runtime_dir}" ]; then
    die "The systemd user runtime directory is missing for ${TARGET_USER}: ${runtime_dir}. Log in as ${TARGET_USER} and rerun ./install.sh from that desktop session."
  fi

  if [ ! -S "${bus_path}" ]; then
    die "The systemd user bus is missing for ${TARGET_USER}: ${bus_path}. Log in as ${TARGET_USER} and rerun ./install.sh from that desktop session."
  fi

  if ! output="$(run_user_systemctl show-environment 2>&1)"; then
    emit_systemctl_failure "The ${TARGET_USER} user" "systemctl --user show-environment" "${output}"
    echo "Run the installer from an active desktop login where 'systemctl --user status' works, then rerun ./install.sh." >&2
    exit 1
  fi
}

import_user_environment() {
  run_user_systemctl import-environment DISPLAY WAYLAND_DISPLAY NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE >/dev/null 2>&1 || true

  if command -v dbus-update-activation-environment >/dev/null 2>&1; then
    if [ "${TARGET_USER}" = "${USER:-}" ] && [ "${EUID}" != "0" ]; then
      XDG_RUNTIME_DIR="/run/user/${TARGET_UID}" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${TARGET_UID}/bus" \
        dbus-update-activation-environment --systemd DISPLAY WAYLAND_DISPLAY NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE >/dev/null 2>&1 || true
    else
      sudo -u "${TARGET_USER}" \
        XDG_RUNTIME_DIR="/run/user/${TARGET_UID}" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${TARGET_UID}/bus" \
        dbus-update-activation-environment --systemd DISPLAY WAYLAND_DISPLAY NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE >/dev/null 2>&1 || true
    fi
  fi
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: Missing required command: $1" >&2
    exit 1
  }
}

need_cmd cargo
need_cmd systemctl
ensure_system_manager_available
ensure_user_manager_available

if [ ! -f "${TAURI_DIR}/Cargo.toml" ]; then
  echo "ERROR: Could not find Tauri crate at ${TAURI_DIR}" >&2
  exit 1
fi

echo "Building Rust runtime binaries..."
cargo build --release \
  --manifest-path "${TAURI_DIR}/Cargo.toml" \
  --bin zenbook-duo-daemon \
  --bin zenbook-duo-session-agent \
  --bin zenbook-duo-lifecycle \
  --bin zenbook-duo-usb-remap-helper

echo "Installing Rust runtime binaries..."
sudo mkdir -p "${RUNTIME_INSTALL_DIR}"
sudo install -m 0755 \
  "${TAURI_DIR}/target/release/zenbook-duo-daemon" \
  "${RUNTIME_INSTALL_DIR}/zenbook-duo-daemon"
sudo install -m 0755 \
  "${TAURI_DIR}/target/release/zenbook-duo-session-agent" \
  "${RUNTIME_INSTALL_DIR}/zenbook-duo-session-agent"
sudo install -m 0755 \
  "${TAURI_DIR}/target/release/zenbook-duo-lifecycle" \
  "${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle"
sudo install -m 0755 \
  "${TAURI_DIR}/target/release/zenbook-duo-usb-remap-helper" \
  "${RUNTIME_INSTALL_DIR}/zenbook-duo-usb-remap-helper"

echo "Installed Rust runtime versions:"
for binary in \
  zenbook-duo-daemon \
  zenbook-duo-session-agent \
  zenbook-duo-lifecycle \
  zenbook-duo-usb-remap-helper; do
  "${RUNTIME_INSTALL_DIR}/${binary}" --version || true
done

echo "Installing Rust runtime services..."
cat <<EOF | sudo tee "/etc/systemd/system/${SYSTEM_SERVICE_NAME}" >/dev/null
[Unit]
Description=Zenbook Duo Rust Daemon
After=network.target

[Service]
Type=simple
ExecStart=${RUNTIME_INSTALL_DIR}/zenbook-duo-daemon
Environment=ZENBOOK_DUO_HOME=${TARGET_HOME}
Environment=ZENBOOK_DUO_USER=${TARGET_USER}
Environment=ZENBOOK_DUO_UID=${TARGET_UID}
Environment=ZENBOOK_DUO_GID=${TARGET_GID}
Restart=on-failure
RestartSec=1

[Install]
WantedBy=multi-user.target
EOF

cat <<EOF | sudo tee "/etc/systemd/system/${LIFECYCLE_SERVICE_NAME}" >/dev/null
[Unit]
Description=Zenbook Duo Rust Lifecycle Handler
DefaultDependencies=no
Before=shutdown.target
After=multi-user.target

[Service]
Type=oneshot
ExecStart=${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle boot
ExecStop=${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle shutdown
RemainAfterExit=yes
TimeoutStartSec=10
TimeoutStopSec=10

[Install]
WantedBy=multi-user.target
EOF

cat <<EOF | sudo tee "/etc/systemd/user/${USER_SERVICE_NAME}" >/dev/null
[Unit]
Description=Zenbook Duo Session Agent
ConditionUser=!gdm
# Restarted with every graphical login on desktops that use
# graphical-session.target, so each new session gets the screen layout for the
# keyboard's state (the user manager outlives a quick logout). Also started
# from default.target for desktops that never start that target.
PartOf=graphical-session.target
After=graphical-session.target
StartLimitIntervalSec=0

[Service]
Type=simple
ExecStartPre=/bin/sh -c 'test -n "\${DISPLAY:-}\${WAYLAND_DISPLAY:-}\${NIRI_SOCKET:-}"'
ExecStart=${RUNTIME_INSTALL_DIR}/zenbook-duo-session-agent
Restart=on-failure
RestartSec=2
TimeoutStopSec=5
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus

[Install]
WantedBy=default.target graphical-session.target
EOF

sudo mkdir -p "${SYSTEM_SLEEP_HOOK_DIR}"
sudo rm -f "${SYSTEM_SLEEP_HOOK_PATH}"
sudo ln -sfn "${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle" "${SYSTEM_SLEEP_HOOK_PATH}"

sudo systemctl disable zenbook-duo.service 2>/dev/null || true
sudo systemctl stop zenbook-duo.service 2>/dev/null || true
sudo rm -f /usr/lib/systemd/system-sleep/duo

run_system_systemctl daemon-reload
run_system_systemctl enable "${SYSTEM_SERVICE_NAME}"
run_system_systemctl enable "${LIFECYCLE_SERVICE_NAME}"
run_system_systemctl restart "${SYSTEM_SERVICE_NAME}"
LIFECYCLE_RESTART_FAILED=0
restart_system_service_reporting "${LIFECYCLE_SERVICE_NAME}" || LIFECYCLE_RESTART_FAILED=1

ensure_user_manager_available
run_user_systemctl_checked daemon-reload
import_user_environment
run_user_systemctl disable zenbook-duo-user.service >/dev/null 2>&1 || true
run_user_systemctl stop zenbook-duo-user.service >/dev/null 2>&1 || true
# Re-enable so the links match the current [Install] section.
run_user_systemctl disable "${USER_SERVICE_NAME}" >/dev/null 2>&1 || true
run_user_systemctl_checked enable "${USER_SERVICE_NAME}"
run_user_systemctl_checked restart "${USER_SERVICE_NAME}"

if [ "${LIFECYCLE_RESTART_FAILED}" = "1" ]; then
  echo "ERROR: the daemon and session agent now run the new version, but ${LIFECYCLE_SERVICE_NAME} failed (see its log lines above)." >&2
  exit 1
fi

echo "Rust runtime services installed."
