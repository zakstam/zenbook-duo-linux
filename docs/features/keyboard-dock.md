<!-- coderly-feature:v1 -->
---
slug: keyboard-dock
name: Keyboard attach/detach
bindings:
  monitor: ui-tauri-react/src-tauri/src/runtime/monitor.rs
  policy: ui-tauri-react/src-tauri/src/runtime/policy.rs
---

Detects when the detachable keyboard is docked on the bottom screen (USB) or taken off (Bluetooth or none) and reacts: the daemon's hardware monitor polls the connection, records attach and detach events, and applies the transition policy, which replays the dock display mode (bottom screen off when docked, on when detached) and restores Wi-Fi and Bluetooth radios across a detach/attach cycle. Connection detection itself sits in the shared sysfs reader, and the dock-mode replay and compositor commands still live in the background runtime's daemon and session agent files until they move into this feature's home.
