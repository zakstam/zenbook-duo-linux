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

run_user_systemctl() {
  if [ "${TARGET_USER}" = "${USER:-}" ] && [ "${EUID}" != "0" ]; then
    systemctl --user "$@"
    return
  fi

  sudo -u "${TARGET_USER}" \
    XDG_RUNTIME_DIR="/run/user/${TARGET_UID}" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${TARGET_UID}/bus" \
    systemctl --user "$@"
}

import_user_environment() {
  run_user_systemctl import-environment DISPLAY WAYLAND_DISPLAY NIRI_SOCKET XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE || true

  if command -v dbus-update-activation-environment >/dev/null 2>&1; then
    if [ "${TARGET_USER}" = "${USER:-}" ] && [ "${EUID}" != "0" ]; then
      dbus-update-activation-environment --systemd DISPLAY WAYLAND_DISPLAY NIRI_SOCKET XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE >/dev/null 2>&1 || true
    else
      sudo -u "${TARGET_USER}" \
        XDG_RUNTIME_DIR="/run/user/${TARGET_UID}" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${TARGET_UID}/bus" \
        dbus-update-activation-environment --systemd DISPLAY WAYLAND_DISPLAY NIRI_SOCKET XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE >/dev/null 2>&1 || true
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
After=graphical-session.target

[Service]
Type=simple
ExecStart=${RUNTIME_INSTALL_DIR}/zenbook-duo-session-agent
Restart=on-failure
RestartSec=1
TimeoutStopSec=5
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus

[Install]
WantedBy=default.target
EOF

sudo rm -f "${SYSTEM_SLEEP_HOOK_PATH}"
sudo ln -s "${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle" "${SYSTEM_SLEEP_HOOK_PATH}"

sudo systemctl disable zenbook-duo.service 2>/dev/null || true
sudo systemctl stop zenbook-duo.service 2>/dev/null || true
sudo rm -f /usr/lib/systemd/system-sleep/duo

sudo systemctl daemon-reload
sudo systemctl enable "${SYSTEM_SERVICE_NAME}"
sudo systemctl enable "${LIFECYCLE_SERVICE_NAME}"
sudo systemctl restart "${SYSTEM_SERVICE_NAME}"
sudo systemctl restart "${LIFECYCLE_SERVICE_NAME}"

run_user_systemctl daemon-reload
import_user_environment
run_user_systemctl disable zenbook-duo-user.service 2>/dev/null || true
run_user_systemctl stop zenbook-duo-user.service 2>/dev/null || true
run_user_systemctl enable "${USER_SERVICE_NAME}"
run_user_systemctl restart "${USER_SERVICE_NAME}"

echo "Rust runtime services installed."
