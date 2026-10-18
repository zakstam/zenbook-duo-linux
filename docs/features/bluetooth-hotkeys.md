<!-- coderly-feature:v1 -->
---
slug: bluetooth-hotkeys
name: Bluetooth keyboard hotkeys
bindings:
  watcher: ui-tauri-react/src-tauri/src/runtime/bluetooth_hotkeys.rs
---

Makes the keyboard's function keys work when it is connected over Bluetooth: the daemon reads the keyboard's vendor hidraw reports and turns the backlight-cycle and brightness up/down keys into backlight and screen brightness changes, recording each as an event.

## Journeys

- On the UX8406MA keyboard over Bluetooth, holding the mic key and pressing volume changes the mic volume; other keyboards mute on a tap as before. (card c4a4a250-49e5-4ff2-82b0-64add9a6c8e0, 2026-10-18)
