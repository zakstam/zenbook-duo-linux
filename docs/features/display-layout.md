<!-- coderly-feature:v1 -->
---
slug: display-layout
name: Dual-screen display layout
bindings:
  adapters: ui-tauri-react/src-tauri/src/hardware/display_layout
  commands: ui-tauri-react/src-tauri/src/commands/display.rs
  planner: ui-tauri-react/src-tauri/src/runtime/session_display_planner.rs
  lid: ui-tauri-react/src-tauri/src/runtime/logind.rs
  page: ui-tauri-react/src/pages/DisplayLayout.tsx
  canvas: ui-tauri-react/src/components/DisplayCanvas.tsx
  layoutModel: ui-tauri-react/src/lib/display-layout.ts
  controller: ui-tauri-react/src/lib/display-layout-controller.ts
---

Arranges the two internal screens and any external monitor: the Display Layout page shows a draggable canvas with resolution, refresh and scale per output, and saving applies the layout through a GNOME, KDE or Niri adapter. The saved layout is replayed when the session agent registers, when the keyboard docks or undocks, after resume, and when the lid closes (external-only clamshell) or opens. Most of the replay logic still lives in the background runtime's daemon file and the dock-mode compositor commands in the session agent file, pending a move into this feature's home.
