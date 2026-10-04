<!-- coderly-ground-note:v1 -->
---
slug: usb-media-remap-helper-rs-fn-routing-and-the-unmeasured
title: "usb_media_remap_helper.rs: Fn routing and the unmeasured hardware"
writtenAt: 1791141227568
tablesTouched: 0
askId: f92c60c9-6bc6-4208-bf41-ec4be1023657
files:
  - path: ui-tauri-react/src-tauri/src/usb_media_remap_helper.rs
    blob: a276fcc9300c42d0e0c8d11320b911b26969b524
---

# usb_media_remap_helper.rs: Fn routing and the unmeasured hardware

Keys are routed by FnPassthrough: a key pressed while Fn is held or the remap is paused stays raw until its release, so press and release always match. Never route a release by the current Fn/pause state. Fn is read from the grabbed node and from sibling by-id nodes that advertise KEY_FN, which are opened read-only and must never be grabbed. Nobody has confirmed that the docked keyboard reports Fn over USB. Check the startup log line before building on it; if Fn is invisible, pause is the only way to reach F1-F12.
