<!-- coderly-ground-note:v1 -->
---
slug: session-agent-lying-flat-rs-duo-sensor-facts-behind-lying
title: "session_agent/lying_flat.rs: Duo sensor facts behind lying-flat detection"
writtenAt: 1792043141374
tablesTouched: 0
askId: 55c31e2c-b151-429a-98d0-03ca76875152
files:
  - path: ui-tauri-react/src-tauri/src/runtime/session_agent/lying_flat.rs
    blob: 284cf17e9e85d9dfd09b0934a40926a20f78c162
  - path: ui-tauri-react/src-tauri/src/runtime/session_agent/rotation.rs
    blob: f3dfd909e019ccb4a15552949704bab28734634d
---

# session_agent/lying_flat.rs: Duo sensor facts behind lying-flat detection

monitor-sensor emits nothing when the Duo goes flat, so lying flat is read from accel_3d's raw sysfs axes (no root); the top panel's z axis points out of its back, so face-up reads negative z. Thresholds come from a UX8406MA: flat -0.99 g, laptop -0.18 to -0.32, kickstand ~0, side ~0. The UX8406 also has an IIO 'hinge' sensor (iio name hinge, channels hinge/screen/keyboard, hinge = 360 - (keyboard - screen)), but one-shot sysfs reads always return 0: it only works through the IIO buffer (root), with hysteresis 0, and the kernel's hid-sensor-custom-intel-hinge reads each 16-bit field as u32, so take value & 0xFFFF. The UX8406 hinge cannot fold past flat, so there is no tent posture.
