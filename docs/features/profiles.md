<!-- coderly-feature:v1 -->
---
slug: profiles
name: Profiles
bindings:
  backend: ui-tauri-react/src-tauri/src/features/profiles
  frontend: ui-tauri-react/src/features/profiles
invariants:
  - Activating a profile applies backlight, top-screen scale and rotation, bottom screen on/off, and bottom-screen scale and rotation in one display layout; external monitors are left as they are.
  - Save Current records each built-in screen's live scale and rotation and whether the bottom screen is on, never the settings default scale.
  - A profiles.json without bottomScale/bottomOrientation still loads; the bottom screen then follows the top screen's values.
  - Built-in profiles use the scale chosen at setup until edited, and edits (to any profile) persist in profiles.json across restarts.
  - "The built-in Presentation profile flips the top screen upside down and keeps the bottom screen normal (#18)."
---

Saves named hardware states (keyboard backlight; top screen scale and rotation; bottom screen on/off, scale and rotation) and applies one with a click from the Profiles page or the tray menu. Save Current captures the screens as they are; any profile, including the built-in Docked, Tablet and Presentation, can be edited.
