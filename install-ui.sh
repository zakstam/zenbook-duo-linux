#!/usr/bin/env bash
set -euo pipefail

REPO_URL_DEFAULT="https://github.com/zakstam/zenbook-duo-linux.git"
BRANCH_DEFAULT=""

SCRIPT_PATH="${BASH_SOURCE[0]:-${0}}"
RUNNING_FROM_STDIN=false
SCRIPT_PATH_ABS=""
if [ -z "${SCRIPT_PATH}" ] || [ "${SCRIPT_PATH}" = "bash" ] || [ "${SCRIPT_PATH}" = "-" ]; then
  RUNNING_FROM_STDIN=true
  SCRIPT_DIR="$(pwd)"
else
  SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
  SCRIPT_PATH_ABS="${SCRIPT_DIR}/$(basename "${SCRIPT_PATH}")"
fi
SCRIPT_VERSION="2026-04-26"

usage() {
  cat <<'EOF_USAGE'
install-ui.sh - clone, build, and install the Zenbook Duo Control UI (Tauri)

Usage:
  ./install-ui.sh [--repo URL] [--branch BRANCH] [--dir PATH] [--keep-dir]

Examples:
  ./install-ui.sh
  ./install-ui.sh --dir "$HOME/src/zenbook-duo-linux"
  ./install-ui.sh --repo https://github.com/zakstam/zenbook-duo-linux.git --branch main

Notes:
  - The Control Panel can be launched from your app menu after install.
  - Privileged steps use a graphical auth prompt when available, with a terminal fallback.
EOF_USAGE
}

die() {
  echo "ERROR: $*" >&2
  exit 1
}

running_as_root() {
  [ "${EUID:-$(id -u)}" -eq 0 ]
}

stop_running_app() {
  if ! command -v pgrep >/dev/null 2>&1; then
    return 1
  fi

  local pids=""
  pids="$(pgrep -x zenbook-duo-control 2>/dev/null || true)"
  if [ -z "$pids" ]; then
    return 1
  fi

  echo "Stopping running zenbook-duo-control (pids: $pids)..."
  kill $pids 2>/dev/null || true

  local i=0
  while [ "$i" -lt 25 ]; do
    if ! pgrep -x zenbook-duo-control >/dev/null 2>&1; then
      echo "Stopped."
      return 0
    fi
    sleep 0.2
    i=$((i + 1))
  done

  echo "Force killing zenbook-duo-control..."
  pids="$(pgrep -x zenbook-duo-control 2>/dev/null || true)"
  if [ -n "$pids" ]; then
    kill -9 $pids 2>/dev/null || true
  fi
  return 0
}

start_app_background() {
  if ! command -v zenbook-duo-control >/dev/null 2>&1; then
    echo "WARN: zenbook-duo-control not found on PATH; can't relaunch automatically." >&2
    return 1
  fi

  echo "Relaunching zenbook-duo-control..."
  nohup zenbook-duo-control >/dev/null 2>&1 &
}

ensure_shortcut() {
  return 0
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}

root_install_prereqs_dnf() {
  echo "Installing build prerequisites (dnf)..."
  dnf install -y \
    git \
    curl \
    nodejs \
    npm \
    gcc \
    gcc-c++ \
    make \
    pkgconf-pkg-config \
    openssl-devel \
    webkit2gtk4.1-devel \
    gtk3-devel \
    librsvg2-devel \
    rpm-build

  echo "Installing AppIndicator dev package (best-effort)..."
  dnf install -y libappindicator-gtk3-devel \
    || dnf install -y libappindicator3-devel \
    || dnf install -y ayatana-appindicator3-devel \
    || echo "WARN: Could not install an AppIndicator *-devel package. Tray integration may not build on some desktops." >&2
}

root_install_prereqs_apt() {
  echo "Installing build prerequisites (apt)..."
  apt update
  apt install -y \
    git \
    curl \
    nodejs \
    npm \
    build-essential \
    pkg-config \
    libssl-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    librsvg2-dev \
    dpkg-dev \
    fakeroot

  echo "Installing AppIndicator dev package (best-effort)..."
  apt install -y libayatana-appindicator3-dev \
    || apt install -y libappindicator3-dev \
    || echo "WARN: Could not install an AppIndicator *-dev package. Tray integration may not build on some desktops." >&2
}

root_install_prereqs_pacman() {
  echo "Installing build prerequisites (pacman)..."
  pacman -S --needed --noconfirm \
    git \
    curl \
    nodejs \
    npm \
    base-devel \
    pkgconf \
    openssl \
    gtk3 \
    webkit2gtk-4.1 \
    librsvg \
    desktop-file-utils

  echo "Installing AppIndicator package (best-effort)..."
  pacman -S --needed --noconfirm libayatana-appindicator \
    || echo "WARN: Could not install libayatana-appindicator. Tray integration may not build on some desktops." >&2
}

root_install_package_dnf() {
  local pkg="${1:-}"
  local built_vr="${2:-}"
  local installed_vr="${3:-}"
  [ -n "${pkg}" ] || die "--root-install-package dnf requires a package path"

  if [ -n "$built_vr" ] && [ -n "$installed_vr" ] && [ "$built_vr" = "$installed_vr" ]; then
    dnf reinstall -y "$pkg"
  else
    dnf install -y "$pkg"
  fi
}

root_install_package_apt() {
  local pkg="${1:-}"
  [ -n "${pkg}" ] || die "--root-install-package apt requires a package path"

  apt install -y --reinstall "$pkg" || dpkg -i "$pkg"
}

root_install_ui_direct() {
  local built_binary="${1:-}"
  local desktop_src="${2:-}"
  local icon_src="${3:-}"

  [ -f "${built_binary}" ] || die "No built UI binary found at ${built_binary}"
  [ -f "${desktop_src}" ] || die "Desktop entry template missing at ${desktop_src}"
  [ -f "${icon_src}" ] || die "Icon missing at ${icon_src}"

  echo "Installing UI binary and desktop assets..."
  install -Dm755 "${built_binary}" /usr/local/bin/zenbook-duo-control
  install -Dm644 "${desktop_src}" /usr/share/applications/zenbook-duo-control.desktop
  install -Dm644 "${icon_src}" /usr/share/pixmaps/zenbook-duo-control.png

  update-desktop-database /usr/share/applications 2>/dev/null || true
  gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
}

dispatch_root_helper() {
  local action="${1:-}"
  shift || true

  running_as_root || die "Privileged helper requires root"

  case "$action" in
    --root-install-prereqs)
      case "${1:-}" in
        dnf)
          root_install_prereqs_dnf
          ;;
        apt)
          root_install_prereqs_apt
          ;;
        pacman)
          root_install_prereqs_pacman
          ;;
        *)
          die "Unsupported package manager for --root-install-prereqs: ${1:-<empty>}"
          ;;
      esac
      ;;
    --root-install-package)
      local pkg_mgr="${1:-}"
      local pkg="${2:-}"
      local built_vr="${3:-}"
      local installed_vr="${4:-}"
      case "$pkg_mgr" in
        dnf)
          root_install_package_dnf "$pkg" "$built_vr" "$installed_vr"
          ;;
        apt)
          root_install_package_apt "$pkg"
          ;;
        *)
          die "Unsupported package manager for --root-install-package: ${pkg_mgr:-<empty>}"
          ;;
      esac
      ;;
    --root-install-direct)
      root_install_ui_direct "${1:-}" "${2:-}" "${3:-}"
      ;;
    *)
      die "Unknown privileged helper action: ${action:-<empty>}"
      ;;
  esac
}

if [[ "${1:-}" == --root-install-* ]]; then
  dispatch_root_helper "$@"
  exit 0
fi

REPO_URL="$REPO_URL_DEFAULT"
BRANCH="$BRANCH_DEFAULT"
TARGET_DIR=""
KEEP_DIR=false
FORCE_CLONE=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --repo)
      [ "${2:-}" != "" ] || die "--repo requires a value"
      REPO_URL="$2"
      shift 2
      ;;
    --branch)
      [ "${2:-}" != "" ] || die "--branch requires a value"
      BRANCH="$2"
      shift 2
      ;;
    --dir)
      [ "${2:-}" != "" ] || die "--dir requires a value"
      TARGET_DIR="$2"
      shift 2
      ;;
    --keep-dir)
      KEEP_DIR=true
      shift
      ;;
    --clone)
      FORCE_CLONE=true
      shift
      ;;
    *)
      die "Unknown argument: $1 (use --help)"
      ;;
  esac
done

echo "install-ui.sh v${SCRIPT_VERSION}"

export PAGER=cat
export SYSTEMD_PAGER=cat
export GIT_PAGER=cat
export LESS="${LESS:--FRX}"

PKG_MGR=""
if command -v dnf >/dev/null 2>&1; then
  PKG_MGR="dnf"
elif command -v apt >/dev/null 2>&1; then
  PKG_MGR="apt"
elif command -v pacman >/dev/null 2>&1; then
  PKG_MGR="pacman"
fi

SUDO_KEEPALIVE_PID=""
PRIVILEGE_MODE=""

has_graphical_session() {
  [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${XDG_SESSION_TYPE:-}" ]
}

prepare_privilege_helper() {
  if running_as_root; then
    PRIVILEGE_MODE="root"
    return 0
  fi

  if [ -z "${SCRIPT_PATH_ABS}" ]; then
    echo "WARN: install-ui.sh is running from stdin; graphical elevation is unavailable in this mode." >&2
  fi

  if [ -n "${SCRIPT_PATH_ABS}" ] && command -v pkexec >/dev/null 2>&1 && has_graphical_session; then
    PRIVILEGE_MODE="pkexec"
    echo "Using graphical privilege prompt via pkexec for system install steps."
    return 0
  fi

  if command -v sudo >/dev/null 2>&1; then
    PRIVILEGE_MODE="sudo"
    echo "Using terminal sudo fallback for system install steps."
    sudo -v
    (
      while true; do
        sleep 50
        sudo -n true || exit 0
      done
    ) >/dev/null 2>&1 &
    SUDO_KEEPALIVE_PID="$!"
    return 0
  fi

  die "Need either pkexec (graphical) or sudo (terminal) to perform system install steps"
}

run_root_helper() {
  local action="$1"
  shift || true

  case "$PRIVILEGE_MODE" in
    root)
      dispatch_root_helper "$action" "$@"
      ;;
    pkexec)
      [ -n "$SCRIPT_PATH_ABS" ] || die "Graphical elevation requires running install-ui.sh from a file"
      pkexec "$SCRIPT_PATH_ABS" "$action" "$@"
      ;;
    sudo)
      [ -n "$SCRIPT_PATH_ABS" ] || die "Terminal elevation requires running install-ui.sh from a file"
      sudo -E "$SCRIPT_PATH_ABS" "$action" "$@"
      ;;
    *)
      die "Privilege helper is not initialized"
      ;;
  esac
}

install_prereqs() {
  if [ "$PKG_MGR" = "dnf" ] || [ "$PKG_MGR" = "apt" ] || [ "$PKG_MGR" = "pacman" ]; then
    run_root_helper --root-install-prereqs "$PKG_MGR"
  else
    echo "WARN: Could not detect dnf/apt/pacman; skipping prereq installation." >&2
  fi
}

ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then
    return 0
  fi
  echo "Rust toolchain not found. Installing via rustup..."
  need_cmd curl
  curl https://sh.rustup.rs -sSf | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
}

install_ui_direct() {
  local built_binary="$PWD/src-tauri/target/release/zenbook-duo-control"
  local desktop_src="$PWD/src-tauri/linux/zenbook-duo-control.desktop"
  local icon_src="$PWD/src-tauri/icons/128x128.png"

  echo "Building UI binary directly for pacman-based systems..."
  npx tauri build --no-bundle

  [ -f "${built_binary}" ] || die "No built UI binary found at ${built_binary}"
  [ -f "${desktop_src}" ] || die "Desktop entry template missing at ${desktop_src}"
  [ -f "${icon_src}" ] || die "Icon missing at ${icon_src}"

  local was_running=false
  if pgrep -x zenbook-duo-control >/dev/null 2>&1; then
    was_running=true
    stop_running_app || true
  fi

  run_root_helper --root-install-direct "${built_binary}" "${desktop_src}" "${icon_src}"

  if [ "${was_running}" = true ]; then
    start_app_background || true
  fi

  rm -f "$HOME/.local/share/applications/zenbook-duo-control.desktop" 2>/dev/null || true
  update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
}

prepare_privilege_helper
install_prereqs

echo "Prerequisites step complete."

ensure_rust

need_cmd git
need_cmd npm
need_cmd cargo

echo "Toolchain looks good. Starting clone/build/install..."

if [ -z "$TARGET_DIR" ]; then
  if [ "$RUNNING_FROM_STDIN" = false ] && [ "$FORCE_CLONE" = false ] && [ -d "$SCRIPT_DIR/ui-tauri-react" ] && [ -e "$SCRIPT_DIR/.git" ]; then
    TARGET_DIR="$SCRIPT_DIR"
    KEEP_DIR=true
    echo "Using local repo: $TARGET_DIR"
  else
    TARGET_DIR="$(mktemp -d -t zenbook-duo-linux-ui.XXXXXX)"
    echo "Cloning into temp dir: $TARGET_DIR"
  fi
else
  mkdir -p "$TARGET_DIR"
  if [ -e "$TARGET_DIR/.git" ]; then
    echo "Using existing repo: $TARGET_DIR"
  elif [ "$(ls -A "$TARGET_DIR" 2>/dev/null | wc -l | tr -d ' ')" != "0" ]; then
    die "--dir points to a non-empty directory without a git repo: $TARGET_DIR"
  fi
fi

cleanup() {
  if [ -n "${SUDO_KEEPALIVE_PID}" ]; then
    kill "${SUDO_KEEPALIVE_PID}" >/dev/null 2>&1 || true
  fi
  if [ "$KEEP_DIR" = false ] && [[ "$TARGET_DIR" == /tmp/* || "$TARGET_DIR" == /var/tmp/* || "$TARGET_DIR" == /private/tmp/* ]]; then
    rm -rf "$TARGET_DIR"
  fi
}
trap cleanup EXIT

if [ "$TARGET_DIR" = "$SCRIPT_DIR" ] && [ "$FORCE_CLONE" = false ]; then
  :
elif [ ! -e "$TARGET_DIR/.git" ]; then
  if [ -n "$BRANCH" ]; then
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$TARGET_DIR"
  else
    git clone --depth 1 "$REPO_URL" "$TARGET_DIR"
  fi
else
  (cd "$TARGET_DIR" && git pull --ff-only) || true
fi

UI_DIR="$TARGET_DIR/ui-tauri-react"
[ -d "$UI_DIR" ] || die "ui directory not found at $UI_DIR"

echo "Building UI in: $UI_DIR"
cd "$UI_DIR"
if [ -f package-lock.json ]; then
  npm ci
else
  npm install
fi

if [ "$PKG_MGR" = "dnf" ]; then
  npm run build -- --bundles rpm
elif [ "$PKG_MGR" = "apt" ]; then
  npm run build -- --bundles deb
elif [ "$PKG_MGR" = "pacman" ]; then
  install_ui_direct
else
  npm run build -- --bundles deb rpm
fi

if [ "$PKG_MGR" = "pacman" ]; then
  echo ""
  echo "Installed. You can launch 'Zenbook Duo Control' from your app menu,"
  echo "or run: zenbook-duo-control"
  exit 0
fi

RPM_GLOB="src-tauri/target/release/bundle/rpm/"'*.rpm'
DEB_GLOB="src-tauri/target/release/bundle/deb/"'*.deb'

if [ "$PKG_MGR" = "dnf" ]; then
  PKG="$(ls -1t $RPM_GLOB 2>/dev/null | head -n 1 || true)"
  [ -n "$PKG" ] || die "No rpm found after build (looked for $RPM_GLOB)"
  echo "Installing rpm: $PKG"
  WAS_RUNNING=false
  if pgrep -x zenbook-duo-control >/dev/null 2>&1; then
    WAS_RUNNING=true
    stop_running_app || true
  fi
  BUILT_VR="$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "$PKG" 2>/dev/null || true)"
  INSTALLED_VR="$(rpm -q --qf '%{VERSION}-%{RELEASE}' zenbook-duo-control 2>/dev/null || true)"
  run_root_helper --root-install-package dnf "$PWD/$PKG" "$BUILT_VR" "$INSTALLED_VR"
  if [ "$WAS_RUNNING" = true ]; then
    start_app_background || true
  fi
  rm -f "$HOME/.local/share/applications/zenbook-duo-control.desktop" 2>/dev/null || true
  update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
elif [ "$PKG_MGR" = "apt" ]; then
  PKG="$(ls -1t $DEB_GLOB 2>/dev/null | head -n 1 || true)"
  [ -n "$PKG" ] || die "No deb found after build (looked for $DEB_GLOB)"
  echo "Installing deb: $PKG"
  WAS_RUNNING=false
  if pgrep -x zenbook-duo-control >/dev/null 2>&1; then
    WAS_RUNNING=true
    stop_running_app || true
  fi
  run_root_helper --root-install-package apt "$PWD/$PKG"
  if [ "$WAS_RUNNING" = true ]; then
    start_app_background || true
  fi
  rm -f "$HOME/.local/share/applications/zenbook-duo-control.desktop" 2>/dev/null || true
  update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
else
  die "Unsupported system (expected dnf, apt, or pacman). Package is built under src-tauri/target/release/bundle/."
fi

echo ""
echo "Installed. You can launch 'Zenbook Duo Control' from your app menu,"
echo "or run: zenbook-duo-control"
