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
touch "${target}/setup-common.sh" "${target}/setup-gnome.sh" "${target}/setup-kde.sh" "${target}/setup-niri.sh" "${target}/install-ui.sh"
EOF
chmod +x "${fake_bin}/git"

checkout_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  export PATH="'"${fake_bin}"':/usr/bin:/bin"
  source <(sed -n "1,128p" "'"${ROOT_DIR}"'/install.sh")
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

desktop_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  source <(sed -n "1,128p" "'"${ROOT_DIR}"'/install.sh")
  XDG_CURRENT_DESKTOP=GNOME DESKTOP_SESSION= XDG_SESSION_DESKTOP= pick_desktop
  XDG_CURRENT_DESKTOP="KDE Plasma" DESKTOP_SESSION= XDG_SESSION_DESKTOP= pick_desktop
  XDG_CURRENT_DESKTOP= DESKTOP_SESSION= XDG_SESSION_DESKTOP=niri pick_desktop
' 2>/dev/null)" || {
  echo "FAIL: pick_desktop should detect all supported desktops" >&2
  exit 1
}

if [[ "${desktop_output}" != $'gnome\nkde\nniri' ]]; then
  echo "FAIL: pick_desktop should map GNOME, KDE, and Niri consistently" >&2
  exit 1
fi

assert_setup_packages() {
  local label="$1"
  local dnf_desktop="$2"
  local apt_desktop="$3"
  local pacman_desktop="$4"
  local manual_hint="$5"
  local output=""

  output="$(bash --noprofile --norc -c '
    set -euo pipefail
    source "$1"
    DNF_DESKTOP_PACKAGES=("$2")
    APT_DESKTOP_PACKAGES=("$3")
    PACMAN_DESKTOP_PACKAGES=("$4")
    MANUAL_DESKTOP_DEPENDENCIES_HINT="$5"
    build_duo_setup_package_lists
    printf "dnf:%s\napt:%s\npacman:%s\nhint:%s\n" \
      "${DNF_PACKAGES[*]}" \
      "${APT_PACKAGES[*]}" \
      "${PACMAN_PACKAGES[*]}" \
      "${MANUAL_DEPENDENCIES_HINT}"
  ' _ "${ROOT_DIR}/setup-common.sh" "${dnf_desktop}" "${apt_desktop}" "${pacman_desktop}" "${manual_hint}")" || {
    echo "FAIL: setup package matrix should build for ${label}" >&2
    exit 1
  }

  local expected=$'dnf:usbutils iio-sensor-proxy systemd '"${dnf_desktop}"$'\napt:usbutils iio-sensor-proxy systemd '"${apt_desktop}"$'\npacman:usbutils iio-sensor-proxy systemd '"${pacman_desktop}"$'\nhint:usbutils, iio-sensor-proxy, systemd, '"${manual_hint}"
  if [[ "${output}" != "${expected}" ]]; then
    echo "FAIL: setup package matrix mismatch for ${label}" >&2
    echo "Got:" >&2
    echo "${output}" >&2
    exit 1
  fi
}

assert_setup_packages gnome mutter mutter-common-bin mutter mutter/gdctl
assert_setup_packages kde kscreen kscreen kscreen kscreen/kscreen-doctor
assert_setup_packages niri niri niri niri niri

if ! grep -q 'WantedBy=default.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should be enabled from default.target" >&2
  exit 1
fi

if grep -q 'Wants=graphical-session.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should not pull in graphical-session.target" >&2
  exit 1
fi

if ! grep -q 'After=graphical-session.target' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: user service should still order itself after graphical-session.target" >&2
  exit 1
fi

if ! grep -q 'import-environment DISPLAY WAYLAND_DISPLAY NIRI_SOCKET XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: installer should import graphical session environment for the user manager" >&2
  exit 1
fi

if ! grep -q 'SYSTEM_SLEEP_HOOK_PATH="/usr/lib/systemd/system-sleep/zenbook-duo-rust-lifecycle"' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: installer should install the Rust lifecycle sleep hook" >&2
  exit 1
fi

if ! grep -q 'ln -sfn "${RUNTIME_INSTALL_DIR}/zenbook-duo-lifecycle" "${SYSTEM_SLEEP_HOOK_PATH}"' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: lifecycle binary should be the single system-sleep entrypoint" >&2
  exit 1
fi

if grep -q 'ExecStart=.*\(pre\|post\|thaw\|hibernate\)' "${ROOT_DIR}/install-rust-runtime.sh"; then
  echo "FAIL: lifecycle service should not duplicate suspend/resume handling" >&2
  exit 1
fi

if ! grep -q 'command -v pacman' "${ROOT_DIR}/setup-common.sh"; then
  echo "FAIL: shared setup helper should support pacman-based systems" >&2
  exit 1
fi

if ! grep -q 'DUO_COMMON_DNF_PACKAGES=(usbutils iio-sensor-proxy systemd)' "${ROOT_DIR}/setup-common.sh"; then
  echo "FAIL: shared setup helper should centralize common dnf dependencies" >&2
  exit 1
fi

remap_default_output="$(bash --noprofile --norc -c '
  set -euo pipefail
  source "'"${ROOT_DIR}"'/setup-common.sh"
  duo_prompt() { printf -v "$2" ""; }

  DEFAULT_BACKLIGHT=0
  DEFAULT_SCALE=1.66
  USB_MEDIA_REMAP_ENABLED=false
  prompt_duo_defaults
  printf "%s\n" "${USB_MEDIA_REMAP_ENABLED}"

  DEFAULT_BACKLIGHT=0
  DEFAULT_SCALE=1.66
  USB_MEDIA_REMAP_ENABLED=true
  prompt_duo_defaults
  printf "%s\n" "${USB_MEDIA_REMAP_ENABLED}"
' 2>&1)" || {
  echo "FAIL: prompt_duo_defaults should be callable with mocked prompts" >&2
  exit 1
}

if [[ "${remap_default_output}" != $'false\ntrue' ]]; then
  echo "FAIL: empty USB remap prompt answer should preserve the current default" >&2
  printf 'Got:\n%s\n' "${remap_default_output}" >&2
  exit 1
fi

for setup_script in setup-gnome.sh setup-kde.sh setup-niri.sh; do
  if ! grep -q 'source "${DUO_SETUP_DIR}/setup-common.sh"' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should delegate to setup-common.sh" >&2
    exit 1
  fi
  if ! grep -q 'DNF_DESKTOP_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare only desktop-specific dnf dependencies" >&2
    exit 1
  fi
  if ! grep -q 'APT_DESKTOP_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare only desktop-specific apt dependencies" >&2
    exit 1
  fi
  if ! grep -q 'PACMAN_DESKTOP_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should declare only desktop-specific pacman dependencies" >&2
    exit 1
  fi
  if grep -q '^DNF_PACKAGES=' "${ROOT_DIR}/${setup_script}" || grep -q '^APT_PACKAGES=' "${ROOT_DIR}/${setup_script}" || grep -q '^PACMAN_PACKAGES=' "${ROOT_DIR}/${setup_script}"; then
    echo "FAIL: ${setup_script} should not duplicate common package arrays" >&2
    exit 1
  fi
done

if ! grep -q 'PKG_MGR="pacman"' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should detect pacman-based systems" >&2
  exit 1
fi

if ! grep -q 'install_ui_direct' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should have a direct-install path for pacman systems" >&2
  exit 1
fi

if ! grep -q 'command -v pkexec' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should prefer pkexec when a graphical auth prompt is available" >&2
  exit 1
fi

if ! grep -q -- '--root-install-prereqs' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should expose a root-only prereq helper entrypoint" >&2
  exit 1
fi

if ! grep -q -- '--root-install-package' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should expose a root-only package install helper entrypoint" >&2
  exit 1
fi

if ! grep -q -- '--root-install-direct' "${ROOT_DIR}/install-ui.sh"; then
  echo "FAIL: install-ui.sh should expose a root-only direct install helper entrypoint" >&2
  exit 1
fi

if ! grep -q 'sudo pacman -Rns --noconfirm zenbook-duo-control' "${ROOT_DIR}/uninstall.sh"; then
  echo "FAIL: uninstall.sh should try pacman removal when available" >&2
  exit 1
fi

echo "PASS"
