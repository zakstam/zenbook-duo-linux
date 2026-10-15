<!-- coderly-ground-note:v1 -->
---
slug: runtime-compositor-rs-niri-logical-sizes-are-rounded
title: "runtime/compositor.rs: niri logical sizes are rounded, positions are not"
writtenAt: 1792044361059
tablesTouched: 0
askId: 55c31e2c-b151-429a-98d0-03ca76875152
files:
  - path: ui-tauri-react/src-tauri/src/runtime/compositor.rs
    blob: 5a298415559d71cdb33e8a2eed69155eeb43a41f
  - path: ui-tauri-react/src-tauri/src/hardware/display_layout/niri.rs
    blob: 2a75d3a3b5e3a29a92b072ca8597a9c524a04671
---

# runtime/compositor.rs: niri logical sizes are rounded, positions are not

niri msg outputs reports logical width/height rounded to whole pixels, but places outputs with the exact fractional size (Duo at 1.66 -> niri scale 1.6583, exact 1736.7x1085.4, reported 1736x1085). An output positioned on the rounded edge overlaps by a fraction and niri silently re-places it to the right of everything. Always round mode size / scale up when positioning a neighbour; niri_output_logical_size_from_value does this now. The Control Panel's layout page does not reveal this, because normalize_display_layout always draws eDP-2 under eDP-1 whatever niri actually did.
