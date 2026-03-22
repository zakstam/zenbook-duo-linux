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

empty_array_output="$(bash --noprofile --norc -c 'set -euo pipefail; BASH_SOURCE=(); source <(head -n 9 "'"${ROOT_DIR}"'/install.sh"); printf "%s\n" "${SCRIPT_DIR}"' 2>&1)" || {
  echo "FAIL: init should survive an empty BASH_SOURCE array" >&2
  exit 1
}

if [[ "${empty_array_output}" != "$(pwd)" ]]; then
  echo "FAIL: empty-array fallback should use current working directory" >&2
  exit 1
fi

temp_root="$(mktemp -d)"
trap 'rm -rf "${temp_root}"' EXIT
fake_bin="${temp_root}/bin"
mkdir -p "${fake_bin}"

cat > "${fake_bin}/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "clone" ]]; then
  echo "unexpected git args: $*" >&2
  exit 1
fi
target="${@: -1}"
mkdir -p "${target}"
touch "${target}/setup-gnome.sh" "${target}/setup-kde.sh" "${target}/setup-niri.sh" "${target}/install-ui.sh"
EOF
chmod +x "${fake_bin}/git"

checkout_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  export PATH="'"${fake_bin}"':/usr/bin:/bin"
  source <(sed -n "1,126p" "'"${ROOT_DIR}"'/install.sh")
  SCRIPT_DIR="'"${temp_root}"'/missing-checkout"
  ensure_repo_checkout
' 2>/dev/null)" || {
  echo "FAIL: ensure_repo_checkout should succeed with fake git" >&2
  exit 1
}

if [[ "${checkout_output}" == *"Downloading zenbook-duo-linux..."* ]]; then
  echo "FAIL: ensure_repo_checkout should not mix status logs into stdout" >&2
  exit 1
fi

if [[ ! -f "${checkout_output}/setup-niri.sh" ]]; then
  echo "FAIL: ensure_repo_checkout should print the checkout path" >&2
  exit 1
fi

echo "PASS"
