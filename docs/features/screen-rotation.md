<!-- coderly-feature:v1 -->
---
slug: screen-rotation
name: Screen rotation
bindings:
  watcher: ui-tauri-react/src-tauri/src/runtime/session_agent/rotation.rs
  lyingFlat: ui-tauri-react/src-tauri/src/runtime/session_agent/lying_flat.rs
  buttons: ui-tauri-react/src/components/OrientationButtons.tsx
---

Rotates the screens to match how the laptop is held, from the accelerometer (monitor-sensor, with bounded retry when the sensor claim times out and an option to invert the sensor's left and right), or manually from the orientation buttons on the Controls page. When the Duo lies opened flat on a table (the raw accel_3d reading shows the top screen facing up), the watcher turns only the top screen around and leaves the bottom one upright, then returns to the sensor's orientation when it is picked up. The manual request goes through the display commands and the daemon's apply-orientation path.
