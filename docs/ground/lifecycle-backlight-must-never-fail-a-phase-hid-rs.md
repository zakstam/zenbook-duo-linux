<!-- coderly-ground-note:v1 -->
---
slug: lifecycle-backlight-must-never-fail-a-phase-hid-rs
title: Lifecycle backlight must never fail a phase (hid.rs)
writtenAt: 1792071586152
tablesTouched: 0
askId: 328c0178-7652-479a-82e0-141204f0e032
files:
  - path: ui-tauri-react/src-tauri/src/hardware/hid.rs
    blob: 012f48c570db97455d1c33cd2a14afb792cdd525
  - path: ui-tauri-react/src-tauri/src/runtime/daemon.rs
    blob: aa8891a950f685fedad7842b5ed32567036e9e21
---

# Lifecycle backlight must never fail a phase (hid.rs)

A detached Duo keyboard paired over Bluetooth but asleep is detected as present (ConnectionType::Bluetooth) yet HIDIOCSFEATURE returns EIO. The lifecycle oneshot unit is restarted by ./install.sh, so any error there used to abort the install before the session agent restart. set_backlight_if_keyboard_present therefore returns Option<warning>, never an error; don't reintroduce `?` on it.
