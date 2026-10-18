<!-- coderly-feature:v1 -->
---
slug: keyboard-dock
name: Keyboard attach/detach
bindings:
  monitor: ui-tauri-react/src-tauri/src/runtime/monitor.rs
  policy: ui-tauri-react/src-tauri/src/runtime/policy.rs
---

Detects when the detachable keyboard is docked on the bottom screen (USB) or taken off (Bluetooth or none) and reacts: the daemon's hardware monitor polls the connection, records attach and detach events, and applies the transition policy, which replays the dock display mode (bottom screen off when docked, on when detached) and restores Wi-Fi and Bluetooth radios across a detach/attach cycle. Connection detection itself sits in the shared sysfs reader, and the dock-mode replay and compositor commands still live in the background runtime's daemon and session agent files until they move into this feature's home.

## Journeys

- Undocking the keyboard on GNOME at a fractional scale like 1.66x now switches to dual-screen mode without an error popup. (card ae019f84-475d-448b-8b67-a9c324046b87, 2026-10-04)
- Turning the docked Duo leaves the screens put, and taking the keyboard off turns them to match how the Duo is held, even with a saved layout. (card c4a4a250-49e5-4ff2-82b0-64add9a6c8e0, 2026-10-18)
