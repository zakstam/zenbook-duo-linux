<!-- coderly-feature:v1 -->
---
slug: event-monitor-and-logs
name: Event monitor and logs
bindings:
  events: ui-tauri-react/src-tauri/src/commands/events.rs
  logs: ui-tauri-react/src-tauri/src/commands/logs.rs
  model: ui-tauri-react/src-tauri/src/models/event.rs
  watchers: ui-tauri-react/src-tauri/src/watchers
  monitorPage: ui-tauri-react/src/pages/EventMonitor.tsx
  logsPage: ui-tauri-react/src/pages/Logs.tsx
  stream: ui-tauri-react/src/components/EventStream.tsx
---

Lets the user see what the hardware and service did: the Event Monitor page streams categorized hardware events (attach, display, backlight, service) with severity filters, fed by the daemon's recent events and a file watcher in the app, and the Logs page shows, filters, copies and clears the runtime log.
