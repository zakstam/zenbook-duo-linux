<!-- coderly-decision:v1 -->
---
slug: move-toward-feature-folders-migrating-gradually-from-today-s
title: Move toward feature folders, migrating gradually from today's layered layout
decidedAt: 1791137527962
domain: layering
provenance: ballot
areas:
  - ui-tauri-react/**
governsConfig:
  - layers
  - structure.slices
  - structure.sharedHomes
  - structure.registries
  - structure.legacyZones
  - structure.bannedFolders
tierMeasured:
  - Three layers (backend Rust, frontend React, install scripts) as the coverage denominator
  - "Where a new feature is born: src-tauri/src/features/* and src/features/*"
  - "Shared homes: src/components/ui, src/shared, src-tauri/src/platform"
  - Registries App.tsx and lib.rs; other cross-feature imports count as removal cost
  - Banned folder names utils/helpers/misc inside a feature home
  - Legacy zones aggregate findings for the old layered folders
tierRecorded:
  - "Migration is gradual: a card that touches a feature moves that feature's files into its home"
  - Slice public surface (what one feature may import from another) is not declared yet, so cross-feature reach is not checked
  - Frontend talks to backend only through Tauri invoke/events, never imports
---

# Move toward feature folders, migrating gradually from today's layered layout

The user chose "Move toward feature folders" (2026-10-04). Today the Tauri app is layered: Rust by kind (commands, hardware, runtime, models, ipc, watchers) and React by kind (pages, components, lib, hooks). The target is one home per feature in each layer: ui-tauri-react/src-tauri/src/features/<slug>/ for Rust and ui-tauri-react/src/features/<slug>/ for React. Code shared by many features lives in src-tauri/src/platform/ (daemon, session agent, IPC, logging) and src/shared/ plus src/components/ui/ (shadcn). App.tsx and lib.rs are the registries that wire every feature. Existing folders are legacy zones: code moves into its feature home when a card touches that feature, not in a big-bang migration. The install shell scripts are their own layer and stay at the repo root.

## Rejected

- Keep the layered shape as the declared structure: rejected, the user wants features to own their code end to end.
- Big-bang migration into feature folders: not chosen; the move is gradual, per card, to avoid churn across a working app.
