<!-- coderly-feature:v1 -->
---
slug: keyboard-backlight
name: Keyboard backlight
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/backlight.rs
  hid: ui-tauri-react/src-tauri/src/hardware/hid.rs
  slider: ui-tauri-react/src/components/BacklightSlider.tsx
---

Sets the detachable keyboard's backlight level (off to 3) from the Controls page slider or the tray menu, writing a HID report over USB when docked or over Bluetooth hidraw when detached. The daemon turns the backlight off before sleep and shutdown, and settings hold a default level.
