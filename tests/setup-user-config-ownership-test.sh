#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TEMP_DIR}"' EXIT

BIN_DIR="${TEMP_DIR}/bin"
HOME_DIR="${TEMP_DIR}/home/tester"
mkdir -p "${BIN_DIR}" "${HOME_DIR}"

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
printf 'python3 user=%s path=%s\n' "$(id -un 2>/dev/null || printf unknown)" "${2:-missing}" >> "${TEST_LOG:?}"
cat >/dev/null
EOF

cat <<'EOF' > "${BIN_DIR}/mkdir"
#!/usr/bin/env bash
set -euo pipefail
printf 'mkdir %s\n' "$*" >> "${TEST_LOG:?}"
exit 0
EOF

cat <<'EOF' > "${BIN_DIR}/sudo"
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = "-u" ]; then
  target_user="${2:-}"
  shift 2
  cmd="${1:-}"
  shift || true
  base="$(basename "${cmd}")"
  printf 'sudo-user %s %s %s\n' "${target_user}" "${base}" "$*" >> "${TEST_LOG:?}"
  case "${base}" in
    python3|mkdir|grep|tee)
      exit 0
      ;;
    *)
      exit 0
      ;;
  esac
fi

cmd="${1:-}"
shift || true
base="$(basename "${cmd}")"
printf 'sudo %s %s\n' "${base}" "$*" >> "${TEST_LOG:?}"
case "${base}" in
  pacman|dnf|apt|usermod|systemd-hwdb|udevadm|rm|cp|chown|chmod|install|systemctl)
    exit 0
    ;;
  grep)
    exit 1
    ;;
  tee)
    cat >/dev/null
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
EOF

chmod +x "${BIN_DIR}/id" "${BIN_DIR}/getent" "${BIN_DIR}/groups" "${BIN_DIR}/python3" \
  "${BIN_DIR}/mkdir" "${BIN_DIR}/sudo"

cat <<'EOF' > "${TEMP_DIR}/install-rust-runtime.sh"
#!/usr/bin/env bash
set -euo pipefail
exit 0
EOF
chmod +x "${TEMP_DIR}/install-rust-runtime.sh"

run_case() {
  local script_name="$1"
  local script_copy="${TEMP_DIR}/${script_name}"
  cp "${ROOT_DIR}/${script_name}" "${script_copy}"
  perl -0pi -e 's/if \[ -r \/dev\/tty \]; then/if false; then/g' "${script_copy}"
  perl -0pi -e 's/\[ "\$\{EUID\}" = "0" \]/true/g' "${script_copy}"

  : > "${TEMP_DIR}/test.log"

  if ! printf '\n\n\n\n' | env \
    PATH="${BIN_DIR}:/usr/bin:/bin" \
    USER=root \
    SUDO_USER=tester \
    FAKE_HOME="${HOME_DIR}" \
    TEST_LOG="${TEMP_DIR}/test.log" \
    setsid bash "${script_copy}" >/dev/null 2>&1; then
    echo "FAIL: ${script_name} should succeed in sudo-style mode" >&2
    exit 1
  fi

  if grep -Fxq "mkdir -p ${HOME_DIR}/.config/zenbook-duo" "${TEMP_DIR}/test.log"; then
    echo "FAIL: ${script_name} should not create the user config directory as root" >&2
    cat "${TEMP_DIR}/test.log" >&2 || true
    exit 1
  fi

  if ! grep -Fq "sudo-user tester mkdir -p ${HOME_DIR}/.config/zenbook-duo" "${TEMP_DIR}/test.log"; then
    echo "FAIL: ${script_name} should create the user config directory via sudo -u tester" >&2
    cat "${TEMP_DIR}/test.log" >&2 || true
    exit 1
  fi

  if ! grep -Fq "sudo chown -R tester:tester ${HOME_DIR}/.config/zenbook-duo" "${TEMP_DIR}/test.log"; then
    echo "FAIL: ${script_name} should repair ownership for an existing root-owned config directory" >&2
    cat "${TEMP_DIR}/test.log" >&2 || true
    exit 1
  fi
}

run_case setup-gnome.sh
run_case setup-kde.sh
run_case setup-hyprland.sh
run_case setup-niri.sh

echo "PASS"
