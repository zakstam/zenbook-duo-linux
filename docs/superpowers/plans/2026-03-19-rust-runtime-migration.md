# Rust Runtime Migration Plan

## Summary

Migrate the non-installer runtime from the current `duo.sh` + Python helper model to a Rust-first architecture with:
- a privileged system daemon as the single source of truth for hardware state and policy
- a small user-session agent for compositor/session-bound actions
- the existing Tauri app refactored into a client of the daemon over a Unix socket API

This is a one-shot cutover, not a staged parity rollout. Installer/setup scripts remain Bash, but they will install and wire up the new Rust binaries/services instead of `duo.sh` and Python helpers.

## Architecture and Key Changes

### 1. Replace `duo.sh` and Python helpers with Rust services

- Rebuild all runtime behavior currently owned by `duo.sh`: dock attach/detach handling, backlight persistence/control, brightness sync, Wi-Fi/Bluetooth state tracking, lock/resume handling, rotation handling, and USB remap lifecycle.
- Fold the three Python helpers into Rust:
  - USB keyboard backlight control into the existing HID/rusb path
  - Bluetooth hidraw backlight control into the existing ioctl HID path
  - brightness key injection into a Rust uinput helper path
- Replace shell-managed state in `/tmp/duo*` with daemon-owned structured runtime state under a new Rust-controlled path; the shell-era temp-file contract is not preserved.

### 2. Introduce two Rust runtime processes

- **System daemon**: owns hardware watchers, state machine, policy, persistence, logging, and privileged operations.
- **User-session agent**: a minimal unprivileged process started per login session that performs GNOME/KDE/Niri display changes and other session-scoped actions on behalf of the daemon.
- The daemon and agent communicate over a local Unix socket contract; the daemon is authoritative and the agent is an executor for session-scoped operations only.

### 3. Refactor the Tauri app into a daemon client

- Move the UI off direct file reads, direct shell control, and direct runtime ownership.
- Replace `status`, control, logs, remap, and event flows with daemon-backed requests and event subscriptions over the Unix socket API.
- Keep the Tauri binary as the UI plus existing privileged helper modes only where still useful during migration; post-cutover, the daemon becomes the runtime authority.
- Profiles/settings remain user-facing concepts in the UI, but application of those settings goes through daemon APIs.

### 4. Preserve backend coverage in the Rust world

- Initial Rust cutover must fully support GNOME, KDE, and Niri.
- Backend-specific display logic currently split across shell helpers becomes Rust-managed backend adapters, with the session agent responsible for invoking the compositor-specific commands/tools inside the user session.
- The system daemon must not directly depend on shell-era display helper scripts after the cutover.

### 5. Installer/setup changes only at the integration boundary

- Keep installer/setup scripts in Bash, but switch them to installing/enabling:
  - the Rust system daemon service
  - the Rust user-session agent service
  - the updated UI/client binary
- Remove installation of Python helpers and `duo.sh` from the target state.
- Preserve simple one-line install/update UX even though the runtime beneath it changes completely.

## Public Interfaces and Contracts

### Unix socket daemon API

Define a structured local API for:
- status snapshot
- control actions: backlight, orientation, display layout, profile activation, service reload/restart-style actions
- settings read/write needed by the UI
- remap control and status
- log/event streaming or polling

The API should be versioned from day one so the UI and daemon can detect mismatches cleanly.

### Session-agent contract

Define a narrow daemon-to-agent contract for:
- apply display layout
- set orientation / rotate modes
- launch session UI actions such as emoji picker
- report execution success/failure and session capability/backends

### Service model

Adopt:
- one systemd **system service** for the daemon
- one systemd **user service** for the session agent
- the Tauri app as an optional client, not a required runtime component

## Test Plan

### Functional parity scenarios

- Boot with keyboard attached and detached
- USB attach/detach transitions
- Bluetooth fallback behavior after detach
- Backlight set/cycle/persist across attach-detach and restart
- Brightness up/down behavior with compositor-native handling
- Wi-Fi/Bluetooth restore behavior across dock transitions
- Lock/unlock and suspend/resume transitions
- Rotation/orientation flows
- USB remap start/stop/pause behavior
- GNOME, KDE, and Niri display reconfiguration paths

### Integration scenarios

- UI connects to daemon, reads status, sends controls, and receives events
- Session agent registration and daemon-to-agent action execution
- Agent absent/unavailable behavior is surfaced clearly without daemon crash
- Service startup ordering and recovery when daemon or agent restarts independently

### Failure-mode coverage

- device disappearance during hotplug
- stale session agent socket
- daemon running without a logged-in agent
- permission/device-open failures
- backend command failures on GNOME/KDE/Niri
- UI/daemon API version mismatch

## Assumptions and Defaults

- Installer/setup scripts stay Bash and are explicitly out of scope for Rust migration except for changing what they install and enable.
- Migration includes replacing `duo.sh`, the three Python helpers, and shell-era runtime file/state ownership.
- This is a one-shot cutover, so the implementation must reach full GNOME/KDE/Niri runtime parity before switching installers/services to the Rust daemon by default.
- The new architecture is intentionally a clean break: no requirement to preserve `/usr/local/bin/duo`, `/tmp/duo*`, or the old CLI/state-file contract.
- The recommended execution order is: implement daemon core first, then session agent, then UI refactor to daemon API, then installer/service cutover, then parity validation across all three backends.
