<!-- coderly-feature:v1 -->
---
slug: app-shell
name: App shell and UI kit
kind: platform
overrideReason: The React shell, store, Tauri bridge, theme and shadcn UI kit are used by every page; the layering decision names src/components/ui and src/shared as shared homes.
bindings:
  main: ui-tauri-react/src/main.tsx
  css: ui-tauri-react/src/index.css
  viteEnv: ui-tauri-react/src/vite-env.d.ts
  assets: ui-tauri-react/src/assets
  ui: ui-tauri-react/src/components/ui
  sidebar: ui-tauri-react/src/components/Sidebar.tsx
  titleBar: ui-tauri-react/src/components/TitleBar.tsx
  themeToggle: ui-tauri-react/src/components/ThemeToggle.tsx
  lib: ui-tauri-react/src/lib
  mobile: ui-tauri-react/src/hooks/use-mobile.ts
  types: ui-tauri-react/src/types
  themeCommand: ui-tauri-react/src-tauri/src/commands/theme.rs
  viteConfig: ui-tauri-react/vite.config.ts
  eslintConfig: ui-tauri-react/eslint.config.js
---

The window every feature appears in: the title bar and sidebar navigation, the global store and its event subscriptions, the Tauri invoke bridge (lib/tauri.ts and the tauri-adapters re-exports every page calls), shared types and defaults, version display, the light/dark/system theme (applied at startup and following the desktop colour scheme through the portal or gsettings), and the shadcn UI kit. App.tsx (the page router) and pages/Controls.tsx (which hosts the backlight, orientation and touchscreen controls and a service restart card) are declared registries that wire features together, so no feature or platform owns them.
