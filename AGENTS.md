# Developer notes

The README is for users only. Architecture, checks and maintainer rules live here.

## Architecture

- **Rust system daemon** (`zenbook-duo-rust-daemon.service`): owns hardware state, policy, persistence, privileged operations, logs/events, Bluetooth hotkeys and daemon IPC.
- **Rust lifecycle hook** (`zenbook-duo-rust-lifecycle.service`): boot, shutdown and sleep phases.
- **Rust session agent** (`zenbook-duo-session-agent.service`, user unit): compositor-bound work only: GNOME/KDE/Hyprland/niri display changes, brightness sync, rotation, notifications. It follows whichever desktop is running, so one install serves every desktop on the machine.
- **Tauri/React Control Panel** (`ui-tauri-react/`): a client of the daemon and session APIs; it never owns hardware policy.
- **Bash installers**: `install.sh` detects the desktop and runs `setup-<desktop>.sh`, thin wrappers over the shared `setup-common.sh`; `install-rust-runtime.sh` builds and installs the runtime under `/usr/local/libexec/zenbook-duo`; `install-ui.sh` builds and installs the app.

Display backends:

| Desktop | Setup wrapper | Display command |
|---------|---------------|-----------------|
| GNOME (Wayland) | `setup-gnome.sh` | `gdctl` |
| KDE Plasma (Wayland) | `setup-kde.sh` | `kscreen-doctor` |
| Hyprland | `setup-hyprland.sh` | `hyprctl` |
| niri | `setup-niri.sh` | `niri msg` |

The Hyprland backend only reconfigures the two internal panels found through DRM eDP metadata. Nonstandard connector names can be set with `ZENBOOK_DUO_UPPER_CONNECTOR` and `ZENBOOK_DUO_LOWER_CONNECTOR` in the session agent's environment.

## Checks

```bash
./check.sh installers   # shell syntax + installer smoke tests
./check.sh rust         # Rust runtime unit tests
./check.sh frontend     # React/TypeScript production build
./check.sh all          # everything
```

Run the narrow target for the area you touched; run `./check.sh all` before handing off cross-area changes.

## UI development

```bash
cd ui-tauri-react
npm install
npm run dev
npm run build:frontend  # frontend-only production build
npm run build:rust      # Rust-only release build
npm run build:local     # Tauri release build without bundles
npm run build           # full Tauri build with bundles
```

## Releases

```bash
./bump-version.sh patch    # or minor, major, or an explicit version like 0.3.8
```

It updates `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock` and `tauri.conf.json` together.

## Maintainer rules

- Keep common installer behavior in `setup-common.sh`; desktop wrappers only add backend packages and manual dependency hints.
- Keep settings defaults aligned across `setup-common.sh`, Rust `DuoSettings::default` and frontend `DEFAULT_DUO_SETTINGS`.
- Keep the daemon authoritative and the session agent narrow (see Architecture).
- Preserve GNOME, KDE, Hyprland and niri parity when changing shared behavior.
- Update `tests/install-stdin-test.sh` whenever supported desktops, package managers, service units, defaults or installer entrypoints change.
- Keep the README user-facing: what a Duo owner needs to check support, install, fix a problem or uninstall. Developer detail goes here.
- No personal data in the repo: no real emails, home paths or machine names.
