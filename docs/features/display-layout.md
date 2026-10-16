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
  scalePresets: ui-tauri-react/src/shared/display-scales.ts
  scales: ui-tauri-react/src-tauri/src/features/display_layout
invariants:
  - Reading the display layout (get_display_layout, backend and frontend) returns each screen where the compositor has it; only applying a layout stacks the Duo screens via normalize_display_layout. Normalizing on read hid drifted screens from the user.
---

Arranges the two internal screens and any external monitor: the Display Layout page shows a draggable canvas with resolution, refresh and scale per output, and saving applies the layout through a GNOME, KDE or Niri adapter. The saved layout is replayed when the session agent registers, when the keyboard docks or undocks, after resume, and when the lid closes (external-only clamshell) or opens. Most of the replay logic still lives in the background runtime's daemon file and the dock-mode compositor commands in the session agent file, pending a move into this feature's home.

## Journeys

- On GNOME at 1.66x scale, removing the keyboard turns the bottom screen on below the top one instead of showing a 'Logical monitors are not adjacent' error. (card ae019f84-475d-448b-8b67-a9c324046b87, 2026-10-04)
- Each display's scale dropdown lists scales in ascending order, includes 1.75, and shows the display's current scale even when it is not a preset. (card 9d6f1731-d4f1-4114-a4b2-12f24323f3ef, 2026-10-05)
- On niri, picking a scale like 1.66x and pressing Apply Layout leaves the bottom screen directly under the top one, so the mouse crosses straight down. (card 8ef9898e-1d8f-4320-86e0-84c68a7a3c7e, 2026-10-15)
- After niri moves the bottom screen beside the top one, the Display Layout page draws it to the right at niri's coordinates, and Apply Layout puts it back underneath. (card e55c3ab8-aaef-4f26-bf26-886889f0f84d, 2026-10-15)
- On the Display Layout page, changing a screen's scale keeps the bottom screen drawn flush under the top one, and after Apply the page shows where the desktop really put the screens. (card c022a42f-1b04-466e-b8be-7b7a0a2156e3, 2026-10-16)
- The scale dropdown offers 1.0x, 1.25x, 1.33x, 1.5x, 1.67x and 2.0x, and the desktop keeps whichever is picked, so the bottom screen stays flush under the top one. (card 17880f60-1d4d-48f7-8e2d-ca8d73859792, 2026-10-16)
- Logging out of niri and into Hyprland (or back) leaves rotation, dock changes and layout working in the new desktop, with no restart. (card cf1eef69-b08e-4b0c-a8a6-a0244af8f4ee, 2026-10-16)
