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

empty_array_output="$(bash -lc 'set -euo pipefail; BASH_SOURCE=(); source <(head -n 9 "'"${ROOT_DIR}"'/install.sh"); printf "%s\n" "${SCRIPT_DIR}"' 2>&1)" || {
  echo "FAIL: init should survive an empty BASH_SOURCE array" >&2
  exit 1
}

if [[ "${empty_array_output}" != "$(pwd)" ]]; then
  echo "FAIL: empty-array fallback should use current working directory" >&2
  exit 1
fi

echo "PASS"
