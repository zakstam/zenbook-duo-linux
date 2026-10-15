<!-- coderly-feature:v1 -->
---
slug: status-dashboard
name: Status dashboard
bindings:
  card: ui-tauri-react/src/components/StatusCard.tsx
  commands: ui-tauri-react/src-tauri/src/commands/status.rs
  page: ui-tauri-react/src/pages/Status.tsx
  probe: ui-tauri-react/src-tauri/src/runtime/probe.rs
  service: ui-tauri-react/src-tauri/src/commands/service.rs
invariants:
  - "Pages and cards have no opacity or entrance animations on mount: animating opacity over backdrop-blur glass cards flickers in WebKitGTK on the Duo."
---

Shows the machine's live state on the Status page: keyboard connection, displays and orientation, Wi-Fi and Bluetooth, and whether the background service is running with its version, with a restart action that restarts the daemon and session agent units.
