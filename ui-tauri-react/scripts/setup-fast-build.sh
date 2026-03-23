#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLE_CONFIG="$ROOT_DIR/.cargo/config.toml.example"
LOCAL_CONFIG="$ROOT_DIR/.cargo/config.toml"

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

install_with_manager() {
  if have_cmd pacman; then
    sudo pacman -S --needed clang mold sccache
    return
  fi

  if have_cmd apt-get; then
    sudo apt-get update
    sudo apt-get install -y clang mold sccache
    return
  fi

  if have_cmd dnf; then
    sudo dnf install -y clang mold sccache
    return
  fi

  if have_cmd zypper; then
    sudo zypper install -y clang mold sccache
    return
  fi

  echo "Unsupported package manager. Install these manually: clang mold sccache" >&2
  exit 1
}

echo "Checking build tools..."

missing=0
for tool in clang mold sccache; do
  if have_cmd "$tool"; then
    echo "  found: $tool"
  else
    echo "  missing: $tool"
    missing=1
  fi
done

if [[ "$missing" -eq 1 ]]; then
  echo
  echo "Installing missing tools..."
  install_with_manager
else
  echo
  echo "All recommended tools are already installed."
fi

mkdir -p "$(dirname "$LOCAL_CONFIG")"

if [[ -f "$LOCAL_CONFIG" ]]; then
  echo
  echo "Keeping existing Cargo config at $LOCAL_CONFIG"
else
  cp "$EXAMPLE_CONFIG" "$LOCAL_CONFIG"
  echo
  echo "Wrote $LOCAL_CONFIG from template."
fi

cat <<'EOF'

Recommended usage:
  CARGO_BUILD_JOBS=8 RUSTC_WRAPPER=sccache npm run build:local

Adjust the jobs value for your machine.
EOF
