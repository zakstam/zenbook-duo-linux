<!-- coderly-feature:v1 -->
---
slug: screen-rotation
name: Screen rotation
bindings:
  buttons: ui-tauri-react/src/components/OrientationButtons.tsx
  lyingFlat: ui-tauri-react/src-tauri/src/runtime/session_agent/lying_flat.rs
  watcher: ui-tauri-react/src-tauri/src/runtime/session_agent/rotation.rs
---

Rotates the screens to match how the laptop is held, from the accelerometer (monitor-sensor, with bounded retry when the sensor claim times out and an option to invert the sensor's left and right), or manually from the orientation buttons on the Controls page. When the Duo lies opened flat on a table (the raw accel_3d reading shows the top screen facing up), the watcher turns only the top screen around and leaves the bottom one upright, then returns to the sensor's orientation when it is picked up. The manual request goes through the display commands and the daemon's apply-orientation path.

## Journeys

- Rotation pauses while the keyboard is docked and is applied on detach; on KDE the 2026 Duo's top screen stays upright when docking. (card c4a4a250-49e5-4ff2-82b0-64add9a6c8e0, 2026-10-18)
