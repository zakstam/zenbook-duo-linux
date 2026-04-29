#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UI_DIR="${ROOT_DIR}/ui-tauri-react"

usage() {
  cat <<'EOF'
check.sh - compatibility-focused project checks

Usage:
  ./check.sh installers   # shell syntax + installer smoke tests
  ./check.sh rust         # Rust runtime unit tests
  ./check.sh frontend     # React/TypeScript production build
  ./check.sh all          # run every check above
EOF
}

check_installers() {
  echo "==> Checking installer shell syntax"
  bash -n \
    "${ROOT_DIR}/install.sh" \
    "${ROOT_DIR}/install-ui.sh" \
    "${ROOT_DIR}/install-rust-runtime.sh" \
    "${ROOT_DIR}/setup-common.sh" \
    "${ROOT_DIR}/setup-gnome.sh" \
    "${ROOT_DIR}/setup-kde.sh" \
    "${ROOT_DIR}/setup-niri.sh" \
    "${ROOT_DIR}/uninstall.sh" \
    "${ROOT_DIR}/tests/install-stdin-test.sh"

  echo "==> Running installer smoke tests"
  bash "${ROOT_DIR}/tests/install-stdin-test.sh"
}

check_rust() {
  echo "==> Running Rust runtime unit tests"
  cargo test --manifest-path "${UI_DIR}/src-tauri/Cargo.toml" --lib
}

check_frontend() {
  echo "==> Building and type-checking frontend"
  (cd "${UI_DIR}" && npm run vite:build)
}

case "${1:-all}" in
  installers)
    check_installers
    ;;
  rust)
    check_rust
    ;;
  frontend)
    check_frontend
    ;;
  all)
    check_installers
    check_rust
    check_frontend
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    echo "ERROR: unknown check target: ${1:-}" >&2
    echo >&2
    usage >&2
    exit 1
    ;;
esac
