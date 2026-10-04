<!-- coderly-feature:v1 -->
---
slug: usb-media-remap
name: USB media key remap
bindings:
  helper: ui-tauri-react/src-tauri/src/usb_media_remap_helper.rs
  commands: ui-tauri-react/src-tauri/src/commands/usb_media_remap.rs
  bin: ui-tauri-react/src-tauri/src/bin/usb-media-remap.rs
  helperBin: ui-tauri-react/src-tauri/src/bin/zenbook-duo-usb-remap-helper.rs
  hook: ui-tauri-react/src/hooks/use-usb-media-remap.ts
  controller: ui-tauri-react/src/lib/usb-media-remap-controller.ts
---

Remaps the docked keyboard's top-row keys to media keys while it is connected over USB: a privileged helper grabs the input device and re-emits remapped key events. It can be enabled in setup or settings, paused and resumed from the Status page or tray, is stopped before sleep and retried after resume, and the daemon's monitor reconciles whether it should be running.
