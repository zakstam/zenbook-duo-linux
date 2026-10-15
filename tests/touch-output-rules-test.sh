#!/usr/bin/env bash
# Each internal touch controller (finger and stylus) must be tied to its own
# screen, or niri sends both touchscreens to the first screen.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../setup-common.sh
source "${ROOT_DIR}/setup-common.sh"

rules="$(duo_touch_output_rules)"

expect_rule() {
  local controller="$1" connector="$2"
  local line="SUBSYSTEM==\"input\", KERNEL==\"event*\", ATTRS{name}==\"${controller}:00 *\", ENV{WL_OUTPUT}=\"${connector}\""
  if ! grep -Fxq "${line}" <<<"${rules}"; then
    echo "FAIL: missing touch output rule for ${controller} -> ${connector}" >&2
    echo "${rules}" >&2
    exit 1
  fi
}

# Same pairing as connector_for_touch_device in touch_mapping.rs.
expect_rule ELAN9008 eDP-1
expect_rule ELAN9009 eDP-2

if [ "$(grep -c 'WL_OUTPUT' <<<"${rules}")" -ne 2 ]; then
  echo "FAIL: expected exactly two WL_OUTPUT rules" >&2
  exit 1
fi

runtime="${ROOT_DIR}/ui-tauri-react/src-tauri/src/runtime/session_agent/touch_mapping.rs"
grep -q 'name.contains("ELAN9008")' "${runtime}" || { echo "FAIL: runtime no longer maps ELAN9008 to the primary screen" >&2; exit 1; }
grep -q 'name.contains("ELAN9009")' "${runtime}" || { echo "FAIL: runtime no longer maps ELAN9009 to the secondary screen" >&2; exit 1; }

case "${DUO_TOUCH_OUTPUT_RULES_FILE}" in
  /etc/udev/rules.d/*.rules) ;;
  *) echo "FAIL: touch output rules must install under /etc/udev/rules.d" >&2; exit 1 ;;
esac
grep -Fq "${DUO_TOUCH_OUTPUT_RULES_FILE##*/}" "${ROOT_DIR}/uninstall.sh" || {
  echo "FAIL: uninstall.sh does not remove the touch output rules" >&2
  exit 1
}

echo "touch output rules: ok"
