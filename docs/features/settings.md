<!-- coderly-feature:v1 -->
---
slug: settings
name: Settings
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/settings.rs
  model: ui-tauri-react/src-tauri/src/models/settings.rs
  page: ui-tauri-react/src/pages/Settings.tsx
  service: ui-tauri-react/src/lib/settings-service.ts
invariants:
  - The Default Display Scale dropdown always shows the saved defaultScale as selected, even when it is not a preset, because the installer accepts any number.
---

Holds the user's preferences and the Settings page that edits them: default backlight and scale, automatic dual-screen, brightness sync, inverted sensor rotation, USB media remap, start minimized on boot, disabled touchscreens, the saved display layout and the theme preference (the theme itself is applied by the app shell).

## Journeys

- A user whose installer saved a scale of 1.75 opens Settings and sees 1.75 selected in Default Display Scale instead of an empty box. (card 9d6f1731-d4f1-4114-a4b2-12f24323f3ef, 2026-10-05)
