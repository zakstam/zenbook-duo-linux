<!-- coderly-ground-note:v1 -->
---
slug: detach-has-two-display-paths-in-daemon-rs
title: Detach has two display paths in daemon.rs
writtenAt: 1792320520351
tablesTouched: 0
askId: c4a4a250-49e5-4ff2-82b0-64add9a6c8e0
files:
  - path: ui-tauri-react/src-tauri/src/runtime/daemon.rs
    blob: 38a2e1c19b1b27c370eb15ec46fe00896ed50531
  - path: ui-tauri-react/src-tauri/src/runtime/session_agent.rs
    blob: a3cb9462be122a8ed84e58f59318be23163b6b08
---

# Detach has two display paths in daemon.rs

replay_current_display_mode either re-applies an exact saved layout (ApplyDisplayLayout) or sends SetDockMode. Anything that must happen on attach/detach (like rotating to how the Duo is held) has to run on both. The saved-layout path skips session_agent::apply_dock_layout entirely, which is why #41's rotate-on-detach silently did nothing for users with a saved layout.
