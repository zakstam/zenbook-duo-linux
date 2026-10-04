<!-- coderly-ground-note:v1 -->
---
slug: runtime-daemon-rs-and-session-agent-rs-carry-display-layout
title: runtime/daemon.rs and session_agent.rs carry display-layout logic
writtenAt: 1791137861557
tablesTouched: 0
askId: 994ea0a8-29bf-4c74-8e71-0ec3eac846c5
files:
  - path: ui-tauri-react/src-tauri/src/runtime/daemon.rs
    blob: b7355d7c29f78f89d3a7d25b0f60c4d477dd2170
  - path: ui-tauri-react/src-tauri/src/runtime/session_agent.rs
    blob: 47b43710fb5df8ed4107547eddf1f4baa1e6fd16
  - path: docs/features/display-layout.md
    blob: 633b842648938b56970668fc69880ff9608c1e51
---

# runtime/daemon.rs and session_agent.rs carry display-layout logic

daemon.rs (2.8k lines) is mostly display replay (dock mode, lid close/open clamshell, resume, session registration), and session_agent.rs holds the GNOME/KDE/Niri dock-mode commands. The registry assigns both files to the background-service-runtime platform because they are also the daemon and session-agent entry points. A card that moves display-layout into src-tauri/src/features/display-layout should extract that logic and leave the entry points behind.
