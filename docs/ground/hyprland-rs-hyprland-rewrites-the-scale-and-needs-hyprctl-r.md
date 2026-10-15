<!-- coderly-ground-note:v1 -->
---
slug: hyprland-rs-hyprland-rewrites-the-scale-and-needs-hyprctl-r
title: "hyprland.rs: Hyprland rewrites the scale and needs hyprctl -r"
writtenAt: 1792072410591
tablesTouched: 0
askId: c022a42f-1b04-466e-b8be-7b7a0a2156e3
files:
  - path: ui-tauri-react/src-tauri/src/hardware/display_layout/hyprland.rs
    blob: 227dc151279976b19ed9bdcbf5c9dede25ee307b
---

# hyprland.rs: Hyprland rewrites the scale and needs hyprctl -r

Hyprland 0.56 applies only scales that divide the mode cleanly (1.66 -> 5/3, 1.75 -> 1.8 on 2880x1800), so stack the lower panel from the size Hyprland reports after applying (stacked_lower_correction). In a nested Hyprland, `hyprctl keyword monitor` without -r was not applied until a reload; the adapter now passes -r. Test it with a nested `Hyprland --config` plus `hyprctl output create headless NAME` and ZENBOOK_DUO_UPPER/LOWER_CONNECTOR pointing at those outputs.
