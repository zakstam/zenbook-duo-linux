<!-- coderly-ground-note:v1 -->
---
slug: display-layout-niri-rs-niri-snaps-requested-scales-to-1-120
title: "display_layout/niri.rs: niri snaps requested scales to 1/120"
writtenAt: 1792047905749
tablesTouched: 0
askId: 8ef9898e-1d8f-4320-86e0-84c68a7a3c7e
files:
  - path: ui-tauri-react/src-tauri/src/hardware/display_layout/niri.rs
    blob: 01b84965ae054bf5a379302f2de5900ef8abb14c
  - path: ui-tauri-react/src-tauri/src/hardware/display_layout/mod.rs
    blob: 757c5c12fc8dd7f540aadb6f4b5ae7d1ee0e2409
---

# display_layout/niri.rs: niri snaps requested scales to 1/120

Niri applies a requested scale rounded to a multiple of 1/120 (1.66 -> 1.6583), so any position computed from the requested scale (normalize_display_layout, the frontend's normalizeDuoDisplays) can be off by a pixel and make niri re-place the screen to the right. apply_niri_display_layout therefore re-places a stacked eDP-2 from niri's live size of eDP-1 after applying; position neighbours from niri's reported state, never from the requested scale.
