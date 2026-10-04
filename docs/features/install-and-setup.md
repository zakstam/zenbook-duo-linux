<!-- coderly-feature:v1 -->
---
slug: install-and-setup
name: Install and setup
bindings:
  install: install.sh
  installRuntime: install-rust-runtime.sh
  installUi: install-ui.sh
  uninstall: uninstall.sh
  setupCommon: setup-common.sh
  setupGnome: setup-gnome.sh
  setupKde: setup-kde.sh
  setupNiri: setup-niri.sh
  check: check.sh
  bumpVersion: bump-version.sh
  installTests: tests
  setupPage: ui-tauri-react/src/pages/Setup.tsx
---

Gets the app onto a machine and ready: shell installers build and install the Rust runtime and the Tauri app, set up systemd units, udev and desktop-specific pieces for GNOME, KDE or Niri, and uninstall cleanly; the first-run Setup page asks for theme and USB media remap and marks setup complete. Release and check scripts live here too.
