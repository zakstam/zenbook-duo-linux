#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Source the function definitions without triggering the daemon entrypoint.
source <(sed '/^# Dispatch: no arguments = run as daemon, with arguments = run as CLI command\./,$d' "${ROOT_DIR}/duo.sh")

assert_eq() {
    local expected="${1}"
    local actual="${2}"
    local message="${3}"
    if [ "${expected}" != "${actual}" ]; then
        echo "FAIL: ${message} (expected ${expected}, got ${actual})" >&2
        exit 1
    fi
}

start_calls=0
stop_calls=0
remap_enabled=true

duo-usb-media-remap-enabled() {
    echo "${remap_enabled}"
}

duo-usb-media-remap-start() {
    start_calls=$((start_calls + 1))
}

duo-usb-media-remap-stop() {
    stop_calls=$((stop_calls + 1))
}

duo-ensure-usb-media-remap-state true
assert_eq 1 "${start_calls}" "starts remap when keyboard is attached and feature is enabled"
assert_eq 0 "${stop_calls}" "does not stop remap while attached and enabled"

remap_enabled=false
duo-ensure-usb-media-remap-state true
assert_eq 1 "${start_calls}" "does not start remap again when feature is disabled"
assert_eq 1 "${stop_calls}" "stops remap when keyboard is attached but feature is disabled"

remap_enabled=true
duo-ensure-usb-media-remap-state false
assert_eq 1 "${start_calls}" "does not start remap when keyboard is detached"
assert_eq 2 "${stop_calls}" "stops remap when keyboard is detached"

echo "PASS"
