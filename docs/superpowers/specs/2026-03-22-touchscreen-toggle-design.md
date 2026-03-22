# Touchscreen Toggle — Design Spec

Per-display touchscreen enable/disable from the Control Panel UI, with state persistence across reboots.

## Context

The ASUS Zenbook Duo has two ELAN touchscreens (ELAN9008 on eDP-1, ELAN9009 on eDP-2). When the keyboard is detached and both screens are active, users may want to disable touch on one screen (e.g., to avoid accidental input while using a stylus on the other). No touchscreen control exists in the project today.

## Mechanism

**sysfs unbind/bind** — compositor-agnostic, immediate, no reboot required.

- Disable: `echo "<i2c_id>" > /sys/bus/i2c/drivers/i2c_hid_acpi/unbind`
- Enable: `echo "<i2c_id>" > /sys/bus/i2c/drivers/i2c_hid_acpi/bind`
- Requires root — routed through the daemon (which runs as root).

## Backend

### New file: `src-tauri/src/hardware/touchscreen.rs`

**`list_touchscreens() -> Vec<TouchscreenDevice>`**

Scans `/sys/class/input/event*` for devices with touch capabilities. Groups by parent i2c device ID to deduplicate (each ELAN device registers multiple event nodes — touch, stylus, touchpad, etc.). Returns one entry per physical touchscreen:

```rust
struct TouchscreenDevice {
    name: String,          // e.g., "ELAN9008:00 04F3:425B"
    i2c_id: String,        // e.g., "i2c-ELAN9008:00"
    connector: String,     // e.g., "eDP-1"
    enabled: bool,
}
```

**Display mapping:** Hardcoded by device name — "ELAN9008" maps to eDP-1 (top/main), "ELAN9009" maps to eDP-2 (bottom). This matches the ACPI topology and is consistent with how the rest of the codebase handles this hardware.

**`set_touchscreen_enabled(i2c_id: &str, enabled: bool) -> Result<()>`**

Writes the i2c_id to the appropriate sysfs bind/unbind file. Called by the daemon (which has root privileges).

### Daemon IPC (`ipc/protocol.rs`)

Add to `DaemonRequest`:

```rust
ListTouchscreens,
SetTouchscreenEnabled { connector: String, enabled: bool },
```

Add to `DaemonResponse`:

```rust
Touchscreens { devices: Vec<TouchscreenDevice> },
```

The daemon handles these requests using the `hardware/touchscreen.rs` functions directly (it runs as root).

### New file: `src-tauri/src/commands/touchscreen.rs`

Two Tauri commands that route through daemon IPC (matching the pattern used by `get_display_layout` and `apply_display_layout`):

- `list_touchscreens` — sends `ListTouchscreens` to daemon, returns devices
- `set_touchscreen_enabled(connector, enabled)` — sends `SetTouchscreenEnabled` to daemon, also updates settings

Register both in the `invoke_handler` macro in `lib.rs`.

## Settings Persistence

Add to existing settings model (`models/settings.rs`):

```rust
#[serde(default)]
touchscreen_disabled: Vec<String>  // connectors with touch disabled, e.g., ["eDP-2"]
```

Uses `#[serde(default)]` to avoid breaking deserialization of existing settings files.

Also update the TypeScript `DuoSettings` interface in `types/duo.ts`:

```typescript
touchscreenDisabled: string[];
```

- Toggle off: add connector to list, save settings
- Toggle on: remove connector from list, save settings

## Boot Restore

In `daemon.rs`, the `handle_lifecycle` function already restores state on `Post`/`Thaw`/`Boot` phases (backlight, dock mode). Add touchscreen restore there:

- Read `touchscreen_disabled` from `state.settings`
- Unbind each listed touchscreen

This is the correct location because the daemon runs as root (can write to sysfs) and already handles boot/resume state restoration.

## Frontend

### Types (`types/duo.ts`)

```typescript
interface TouchscreenDevice {
  connector: string;
  name: string;
  enabled: boolean;
}
```

### API (`lib/tauri.ts`)

```typescript
export const listTouchscreens = () =>
  invoke<TouchscreenDevice[]>("list_touchscreens");
export const setTouchscreenEnabled = (connector: string, enabled: boolean) =>
  invoke<void>("set_touchscreen_enabled", { connector, enabled });
```

### Controls page (`Controls.tsx`)

New "Touchscreen" section with a toggle per display. Fetches state via `listTouchscreens()` on mount, calls `setTouchscreenEnabled()` on toggle. Follows existing Switch component pattern.

### Display page (`DisplayLayout.tsx`)

Per-display touch toggle in the display details list, next to the scale selector. Only shown for displays that have a mapped touchscreen. Same data source and commands as Controls page.

## Setup Scripts

The setup scripts (`setup-gnome.sh`, `setup-kde.sh`, `setup-niri.sh`) do **not** need changes — the daemon already runs as root and handles the sysfs writes directly. No new sudoers entries or polkit policies are required.

## Scope Exclusions

- No auto-toggle based on keyboard attach/detach (screen is off when keyboard attached)
- No profile integration
- No compositor-specific implementations (sysfs is universal)
