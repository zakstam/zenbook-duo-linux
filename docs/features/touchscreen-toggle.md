<!-- coderly-feature:v1 -->
---
slug: touchscreen-toggle
name: Touchscreen toggle
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/touchscreen.rs
  hardware: ui-tauri-react/src-tauri/src/hardware/touchscreen.rs
  hook: ui-tauri-react/src/hooks/use-touchscreens.ts
---

Enables or disables touch input per screen from the Controls page: each ELAN touchscreen is matched to its internal display connector, and the disabled list is kept in settings so it survives restarts.
