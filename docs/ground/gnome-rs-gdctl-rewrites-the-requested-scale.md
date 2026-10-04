<!-- coderly-ground-note:v1 -->
---
slug: gnome-rs-gdctl-rewrites-the-requested-scale
title: "gnome.rs: gdctl rewrites the requested scale"
writtenAt: 1791138898418
tablesTouched: 0
askId: ae019f84-475d-448b-8b67-a9c324046b87
files:
  - path: ui-tauri-react/src-tauri/src/hardware/display_layout/gnome.rs
    blob: 4b2b45f076261d4221c0c77b98d1595c6f24b772
  - path: ui-tauri-react/src-tauri/src/runtime/session_agent.rs
    blob: 2d0c5e5530e933f0810bb4f8508506ca04b7606b
---

# gnome.rs: gdctl rewrites the requested scale

gdctl snaps any --scale to the nearest scale mutter supports for the mode (within 0.1), e.g. 1.66 becomes 5/3 or 1.7476 on 2880x1800. Any absolute --x/--y derived from the requested scale (ceil(1800/1.66)) is wrong, and mutter rejects gaps as 'Logical monitors are not adjacent'. Place stacked panels with --below/--right-of, which gdctl computes from the applied scale, and list the reference monitor first on the command line.
