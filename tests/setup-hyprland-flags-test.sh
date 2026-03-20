#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TEMP_DIR}"' EXIT

HOME_DIR="${TEMP_DIR}/home/tester"
BIN_DIR="${TEMP_DIR}/bin"
mkdir -p "${HOME_DIR}" "${BIN_DIR}"

cp "${ROOT_DIR}/setup-hyprland.sh" "${TEMP_DIR}/setup-hyprland.sh"
perl -0pi -e 's/if \[ -r \/dev\/tty \]; then/if false; then/g' "${TEMP_DIR}/setup-hyprland.sh"
cat <<'EOF' > "${TEMP_DIR}/install-rust-runtime.sh"
#!/usr/bin/env bash
set -euo pipefail
printf 'install-rust-runtime\n' >> "${TEST_LOG:?}"
EOF
chmod +x "${TEMP_DIR}/install-rust-runtime.sh"

cat <<'EOF' > "${BIN_DIR}/id"
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "-u" ]; then
  printf '1000\n'
elif [ "${1:-}" = "-g" ]; then
  printf '1000\n'
else
  printf '1000\n'
fi
EOF

cat <<'EOF' > "${BIN_DIR}/getent"
#!/usr/bin/env bash
set -euo pipefail
printf 'tester:x:1000:1000:Test User:%s:/bin/bash\n' "${FAKE_HOME:?}"
EOF

cat <<'EOF' > "${BIN_DIR}/groups"
#!/usr/bin/env bash
set -euo pipefail
printf 'tester : tester input\n'
EOF

cat <<'EOF' > "${BIN_DIR}/python3"
#!/usr/bin/env bash
set -euo pipefail
printf 'python3 usb_media_remap=%s\n' "${5:-missing}" >> "${TEST_LOG:?}"
cat >/dev/null
EOF

cat <<'EOF' > "${BIN_DIR}/sudo"
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = "-u" ]; then
  shift 2
fi

cmd="${1:-}"
shift || true
base="$(basename "${cmd}")"

case "${base}" in
  dnf|apt|usermod|systemd-hwdb|udevadm|rm)
    exit 0
    ;;
  grep)
    exit 1
    ;;
  tee)
    cat >/dev/null
    exit 0
    ;;
  python3)
    exec "${cmd}" "$@"
    ;;
  mkdir)
    exec "${cmd}" "$@"
    ;;
  *)
    exit 0
    ;;
esac
EOF

chmod +x "${BIN_DIR}/id" "${BIN_DIR}/getent" "${BIN_DIR}/groups" "${BIN_DIR}/python3" "${BIN_DIR}/sudo"

run_case() {
  local flag="$1"
  local expected="$2"
  : > "${TEMP_DIR}/test.log"

  printf '\n\n\n\n' | env \
    PATH="${BIN_DIR}:/usr/bin:/bin" \
    USER=tester \
    FAKE_HOME="${HOME_DIR}" \
    TEST_LOG="${TEMP_DIR}/test.log" \
    setsid bash "${TEMP_DIR}/setup-hyprland.sh" "${flag}" >/dev/null 2>&1

  if ! grep -Fq "python3 usb_media_remap=${expected}" "${TEMP_DIR}/test.log"; then
    echo "FAIL: ${flag} should preserve usb_media_remap=${expected} when prompts use Enter defaults" >&2
    cat "${TEMP_DIR}/test.log" >&2 || true
    exit 1
  fi

  if ! grep -Fq "install-rust-runtime" "${TEMP_DIR}/test.log"; then
    echo "FAIL: setup-hyprland.sh should still run install-rust-runtime.sh" >&2
    exit 1
  fi
}

run_case --no-usb-media-remap false
run_case --usb-media-remap true

echo "PASS"
