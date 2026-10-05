<!-- coderly-feature:v1 -->
---
slug: display-layout
name: Dual-screen display layout
bindings:
  adapters: ui-tauri-react/src-tauri/src/hardware/display_layout
  canvas: ui-tauri-react/src/components/DisplayCanvas.tsx
  commands: ui-tauri-react/src-tauri/src/commands/display.rs
  controller: ui-tauri-react/src/lib/display-layout-controller.ts
  layoutModel: ui-tauri-react/src/lib/display-layout.ts
  lid: ui-tauri-react/src-tauri/src/runtime/logind.rs
  page: ui-tauri-react/src/pages/DisplayLayout.tsx
  planner: ui-tauri-react/src-tauri/src/runtime/session_display_planner.rs
---

Arranges the two internal screens and any external monitor: the Display Layout page shows a draggable canvas with resolution, refresh and scale per output, and saving applies the layout through a GNOME, KDE or Niri adapter. The saved layout is replayed when the session agent registers, when the keyboard docks or undocks, after resume, and when the lid closes (external-only clamshell) or opens. Most of the replay logic still lives in the background runtime's daemon file and the dock-mode compositor commands in the session agent file, pending a move into this feature's home.

## Journeys

- On GNOME at 1.66x scale, removing the keyboard turns the bottom screen on below the top one instead of showing a 'Logical monitors are not adjacent' error. (card ae019f84-475d-448b-8b67-a9c324046b87, 2026-10-04)
- Each display's scale dropdown lists scales in ascending order, includes 1.75, and shows the display's current scale even when it is not a preset. (card 9d6f1731-d4f1-4114-a4b2-12f24323f3ef, 2026-10-05)
