<!-- coderly-feature:v1 -->
---
slug: bluetooth-hotkeys
name: Bluetooth keyboard hotkeys
bindings:
  watcher: ui-tauri-react/src-tauri/src/runtime/bluetooth_hotkeys.rs
---

Makes the keyboard's function keys work when it is connected over Bluetooth: the daemon reads the keyboard's vendor hidraw reports and turns the backlight-cycle and brightness up/down keys into backlight and screen brightness changes, recording each as an event.
