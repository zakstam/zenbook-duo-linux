<!-- coderly-ground-note:v1 -->
---
slug: display-scales-presets-live-in-two-files-and-must-stay-clean
title: "Display scales: presets live in two files and must stay clean"
writtenAt: 1792141887027
tablesTouched: 0
askId: 17880f60-1d4d-48f7-8e2d-ca8d73859792
files:
  - path: ui-tauri-react/src-tauri/src/features/display_layout/scale.rs
    blob: 89d0c928e201d4ff98ecda6467e1ba009e62ba48
  - path: ui-tauri-react/src/shared/display-scales.ts
    blob: f3e50b05ba036c0fd24218574ca47f747d979ee9
  - path: setup-common.sh
    blob: e9097caeeda47ffb41d180681bebb4bc094f7519
---

# Display scales: presets live in two files and must stay clean

The scale presets exist twice, in Rust (features/display_layout/scale.rs) and TS (src/shared/display-scales.ts), plus the installer prompt; both lists have a test requiring each preset to be n/120 and to divide 2880x1800 evenly. Don't add 1.6/1.75/1.8: mutter doesn't offer them at 2880x1800. Any logical-size math must go through ceil_logical_pixels, never a bare (px/scale).ceil(), because desktops report 5/3 as f32. Check mutter behaviour without touching the user's screens: dbus-run-session + mutter --headless --wayland --no-x11 --virtual-monitor 2880x1800, then gdctl.
