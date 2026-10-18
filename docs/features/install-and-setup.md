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
- The installer's scale prompt lists 1, 1.25, 1.33, 1.5, 1.67 and 2, with 1.67 as the default. (card 17880f60-1d4d-48f7-8e2d-ca8d73859792, 2026-10-16)
- Installing once from niri also sets the app up for Hyprland when it is installed, and a terminal still carrying an earlier desktop's settings no longer makes the installer give up. (card cf1eef69-b08e-4b0c-a8a6-a0244af8f4ee, 2026-10-16)
- Running the installer writes the brightness sudoers rules through root-owned temp files, so no program running as the user can slip its own rules in during install. (card 4070bb5c-c90c-451e-bf78-f0e4f25bba3b, 2026-10-17)
- A Duo owner can read the README top table to see whether their model, desktop and distro are supported, and install, troubleshoot or uninstall from it alone. (card 5b8650f5-37b0-4845-863e-5b518f87b48f, 2026-10-18)
- Reinstalling keeps the saved settings as defaults and removes the old brightness sudoers rules. (card c4a4a250-49e5-4ff2-82b0-64add9a6c8e0, 2026-10-18)
