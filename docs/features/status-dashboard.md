<!-- coderly-feature:v1 -->
---
slug: status-dashboard
name: Status dashboard
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/status.rs
  service: ui-tauri-react/src-tauri/src/commands/service.rs
  probe: ui-tauri-react/src-tauri/src/runtime/probe.rs
  page: ui-tauri-react/src/pages/Status.tsx
  card: ui-tauri-react/src/components/StatusCard.tsx
---

Shows the machine's live state on the Status page: keyboard connection, displays and orientation, Wi-Fi and Bluetooth, and whether the background service is running with its version, with a restart action that restarts the daemon and session agent units.
