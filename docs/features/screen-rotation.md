<!-- coderly-feature:v1 -->
---
slug: screen-rotation
name: Screen rotation
bindings:
  watcher: ui-tauri-react/src-tauri/src/runtime/session_agent/rotation.rs
  buttons: ui-tauri-react/src/components/OrientationButtons.tsx
---

Rotates the screens to match how the laptop is held, from the accelerometer (monitor-sensor, with bounded retry when the sensor claim times out and an option to invert the sensor's left and right), or manually from the orientation buttons on the Controls page. The manual request goes through the display commands and the daemon's apply-orientation path.
