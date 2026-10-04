<!-- coderly-feature:v1 -->
---
slug: brightness-sync
name: Brightness sync between screens
bindings:
  watcher: ui-tauri-react/src-tauri/src/runtime/session_agent/brightness_sync.rs
---

Keeps the bottom screen's brightness equal to the top screen's: while the keyboard is detached and sync is enabled in settings, the session agent polls the primary backlight every second and copies changes to the secondary panel.
