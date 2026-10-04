<!-- coderly-feature:v1 -->
---
slug: input-diagnostics
name: Input diagnostics
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/diagnostics.rs
  page: ui-tauri-react/src/pages/Diagnostics.tsx
---

A troubleshooting tool on the Diagnostics page that lists evdev and HID devices, reads HID report descriptors, and captures raw key events (including privileged hidraw capture through pkexec), used to find out what a key on the keyboard actually sends.
