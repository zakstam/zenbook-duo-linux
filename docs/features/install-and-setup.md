<!-- coderly-feature:v1 -->
---
slug: install-and-setup
name: Install and setup
bindings:
  bumpVersion: bump-version.sh
  check: check.sh
  install: install.sh
  installRuntime: install-rust-runtime.sh
  installTests: tests
  installUi: install-ui.sh
  setupCommon: setup-common.sh
  setupGnome: setup-gnome.sh
  setupKde: setup-kde.sh
  setupNiri: setup-niri.sh
  setupPage: ui-tauri-react/src/pages/Setup.tsx
  uninstall: uninstall.sh
---

Gets the app onto a machine and ready: shell installers build and install the Rust runtime and the Tauri app, set up systemd units, udev and desktop-specific pieces for GNOME, KDE or Niri, and uninstall cleanly; the first-run Setup page asks for theme and USB media remap and marks setup complete. Release and check scripts live here too.

## Journeys

- Running the installer with the keyboard detached and asleep finishes successfully and leaves the session agent running the new version; if a service does fail, the installer names it and shows its log lines. (card 328c0178-7652-479a-82e0-141204f0e032, 2026-10-15)
