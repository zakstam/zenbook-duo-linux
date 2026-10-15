<!-- coderly-ground-note:v1 -->
---
slug: control-panel-store-read-slices-with-usestoreselector
title: "Control Panel store: read slices with useStoreSelector"
writtenAt: 1792050850182
tablesTouched: 0
askId: 3d42c96a-dabd-49f0-a286-8d9cf85a78bd
files:
  - path: ui-tauri-react/src/lib/store.tsx
    blob: 2d9c38a0a90d1d37b550547a318e95eb4fe20c0d
---

# Control Panel store: read slices with useStoreSelector

The store is an external store (useSyncExternalStore), not a context value. Read it with useStoreSelector((s) => s.slice). The selector must return an existing slice or a primitive, never a freshly built object, or the component re-renders on every dispatch (or loops). useStore was removed on purpose. SET_STATUS and SET_VERSION_INFO keep the old object when the payload is shallow-equal, because the backend sends fresh objects on every refresh.
