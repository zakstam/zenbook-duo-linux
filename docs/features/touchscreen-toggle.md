<!-- coderly-feature:v1 -->
---
slug: touchscreen-toggle
name: Touchscreen toggle
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/touchscreen.rs
  hardware: ui-tauri-react/src-tauri/src/hardware/touchscreen.rs
  hook: ui-tauri-react/src/hooks/use-touchscreens.ts
  touch-mapping: ui-tauri-react/src-tauri/src/runtime/session_agent/touch_mapping.rs
---

Enables or disables touch input per screen from the Controls page: each ELAN touchscreen is matched to its internal display connector, and the disabled list is kept in settings so it survives restarts.

It also keeps each screen's touch and stylus input landing on that screen: on GNOME the session agent writes the per-device output mapping into dconf, and on niri and KDE the installer's udev rule (`91-zenbook-duo-touch-output.rules`, from `setup-common.sh`) tags each controller with its screen via `WL_OUTPUT` (ELAN9008 is eDP-1, the top screen; ELAN9009 is eDP-2, the bottom).
