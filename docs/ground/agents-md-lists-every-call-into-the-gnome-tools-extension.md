<!-- coderly-ground-note:v1 -->
---
slug: agents-md-lists-every-call-into-the-gnome-tools-extension
title: AGENTS.md lists every call into the GNOME Tools extension
writtenAt: 1792321350758
tablesTouched: 0
askId: b9bea0b3-16ef-4442-90d3-1a88a7756919
files:
  - path: ui-tauri-react/src-tauri/src/runtime/osd.rs
    blob: 5fb3c39b5d4932dabdd9f485e138484322c8f9b7
  - path: AGENTS.md
    blob: d3b26c794fd245a5b3256e642ea32d314eb7c911
---

# AGENTS.md lists every call into the GNOME Tools extension

Every runtime call to the third-party GNOME extension (org.gnome.Shell.Extensions.ZenbookDuoGnomeTools) is listed in AGENTS.md with its caller and fallback. Adding, removing or renaming a call means updating that table, and each call must keep its fallback, since the extension is optional and absent off GNOME. The extension also writes defaultScale/syncBrightness into settings.json and reads /var/lib/zenbook-duo/state.json, so format changes there affect it.
