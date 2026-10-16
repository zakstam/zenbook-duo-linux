<!-- coderly-feature:v1 -->
---
slug: touchscreen-toggle
name: Touchscreen toggle
bindings:
  commands: ui-tauri-react/src-tauri/src/commands/touchscreen.rs
  hardware: ui-tauri-react/src-tauri/src/hardware/touchscreen.rs
  hook: ui-tauri-react/src/hooks/use-touchscreens.ts
  touchMapping: ui-tauri-react/src-tauri/src/runtime/session_agent/touch_mapping.rs
---

Enables or disables touch input per screen from the Controls page: each ELAN touchscreen is matched to its internal display connector, and the disabled list is kept in settings so it survives restarts.

On GNOME and Hyprland the session agent also maps each screen's touch and stylus input to that screen (`touch_mapping.rs`): GNOME through dconf, Hyprland through `hyprctl keyword device[<name>]:output`, applied at session start, after dock mode and layout changes, and again after every Hyprland config reload (which drops runtime keywords). niri 26.04 cannot map touch per device (its libinput devices report no output, and `input { touch { map-to-output } }` is global); per-device touch config waits on upstream PR niri-wm/niri#1856. ELAN9008 is eDP-1 (top), ELAN9009 is eDP-2 (bottom).
