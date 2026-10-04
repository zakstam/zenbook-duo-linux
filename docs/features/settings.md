<!-- coderly-feature:v1 -->
---
slug: settings
name: Settings
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/settings.rs
  model: ui-tauri-react/src-tauri/src/models/settings.rs
  page: ui-tauri-react/src/pages/Settings.tsx
  service: ui-tauri-react/src/lib/settings-service.ts
---

Holds the user's preferences and the Settings page that edits them: default backlight and scale, automatic dual-screen, brightness sync, inverted sensor rotation, USB media remap, start minimized on boot, disabled touchscreens, the saved display layout and the theme preference (the theme itself is applied by the app shell).
