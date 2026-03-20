# Hyprland Support Design

**Date:** 2026-03-20

**Goal:** Add full Hyprland support to zenbook-duo-linux with installer detection, session dock-mode behavior, display layout read/write support, orientation control, and Control Panel parity.

## Summary

This project already treats GNOME, KDE Plasma, and Niri as compositor-specific backends layered under shared runtime and UI models. Hyprland should be added as another first-class backend that plugs into the same seams rather than as a one-off compatibility path.

The preferred design uses Hyprland's runtime CLI for active display control and an optional installer-managed config snippet only for session-start glue. Runtime state remains owned by the Rust daemon and session agent. The snippet is intentionally small and must not become the primary place where display behavior is encoded.

## Scope

In scope:

- Auto-detect Hyprland in the unified installer.
- Add a `setup-hyprland.sh` setup path parallel to GNOME, KDE, and Niri.
- Add `Hyprland` as a session/display backend in the Rust runtime.
- Support docked and undocked display mode transitions in Hyprland sessions.
- Support reading the current display layout from Hyprland.
- Support applying display layouts from the Control Panel in Hyprland sessions.
- Support orientation changes in Hyprland sessions.
- Update user-facing documentation and requirements to list Hyprland.
- Add parser and backend-detection tests for the new backend.

Out of scope:

- Re-architecting all compositor backends behind a new generic trait layer.
- Persistent multi-profile layout restore beyond the project's existing runtime behavior.
- Managing general-purpose user Hyprland configuration outside a small Zenbook-specific optional snippet.
- Adding support for non-Hyprland wlroots compositors as part of this effort.

## User Experience

From the user's perspective, Hyprland support should behave like the existing supported sessions:

- `install.sh` detects Hyprland and routes to a Hyprland-specific setup script.
- Manual install remains possible with `./setup-hyprland.sh`.
- The same keyboard attach/detach behavior works in Hyprland sessions.
- The Control Panel can read the current layout, apply layout changes, and change orientation without showing Hyprland-specific UI.
- Setup may offer an optional Hyprland snippet for more reliable session startup glue, but runtime control should work through `hyprctl` rather than through static config regeneration.

## Architecture

Hyprland support should be implemented as a new backend in the existing compositor switch points:

1. Installer detection and setup dispatch
2. Session backend registration and dock-mode handling
3. Display layout querying and application
4. Orientation control
5. Documentation and setup guidance

The shared models such as `DisplayLayout`, `DisplayInfo`, `Orientation`, and `SessionBackend` stay compositor-agnostic. Hyprland-specific behavior lives in backend-specific branches inside the existing runtime modules unless a helper extraction becomes necessary to keep functions readable.

## Components

### 1. Installer and setup script

Files involved:

- `install.sh`
- `setup-hyprland.sh` (new)
- `README.md`

Responsibilities:

- Detect Hyprland from `XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP`, or `DESKTOP_SESSION`.
- Route unified installs to `setup-hyprland.sh`.
- Install Hyprland-specific package dependencies using the same package-manager strategy as the existing scripts.
- Reuse the same shared setup behavior already present in the other scripts:
  - prompt for default backlight and scale
  - write UI defaults
  - install udev rules
  - configure sudoers entries for brightness writes
  - add the user to the `input` group
  - deploy runtime binaries and units
- Optionally install a minimal Hyprland snippet under the user's config directory and add an include line if missing.

The setup script should mirror the current setup-script style rather than introducing a new installer framework as part of this task.

### 2. Session backend model

Files involved:

- `ui-tauri-react/src-tauri/src/ipc/protocol.rs`
- `ui-tauri-react/src-tauri/src/runtime/state.rs`
- `ui-tauri-react/src-tauri/src/runtime/session_agent.rs`

Responsibilities:

- Add `SessionBackend::Hyprland`.
- Detect Hyprland sessions from the same environment-variable sources used by the current backends.
- Register the session agent with the daemon using the new backend enum value.
- Route dock-mode requests to a Hyprland-specific implementation.

This change should remain additive and should not change the wire format for existing backends beyond introducing the new serialized enum value.

### 3. Hyprland display backend

Files involved:

- `ui-tauri-react/src-tauri/src/hardware/display_config.rs`
- optionally a small extracted helper module if `display_config.rs` becomes too dense

Responsibilities:

- Detect Hyprland as a `DisplayBackend`.
- Read current monitor state from `hyprctl monitors -j`.
- Translate Hyprland monitor JSON into the existing `DisplayLayout` and `DisplayInfo` types.
- Apply requested layout changes using `hyprctl keyword monitor ...`.
- Implement orientation changes for one or two built-in panels and compute the second panel position from the first panel's logical size.

The Control Panel should continue to work through the existing generic commands and should not need a Hyprland-specific screen.

## Command Strategy

### Runtime control

Runtime display changes should use `hyprctl`, with Hyprland as the live source of truth for the current session.

Expected command families:

- Read monitors: `hyprctl monitors -j`
- Apply monitor state: `hyprctl keyword monitor <connector>,<mode>,<position>,<scale>,transform,<transform>`
- Disable monitor when docked: Hyprland-specific disabled monitor syntax, represented through the same backend function that handles dock mode and layout application

The implementation should isolate command construction behind backend helpers so layout logic is not interwoven with shell token formatting.

### Optional startup snippet

The optional Hyprland snippet should be minimal and intentionally narrow in scope.

Allowed responsibilities:

- session startup glue such as ensuring the Zenbook session agent is launched in Hyprland if the existing systemd user path needs help
- optional comments that explain how to remove or disable the snippet

Disallowed responsibilities:

- storing the canonical dual-screen layout
- continuously reapplying layout on every config reload
- replacing runtime decisions made by the daemon or session agent
- rewriting broader Hyprland configuration outside the Zenbook-specific include

If the snippet cannot be installed automatically, setup should warn clearly and continue when runtime-only support is still usable.

## Behavior Design

### Installer detection

The unified installer should treat Hyprland like the existing supported desktops:

- detect any environment token containing `hyprland`
- if exactly one supported session is detected, run that setup script
- if detection is ambiguous, show the same manual fallback guidance pattern and list `./setup-hyprland.sh` alongside the other manual entry points

### Dock mode

Dock mode should follow the current product behavior:

- When the keyboard is attached:
  - ensure `eDP-1` is enabled
  - disable `eDP-2`
- When the keyboard is detached:
  - ensure both `eDP-1` and `eDP-2` are enabled
  - place `eDP-1` at `(0,0)`
  - place `eDP-2` directly below `eDP-1` using the primary panel's logical height

This should happen in the session agent, parallel to the current GNOME/KDE/Niri flow.

### Display layout read and write

Layout support should provide parity with the other backends:

- Reading layout should capture:
  - connector name
  - width and height
  - refresh rate
  - scale
  - x and y
  - transform
  - primary status when derivable
- Applying layout should:
  - enable outputs present in the requested layout
  - update their transform, scale, and position
  - disable built-in outputs omitted from the requested layout when the request intentionally represents a single-screen state

When Hyprland JSON omits fields required by the generic model, the backend should use conservative defaults and return an error only when the layout would otherwise be nonsensical.

### Orientation

Orientation control should preserve the current compositor-agnostic contract:

- `Normal`
- `Left`
- `Right`
- `Inverted`

For a single enabled built-in monitor, rotate `eDP-1` and reset its position to `(0,0)`.

For two enabled built-in monitors:

- apply the same transform to `eDP-1` and `eDP-2`
- recompute the second panel's position from the logical size of the transformed first panel
- place the second panel:
  - below for `Normal`
  - to the left for `Left`
  - to the right for `Right`
  - above for `Inverted`

## Error Handling

Hyprland support should fail clearly rather than guessing:

- If `hyprctl` is unavailable, return an unsupported or unavailable Hyprland backend error.
- If monitor JSON is malformed, include the parser failure in the returned message.
- If a requested connector is not present, return a connector-specific error rather than silently redirecting the layout.
- If only one internal panel is visible, operate on that visible panel and avoid failing multi-output logic that can safely degrade.
- If a multi-step layout application partially fails, return the exact stderr from the failed command so logs and UI diagnostics are actionable.
- If snippet installation fails but runtime support remains viable, emit a warning during setup instead of aborting the whole install.

## Testing Strategy

### Automated tests

Add or extend unit tests for:

- backend detection when environment variables contain `hyprland`
- Hyprland monitor JSON parsing into `DisplayLayout`
- transform token to `Orientation` mapping and reverse mapping
- logical size extraction used for orientation and dock positioning
- enabled/disabled output counting
- installer detection for Hyprland session tokens

Prefer parser-focused tests built from representative `hyprctl monitors -j` payloads so backend behavior can be validated without needing a live Hyprland session in CI.

### Manual verification

Verify at least these flows in a real Hyprland session:

1. Fresh install through `install.sh`
2. Manual install through `./setup-hyprland.sh`
3. Keyboard attach disables the lower panel
4. Keyboard detach enables and repositions the lower panel
5. Orientation changes work for all four orientations
6. Layout edits from the Control Panel apply correctly
7. Session-agent restart recovers and continues to operate
8. Optional snippet install and remove flow behaves predictably

## Documentation

Update documentation to make Hyprland a first-class supported session:

- add Hyprland to the quick-start requirements list
- update installer detection language
- add `setup-hyprland.sh` to manual fallback examples
- document Hyprland dependencies
- explain the optional snippet and what it does
- include troubleshooting guidance for missing `hyprctl`, stale include lines, or monitor command failures

## Risks and Mitigations

### Risk: Hyprland monitor command syntax is stricter than the generic layout model

Mitigation:

- centralize Hyprland command formatting in helper functions
- test representative single-panel and dual-panel command generation
- keep the generic `DisplayLayout` contract unchanged unless Hyprland reveals a real mismatch

### Risk: startup behavior differs across Hyprland user configs

Mitigation:

- keep the snippet optional and narrow
- prefer systemd user services as the primary startup path
- document what the snippet changes and how to remove it

### Risk: layout application may partially succeed

Mitigation:

- apply commands in a deterministic order
- stop on the first hard failure
- surface exact command stderr in logs and UI errors

## Open Implementation Notes

These notes are constraints for planning, not unresolved design items:

- Reuse existing compositor switch points instead of introducing a wider backend refactor in this scope.
- Follow the current setup-script conventions even if there is duplicated shell logic across setup scripts.
- Keep Hyprland support additive so existing GNOME/KDE/Niri behavior remains untouched except for shared detection branches.
