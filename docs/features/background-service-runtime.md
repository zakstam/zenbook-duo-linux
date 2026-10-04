<!-- coderly-feature:v1 -->
---
slug: background-service-runtime
name: Background service runtime
kind: platform
overrideReason: The daemon, session agent, IPC, shared hardware readers and data models run every feature and belong to none; the layering decision names them as platform.
bindings:
  runtime: ui-tauri-react/src-tauri/src/runtime
  ipc: ui-tauri-react/src-tauri/src/ipc
  hardware: ui-tauri-react/src-tauri/src/hardware
  models: ui-tauri-react/src-tauri/src/models
  bin: ui-tauri-react/src-tauri/src/bin
  commandsMod: ui-tauri-react/src-tauri/src/commands/mod.rs
  main: ui-tauri-react/src-tauri/src/main.rs
  build: ui-tauri-react/src-tauri/build.rs
---

What every feature runs on: the root daemon (socket server, request router, persisted runtime state, lifecycle handling for sleep, resume and shutdown, notifications), the per-user session agent that runs compositor commands and the rotation and brightness watchers, the lifecycle binary, the IPC protocol between them, desktop-session detection, compositor command helpers, logging and paths, the shared sysfs and connector readers, and the shared data models. lib.rs, the Tauri registry that wires every feature's commands and the tray menu, is a declared registry and owned by no unit. The daemon and session agent files still carry display-replay and dock-mode logic that belongs to the display layout feature.
