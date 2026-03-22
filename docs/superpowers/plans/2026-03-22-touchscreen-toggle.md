# Touchscreen Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-display touchscreen enable/disable to the Zenbook Duo Control Panel, persisted across reboots.

**Architecture:** sysfs unbind/bind via daemon IPC (daemon runs as root). Settings stored in existing DuoSettings model. UI toggles on Controls and Display pages.

**Tech Stack:** Rust (Tauri backend, daemon), React + TypeScript (frontend), serde JSON IPC

**Spec:** `docs/superpowers/specs/2026-03-22-touchscreen-toggle-design.md`

---

### Task 1: Hardware module — touchscreen detection and control

**Files:**
- Create: `ui-tauri-react/src-tauri/src/hardware/touchscreen.rs`
- Modify: `ui-tauri-react/src-tauri/src/hardware/mod.rs`

- [ ] **Step 1: Create `hardware/touchscreen.rs` with `TouchscreenDevice` struct**

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchscreenDevice {
    pub name: String,
    pub i2c_id: String,
    pub connector: String,
    pub enabled: bool,
}
```

- [ ] **Step 2: Implement `list_touchscreens()`**

Scans `/sys/bus/i2c/drivers/i2c_hid_acpi/` for ELAN touchscreen devices. Checks if bound (enabled) by testing if the device symlink exists under the driver directory.

```rust
/// Maps ELAN model number to display connector.
fn elan_to_connector(name: &str) -> Option<&'static str> {
    if name.contains("ELAN9008") {
        Some("eDP-1")
    } else if name.contains("ELAN9009") {
        Some("eDP-2")
    } else {
        None
    }
}

/// Reads the device name from sysfs for an i2c device.
fn read_i2c_device_name(i2c_id: &str) -> Option<String> {
    let path = format!("/sys/bus/i2c/devices/{}/name", i2c_id);
    fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

/// Checks if the i2c device is currently bound to its driver.
fn is_bound(i2c_id: &str) -> bool {
    Path::new(&format!(
        "/sys/bus/i2c/drivers/i2c_hid_acpi/{}",
        i2c_id
    ))
    .exists()
}

pub fn list_touchscreens() -> Vec<TouchscreenDevice> {
    let mut devices = Vec::new();
    let i2c_devices = match fs::read_dir("/sys/bus/i2c/devices") {
        Ok(entries) => entries,
        Err(_) => return devices,
    };
    for entry in i2c_devices.flatten() {
        let i2c_id = entry.file_name().to_string_lossy().to_string();
        if !i2c_id.starts_with("i2c-ELAN") {
            continue;
        }
        let name = match read_i2c_device_name(&i2c_id) {
            Some(n) => n,
            None => continue,
        };
        let connector = match elan_to_connector(&name) {
            Some(c) => c.to_string(),
            None => continue,
        };
        devices.push(TouchscreenDevice {
            name,
            i2c_id: i2c_id.clone(),
            connector,
            enabled: is_bound(&i2c_id),
        });
    }
    devices
}
```

- [ ] **Step 3: Implement `set_touchscreen_enabled()`**

```rust
pub fn set_touchscreen_enabled(i2c_id: &str, enabled: bool) -> Result<(), String> {
    let path = if enabled {
        "/sys/bus/i2c/drivers/i2c_hid_acpi/bind"
    } else {
        "/sys/bus/i2c/drivers/i2c_hid_acpi/unbind"
    };
    fs::write(path, i2c_id)
        .map_err(|e| format!("Failed to {} touchscreen {}: {}",
            if enabled { "bind" } else { "unbind" }, i2c_id, e))
}
```

- [ ] **Step 4: Register module in `hardware/mod.rs`**

Add `pub mod touchscreen;` after existing declarations.

- [ ] **Step 5: Verify it compiles**

Run: `cd ui-tauri-react && cargo check --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 6: Commit**

```bash
git add ui-tauri-react/src-tauri/src/hardware/touchscreen.rs ui-tauri-react/src-tauri/src/hardware/mod.rs
git commit -m "feat: add touchscreen hardware detection and sysfs control"
```

---

### Task 2: IPC protocol — add touchscreen request/response variants

**Files:**
- Modify: `ui-tauri-react/src-tauri/src/ipc/protocol.rs`

- [ ] **Step 1: Add `ListTouchscreens` and `SetTouchscreenEnabled` to `DaemonRequest`**

Add to the `DaemonRequest` enum:

```rust
ListTouchscreens,
SetTouchscreenEnabled { connector: String, enabled: bool },
```

- [ ] **Step 2: Add `Touchscreens` to `DaemonResponse`**

Add to the `DaemonResponse` enum:

```rust
Touchscreens { devices: Vec<crate::hardware::touchscreen::TouchscreenDevice> },
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ui-tauri-react && cargo check --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 4: Commit**

```bash
git add ui-tauri-react/src-tauri/src/ipc/protocol.rs
git commit -m "feat: add touchscreen IPC protocol variants"
```

---

### Task 3: Settings persistence — add `touchscreen_disabled` field

**Files:**
- Modify: `ui-tauri-react/src-tauri/src/models/settings.rs`

- [ ] **Step 1: Add field to `DuoSettings`**

Add to the `DuoSettings` struct:

```rust
#[serde(default)]
pub touchscreen_disabled: Vec<String>,
```

- [ ] **Step 2: Update `Default` implementation**

In the `Default` impl for `DuoSettings`, add:

```rust
touchscreen_disabled: Vec::new(),
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ui-tauri-react && cargo check --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 4: Commit**

```bash
git add ui-tauri-react/src-tauri/src/models/settings.rs
git commit -m "feat: add touchscreen_disabled to settings model"
```

---

### Task 4: Daemon handler — process touchscreen requests

**Files:**
- Modify: `ui-tauri-react/src-tauri/src/runtime/daemon.rs`

- [ ] **Step 1: Add match arms for touchscreen requests in `handle_client`**

In the `match envelope.payload { ... }` block, add:

```rust
DaemonRequest::ListTouchscreens => {
    let devices = hardware::touchscreen::list_touchscreens();
    DaemonResponse::Touchscreens { devices }
}
DaemonRequest::SetTouchscreenEnabled { connector, enabled } => {
    let devices = hardware::touchscreen::list_touchscreens();
    match devices.iter().find(|d| d.connector == connector) {
        Some(dev) => {
            match hardware::touchscreen::set_touchscreen_enabled(&dev.i2c_id, enabled) {
                Ok(()) => DaemonResponse::Ack,
                Err(message) => DaemonResponse::Error { message },
            }
        }
        None => DaemonResponse::Error {
            message: format!("No touchscreen found for connector {}", connector),
        },
    }
}
```

- [ ] **Step 2: Add touchscreen restore to `handle_lifecycle`**

In the `Post | Thaw | Boot` arm of `handle_lifecycle`, insert **between** the write-guard block (where `guard.touch()` / `persist_state()` are called) and the `forward_session_command` call. Use a new read-lock scope:

```rust
// Restore touchscreen disabled state
{
    let guard = state.read().await;
    let disabled = guard.settings.touchscreen_disabled.clone();
    drop(guard);
    for connector in &disabled {
        let devices = hardware::touchscreen::list_touchscreens();
        if let Some(dev) = devices.iter().find(|d| &d.connector == connector) {
            if let Err(e) = hardware::touchscreen::set_touchscreen_enabled(&dev.i2c_id, false) {
                eprintln!("rust-daemon: failed to restore touchscreen disabled for {}: {}", connector, e);
            }
        }
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ui-tauri-react && cargo check --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 4: Commit**

```bash
git add ui-tauri-react/src-tauri/src/runtime/daemon.rs
git commit -m "feat: handle touchscreen IPC requests and boot restore in daemon"
```

---

### Task 5: Tauri commands — expose touchscreen control to frontend

**Files:**
- Create: `ui-tauri-react/src-tauri/src/commands/touchscreen.rs`
- Modify: `ui-tauri-react/src-tauri/src/commands/mod.rs`
- Modify: `ui-tauri-react/src-tauri/src/lib.rs`

- [ ] **Step 1: Create `commands/touchscreen.rs`**

Follows the daemon-first-with-fallback pattern from `commands/display.rs`:

```rust
use crate::hardware::touchscreen::{self, TouchscreenDevice};
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;

#[tauri::command]
pub fn list_touchscreens() -> Result<Vec<TouchscreenDevice>, String> {
    match client::request(DaemonRequest::ListTouchscreens) {
        Ok(DaemonResponse::Touchscreens { devices }) => Ok(devices),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Ok(touchscreen::list_touchscreens()),
        Err(_) => Ok(touchscreen::list_touchscreens()),
    }
}

#[tauri::command]
pub fn set_touchscreen_enabled(connector: String, enabled: bool) -> Result<(), String> {
    let fallback = || {
        let devices = touchscreen::list_touchscreens();
        match devices.iter().find(|d| d.connector == connector) {
            Some(dev) => touchscreen::set_touchscreen_enabled(&dev.i2c_id, enabled),
            None => Err(format!("No touchscreen found for {}", connector)),
        }
    };
    match client::request(DaemonRequest::SetTouchscreenEnabled {
        connector: connector.clone(),
        enabled,
    }) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => fallback(),
        Err(_) => fallback(),
    }
}
```

- [ ] **Step 2: Register module in `commands/mod.rs`**

Add `pub mod touchscreen;` after existing declarations.

- [ ] **Step 3: Register commands in `lib.rs` invoke_handler**

Add to the `generate_handler!` macro:

```rust
commands::touchscreen::list_touchscreens,
commands::touchscreen::set_touchscreen_enabled,
```

- [ ] **Step 4: Verify it compiles**

Run: `cd ui-tauri-react && cargo check --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 5: Commit**

```bash
git add ui-tauri-react/src-tauri/src/commands/touchscreen.rs ui-tauri-react/src-tauri/src/commands/mod.rs ui-tauri-react/src-tauri/src/lib.rs
git commit -m "feat: add Tauri commands for touchscreen control"
```

---

### Task 6: Frontend types and API

**Files:**
- Modify: `ui-tauri-react/src/types/duo.ts`
- Modify: `ui-tauri-react/src/lib/tauri.ts`

- [ ] **Step 1: Add `TouchscreenDevice` interface to `duo.ts`**

```typescript
export interface TouchscreenDevice {
  name: string;
  i2cId: string;
  connector: string;
  enabled: boolean;
}
```

- [ ] **Step 2: Add `touchscreenDisabled` to `DuoSettings` interface**

```typescript
touchscreenDisabled: string[];
```

- [ ] **Step 3: Add invoke functions to `tauri.ts`**

```typescript
export const listTouchscreens = () =>
  invoke<TouchscreenDevice[]>("list_touchscreens");
export const setTouchscreenEnabled = (connector: string, enabled: boolean) =>
  invoke<void>("set_touchscreen_enabled", { connector, enabled });
```

- [ ] **Step 4: Verify frontend compiles**

Run: `cd ui-tauri-react && npm run check` (or `npx tsc --noEmit`)

- [ ] **Step 5: Commit**

```bash
git add ui-tauri-react/src/types/duo.ts ui-tauri-react/src/lib/tauri.ts
git commit -m "feat: add touchscreen types and API functions"
```

---

### Task 7: Controls page — touchscreen toggle section

**Files:**
- Modify: `ui-tauri-react/src/pages/Controls.tsx`

- [ ] **Step 1: Add imports and state**

Add `useEffect` to the React import (line 1 currently only imports `useState`). Add to existing imports:

```typescript
import { useState, useEffect } from "react";
import { listTouchscreens, setTouchscreenEnabled } from "@/lib/tauri";
import { TouchscreenDevice } from "@/types/duo";
import { Switch } from "@/components/ui/switch";
import { IconHandFinger } from "@tabler/icons-react";
```

Add state inside the component:

```typescript
const [touchscreens, setTouchscreens] = useState<TouchscreenDevice[]>([]);

useEffect(() => {
  listTouchscreens().then(setTouchscreens).catch(console.error);
}, []);

const handleTouchToggle = async (connector: string, enabled: boolean) => {
  try {
    await setTouchscreenEnabled(connector, enabled);
    setTouchscreens((prev) =>
      prev.map((ts) => (ts.connector === connector ? { ...ts, enabled } : ts))
    );
  } catch (e) {
    console.error("Failed to toggle touchscreen:", e);
  }
};
```

- [ ] **Step 2: Add touchscreen card section**

Add after the existing card sections (Service Control card). Follow the same glass-card pattern:

```tsx
{touchscreens.length > 0 && (
  <div className="glass-card animate-stagger-in stagger-4 rounded-xl p-5">
    <div className="mb-5 flex items-center gap-2.5">
      <div className="flex size-7 items-center justify-center rounded-lg bg-purple-500/12 text-purple-500 dark:bg-purple-400/10 dark:text-purple-400">
        <IconHandFinger className="size-3.5" stroke={1.75} />
      </div>
      <div>
        <h3 className="text-[13px] font-semibold text-foreground">
          Touchscreen
        </h3>
        <p className="text-[11px] text-muted-foreground">
          Enable or disable touch input per display
        </p>
      </div>
    </div>
    <div className="space-y-3">
      {touchscreens.map((ts) => (
        <div key={ts.connector} className="flex items-center justify-between">
          <span className="text-[13px]">
            {ts.connector}
            <span className="text-muted-foreground ml-2 text-[11px]">
              {ts.name}
            </span>
          </span>
          <Switch
            checked={ts.enabled}
            onCheckedChange={(checked) =>
              handleTouchToggle(ts.connector, checked)
            }
          />
        </div>
      ))}
    </div>
  </div>
)}
```

- [ ] **Step 3: Verify frontend compiles and renders**

Run: `cd ui-tauri-react && npm run check`

- [ ] **Step 4: Commit**

```bash
git add ui-tauri-react/src/pages/Controls.tsx
git commit -m "feat: add touchscreen toggle section to Controls page"
```

---

### Task 8: Display page — per-display touch toggle

**Files:**
- Modify: `ui-tauri-react/src/pages/DisplayLayout.tsx`

- [ ] **Step 1: Add imports and state**

Add to existing imports:

```typescript
import { listTouchscreens, setTouchscreenEnabled } from "@/lib/tauri";
import { TouchscreenDevice } from "@/types/duo";
import { Switch } from "@/components/ui/switch";
```

Add state inside the component:

```typescript
const [touchscreens, setTouchscreens] = useState<TouchscreenDevice[]>([]);

useEffect(() => {
  listTouchscreens().then(setTouchscreens).catch(console.error);
}, []);

const handleTouchToggle = async (connector: string, enabled: boolean) => {
  try {
    await setTouchscreenEnabled(connector, enabled);
    setTouchscreens((prev) =>
      prev.map((ts) => (ts.connector === connector ? { ...ts, enabled } : ts))
    );
  } catch (e) {
    console.error("Failed to toggle touchscreen:", e);
  }
};
```

- [ ] **Step 2: Add touch toggle to per-display details**

In the "Connected Displays" section where each display's stats are shown, add a touch toggle for displays that have a mapped touchscreen. After the stats line (`{d.width}x{d.height} @ ...`), add:

```tsx
{(() => {
  const ts = touchscreens.find((t) => t.connector === d.connector);
  if (!ts) return null;
  return (
    <div className="flex items-center gap-2 mt-2">
      <span className="text-xs text-muted-foreground">Touch</span>
      <Switch
        checked={ts.enabled}
        onCheckedChange={(checked) =>
          handleTouchToggle(d.connector, checked)
        }
      />
    </div>
  );
})()}
```

- [ ] **Step 3: Verify frontend compiles**

Run: `cd ui-tauri-react && npm run check`

- [ ] **Step 4: Commit**

```bash
git add ui-tauri-react/src/pages/DisplayLayout.tsx
git commit -m "feat: add per-display touch toggle to Display page"
```

---

### Task 9: Settings sync — persist touchscreen state on toggle

**Files:**
- Modify: `ui-tauri-react/src/pages/Controls.tsx`
- Modify: `ui-tauri-react/src/pages/DisplayLayout.tsx`

- [ ] **Step 1: Update `handleTouchToggle` in Controls.tsx to persist settings**

After the `setTouchscreenEnabled` call succeeds, also update and save settings:

```typescript
const handleTouchToggle = async (connector: string, enabled: boolean) => {
  try {
    await setTouchscreenEnabled(connector, enabled);
    setTouchscreens((prev) =>
      prev.map((ts) => (ts.connector === connector ? { ...ts, enabled } : ts))
    );
    // Persist to settings
    const settings = await loadSettings();
    const disabled = settings.touchscreenDisabled ?? [];
    settings.touchscreenDisabled = enabled
      ? disabled.filter((c) => c !== connector)
      : [...disabled.filter((c) => c !== connector), connector];
    await saveSettings(settings);
  } catch (e) {
    console.error("Failed to toggle touchscreen:", e);
  }
};
```

Make sure `loadSettings` and `saveSettings` are imported from `@/lib/tauri`.

- [ ] **Step 2: Apply the same update to `handleTouchToggle` in DisplayLayout.tsx**

Same logic as step 1.

- [ ] **Step 3: Verify frontend compiles**

Run: `cd ui-tauri-react && npm run check`

- [ ] **Step 4: Commit**

```bash
git add ui-tauri-react/src/pages/Controls.tsx ui-tauri-react/src/pages/DisplayLayout.tsx
git commit -m "feat: persist touchscreen toggle state to settings"
```

---

### Task 10: Manual testing

- [ ] **Step 1: Build the project**

Run: `cd ui-tauri-react && cargo build --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 2: Verify touchscreen detection**

With the daemon running, open the Control Panel. The Controls page should show a "Touchscreen" section with toggles for eDP-1 and eDP-2.

- [ ] **Step 3: Test toggle**

Toggle one touchscreen off. Verify:
- Touch input stops on that display
- The toggle shows the correct state after refresh
- The setting persists (check settings file)

- [ ] **Step 4: Test boot restore**

Restart the daemon. Verify the previously disabled touchscreen remains disabled.

- [ ] **Step 5: Test re-enable**

Toggle the touchscreen back on. Verify touch input works again.
