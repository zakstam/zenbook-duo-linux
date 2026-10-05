<!-- coderly-ground-note:v1 -->
---
slug: usb-media-remap-rs-get-status-writes-the-helper-s-pid-file
title: usb_media_remap.rs get_status writes the helper's pid file
writtenAt: 1791236023907
tablesTouched: 0
askId: a25d1807-7680-4c95-819e-511c4dd23b6e
files:
  - path: ui-tauri-react/src-tauri/src/commands/usb_media_remap.rs
    blob: 8c0e21562137f17114467a95b9b9e77dcda36d3e
  - path: ui-tauri-react/src-tauri/src/usb_media_remap_helper.rs
    blob: cc7ad9890d5f1ae51746852e17490fa6c65218f6
---

# usb_media_remap.rs get_status writes the helper's pid file

get_status() is not read-only. When the pid file is missing, recover_running_helper_pid scans /proc for a zenbook-duo-usb-remap-helper with the same --pid-file and writes its pid into the file, and start_remap calls it right after spawning. The helper must therefore accept a pid file that holds its own pid (write_pid does), and it claims the file before opening or grabbing the keyboard. Breaking either of these brings back the every-second restart loop of #28/#19 (before 4337c01). Also, a successful start no longer clears the retry cooldown in runtime/monitor.rs; the next tick that sees the helper running clears it.
