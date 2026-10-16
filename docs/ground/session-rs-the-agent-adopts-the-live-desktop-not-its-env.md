<!-- coderly-ground-note:v1 -->
---
slug: session-rs-the-agent-adopts-the-live-desktop-not-its-env
title: "session.rs: the agent adopts the live desktop, not its env"
writtenAt: 1792141702125
tablesTouched: 0
askId: cf1eef69-b08e-4b0c-a8a6-a0244af8f4ee
files:
  - path: ui-tauri-react/src-tauri/src/runtime/session.rs
    blob: 107b4654943ad541334c9ebf271bedcba315057d
  - path: ui-tauri-react/src-tauri/src/runtime/session_agent.rs
    blob: 4c0e54b19f45bcce70efe8630d0c5f7cb89caaae
  - path: ui-tauri-react/src-tauri/src/bin/zenbook-duo-session-agent.rs
    blob: b39acbb455a016b7985adf157faff4972611dc19
---

# session.rs: the agent adopts the live desktop, not its env

The user has Linger=yes, so the session agent outlives logout. Desktop identity comes from discover_live_session (live Hyprland or niri sockets in $XDG_RUNTIME_DIR, then GNOME/KDE from `systemctl --user show-environment`). It is written into the process env in main before the tokio runtime starts; don't call set_var later from async code. The agent exits 75 when that desktop stops answering and systemd restarts it. daemon.log is root-owned, so agent lines only reach the user journal. Running the debug agent binary on zak-cachyos (the Duo) hijacks the installed agent's socket and replays the layout on the real screens; restart zenbook-duo-session-agent afterwards.
