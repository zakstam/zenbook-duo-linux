<!-- coderly-ground-note:v1 -->
---
slug: features-profiles-layout-rs-profiles-apply-as-one-display
title: "features/profiles/layout.rs: profiles apply as one display layout"
writtenAt: 1791140572911
tablesTouched: 0
askId: ec32a902-bdb3-44be-b22f-eb501473e1a5
files:
  - path: ui-tauri-react/src-tauri/src/features/profiles/layout.rs
    blob: fa1ec273e61940fa6935217775d55b6fdbd4b1b3
  - path: ui-tauri-react/src-tauri/src/features/profiles/commands.rs
    blob: 9e078a49bc98ab5f2652d937ad656b733510de84
---

# features/profiles/layout.rs: profiles apply as one display layout

A profile is applied by building a full DisplayLayout from the current one and sending it through commands::display::apply_display_layout; leaving eDP-2 out of the layout is what turns the bottom screen off (every adapter disables omitted outputs). normalize_display_layout always stacks eDP-2 under eDP-1, so a rotation shared by both screens is fixed up afterwards with set_orientation, which on GNOME reapplies one scale to both panels. Transform degrees map 90=Left, 180=Inverted, 270=Right, the same as gnome.rs and runtime/probe.rs.
