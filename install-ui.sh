#!/usr/bin/env bash
set -euo pipefail

REPO_URL_DEFAULT="https://github.com/zakstam/zenbook-duo-linux.git"
BRANCH_DEFAULT=""

SCRIPT_PATH="${BASH_SOURCE[0]:-${0}}"
RUNNING_FROM_STDIN=false
if [ -z "${SCRIPT_PATH}" ] || [ "${SCRIPT_PATH}" = "bash" ] || [ "${SCRIPT_PATH}" = "-" ]; then
  RUNNING_FROM_STDIN=true
  SCRIPT_DIR="$(pwd)"
else
  SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
fi
SCRIPT_VERSION="2026-01-30"

usage() {
  cat <<'EOF'
install-ui.sh - clone, build, and install the Zenbook Duo Control UI (Tauri)

Usage:
  ./install-ui.sh [--repo URL] [--branch BRANCH] [--dir PATH] [--keep-dir]

Examples:
  ./install-ui.sh
  ./install-ui.sh --dir "$HOME/src/zenbook-duo-linux"
  ./install-ui.sh --repo https://github.com/zakstam/zenbook-duo-linux.git --branch main

Notes:
  - The Control Panel can be launched from your app menu after install.
EOF
}

die() {
  echo "ERROR: $*" >&2
  exit 1
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
  # Try graceful first.
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
  # Detach from this script completely.
  nohup zenbook-duo-control >/dev/null 2>&1 &
}

ensure_shortcut() {
  # The RPM/DEB already installs a system .desktop file (and icons). Creating another entry in
  # ~/.local/share/applications results in duplicate app launchers in GNOME.
  #
  # Keep this as a no-op for now. If we want a desktop icon in the future, we should create it
  # conditionally and only when there isn't already a system-installed desktop entry.
  return 0
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}

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
fi

SUDO_KEEPALIVE_PID=""
if command -v sudo >/dev/null 2>&1; then
  # Ask for sudo once up-front so the script doesn't pause mid-run.
  echo "Requesting sudo..."
  sudo -v
  # Keep sudo alive while we run (best-effort).
  ( while true; do sleep 50; sudo -n true || exit 0; done ) >/dev/null 2>&1 &
  SUDO_KEEPALIVE_PID="$!"
fi

install_prereqs_dnf() {
  echo "Installing build prerequisites (dnf)..."
  sudo dnf install -y \
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

  # AppIndicator dev package naming varies across Fedora variants.
  # Avoid `dnf list ... >/dev/null` checks (can be slow and look like a hang with no output).
  echo "Installing AppIndicator dev package (best-effort)..."
  sudo dnf install -y libappindicator-gtk3-devel \
    || sudo dnf install -y libappindicator3-devel \
    || sudo dnf install -y ayatana-appindicator3-devel \
    || echo "WARN: Could not install an AppIndicator *-devel package. Tray integration may not build on some desktops." >&2
}

install_prereqs_apt() {
  echo "Installing build prerequisites (apt)..."
  sudo apt update
  sudo apt install -y \
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

  # AppIndicator dev package naming varies across Debian/Ubuntu.
  echo "Installing AppIndicator dev package (best-effort)..."
  sudo apt install -y libayatana-appindicator3-dev \
    || sudo apt install -y libappindicator3-dev \
    || echo "WARN: Could not install an AppIndicator *-dev package. Tray integration may not build on some desktops." >&2
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

if [ "$PKG_MGR" = "dnf" ]; then
  install_prereqs_dnf
elif [ "$PKG_MGR" = "apt" ]; then
  install_prereqs_apt
else
  echo "WARN: Could not detect dnf/apt; skipping prereq installation." >&2
fi

echo "Prerequisites step complete."

ensure_rust

# Re-check tools after installing prereqs.
need_cmd git
need_cmd npm
need_cmd cargo

echo "Toolchain looks good. Starting clone/build/install..."

if [ -z "$TARGET_DIR" ]; then
  # If the script is running from inside the repo, build in-place by default
  # (this makes development iterations faster and ensures local fixes are used).
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
  : # using local repo, do not clone/pull
elif [ ! -e "$TARGET_DIR/.git" ]; then
  if [ -n "$BRANCH" ]; then
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$TARGET_DIR"
  else
    git clone --depth 1 "$REPO_URL" "$TARGET_DIR"
  fi
else
  # Non-destructive update attempt (fails if local changes / detached head).
  (cd "$TARGET_DIR" && git pull --ff-only) || true
fi

UI_DIR="$TARGET_DIR/ui-tauri-react"
[ -d "$UI_DIR" ] || die "ui directory not found at $UI_DIR"

echo "Building UI in: $UI_DIR"
cd "$UI_DIR"
npm install
# Avoid building AppImage by selecting bundles explicitly.
if [ "$PKG_MGR" = "dnf" ]; then
  npm run build -- --bundles rpm
elif [ "$PKG_MGR" = "apt" ]; then
  npm run build -- --bundles deb
else
  npm run build -- --bundles deb rpm
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
  # dnf won't replace an installed package if the NEVRA is identical. Use reinstall in that case
  # so local builds apply even when you forgot to bump the version.
  if rpm -q zenbook-duo-control >/dev/null 2>&1; then
    sudo dnf reinstall -y "$PKG"
  else
    sudo dnf install -y "$PKG"
  fi
  if [ "$WAS_RUNNING" = true ]; then
    start_app_background || true
  fi
  # Cleanup: older versions of this script created a duplicate launcher in ~/.local.
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
  # Prefer apt reinstall (keeps deps tidy), fallback to dpkg.
  sudo apt install -y --reinstall "$PWD/$PKG" || sudo dpkg -i "$PWD/$PKG"
  if [ "$WAS_RUNNING" = true ]; then
    start_app_background || true
  fi
  rm -f "$HOME/.local/share/applications/zenbook-duo-control.desktop" 2>/dev/null || true
  update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
else
  die "Unsupported system (expected dnf or apt). Package is built under src-tauri/target/release/bundle/."
fi

echo ""
echo "Installed. You can launch 'Zenbook Duo Control' from your app menu,"
echo "or run: zenbook-duo-control"
