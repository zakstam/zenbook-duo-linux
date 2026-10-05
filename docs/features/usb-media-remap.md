<!-- coderly-feature:v1 -->
---
slug: usb-media-remap
name: USB media key remap
bindings:
  bin: ui-tauri-react/src-tauri/src/bin/usb-media-remap.rs
  commands: ui-tauri-react/src-tauri/src/commands/usb_media_remap.rs
  controller: ui-tauri-react/src/lib/usb-media-remap-controller.ts
  helper: ui-tauri-react/src-tauri/src/usb_media_remap_helper.rs
  helperBin: ui-tauri-react/src-tauri/src/bin/zenbook-duo-usb-remap-helper.rs
  hook: ui-tauri-react/src/hooks/use-usb-media-remap.ts
---

Remaps the docked keyboard's top-row keys to media keys while it is connected over USB: a privileged helper grabs the input device and re-emits remapped key events. It can be enabled in setup or settings, paused and resumed from the Status page or tray, is stopped before sleep and retried after resume, and the daemon's monitor reconciles whether it should be running.

## Journeys

- With the keyboard docked and the remap running, holding Fn and pressing F2 renames a file in Files when the keyboard reports Fn, and the remap's startup log says whether it does. (card f92c60c9-6bc6-4208-bf41-ec4be1023657, 2026-10-05)
