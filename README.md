# Linux for the ASUS Zenbook Duo

This project adds better Linux support for the Zenbook Duo by running a small background service that reacts to the keyboard being attached/detached (USB or Bluetooth) and keeps the dual-screen experience usable.

## Quick Start (Non-technical)

### What you need

- An ASUS Zenbook Duo
- GNOME on Wayland, KDE Plasma on Wayland, or Niri
- A Terminal and your sudo password (the installer needs to change system settings)

### Install (recommended)

One-line install:

```bash
curl -fsSL https://raw.githubusercontent.com/zakstam/zenbook-duo-linux/main/install.sh | bash
```

1. Download this repo (GitHub "Code" → "Download ZIP"), then extract it.
2. Open a Terminal in the extracted folder.
3. Run the installer and answer the prompts:

```bash
./install.sh
```

Notes:
- `install.sh` auto-detects GNOME, KDE Plasma, or Niri and then runs the matching setup script plus the UI installer.
- If you prefer to run it with sudo, use `sudo -E ./install.sh` (so per-user setup targets your user session).
- If you re-run the installer, restart the session agent: `systemctl --user restart zenbook-duo-session-agent.service`

4. Log out and back in (needed for permission changes).

Manual fallback:

```bash
./setup-gnome.sh
# or
./setup-kde.sh
# or
./setup-niri.sh
```

### Optional: install or update just the Control Panel app (UI)

If you only want to build/update the desktop app:

```bash
./install-ui.sh
```

On desktop sessions, the installer now prefers a graphical admin-password prompt for the system steps and falls back to terminal `sudo` when needed.

### Uninstall

To remove the background service and system changes:

```bash
./uninstall.sh
```

To remove the optional UI app:

- Fedora / RHEL-based: `sudo dnf remove zenbook-duo-control`
- Debian / Ubuntu-based: `sudo apt remove zenbook-duo-control`
- Arch / CachyOS: `sudo rm -f /usr/local/bin/zenbook-duo-control /usr/share/applications/zenbook-duo-control.desktop /usr/share/pixmaps/zenbook-duo-control.png`

---

## Advanced (Technical)

### Screenshots

![Zenbook Duo Control USB](sc.png)
![Zenbook Duo Control BLUETOOTH](sc2.png)

### Features

| Feature | USB | Bluetooth |
|---------|:---:|:---------:|
| Toggle bottom screen on when keyboard removed | ✅ | ✅ |
| Toggle bottom screen off when keyboard placed | ✅ | ✅ |
| Toggle bluetooth on when keyboard removed | ✅ | ✅ |
| Toggle bluetooth off when keyboard placed (if it was off before) | ✅ | ✅ |
| Screen brightness sync | ✅ | ✅ |
| Reset airplane mode on keyboard attach/detach | ✅ | N/A |
| Keyboard backlight set on boot/attach | ✅ | ✅ |
| Keyboard backlight sync across attach/detach | ✅ | ✅ |
| Keyboard backlight cycle (F4) | ✅ | ✅ |
| Correct state on boot/resume (suspend & hibernate) | ✅ | ✅ |
| Auto rotation | ✅ | ✅ |
| Function keys (F1 mute, F2 volume down, F3 volume up, F10 bluetooth) | ✅ | ✅ |
| Function keys (F5 brightness down, F6 brightness up) | ✅ | ✅ |
| Function keys (F7 swap displays) | ✅ | ✅ |
| Function keys (F9 mic mute) | ✅ | ❌ |
| Function keys (F11 emojis) | ✅ | ✅ (Fn+F11) |
| Function keys (F8 airplane mode, F12 ASUS software) | ❌ | ❌ |
| Correct state on lock/unlock | ✅ | ✅ |
| Fn layer (top row) | ✅ | ✅ |

Notes:
- USB (docked): the optional USB media remap turns the top row into media keys (F1-F3 volume, F4 backlight, F5/F6 brightness, F11 emojis). Holding `Fn` sends plain `F1`-`F12` only if the keyboard reports the Fn key over USB. The remap logs which case applies when it starts: look for `Fn key reported by` or `No keyboard node reports the Fn key` in `/run/user/$UID/zenbook-duo/duo.log`.
- If Fn is not reported, use pause as an Fn-lock: bind `zenbook-duo-control --toggle-remap-pause` to a desktop shortcut (or use the Pause button in the app). While paused the top row sends plain `F1`-`F12`, and you get a notification on each toggle.
- Do not install hwdb remaps for `KEYBOARD_KEY_7003*` on USB (it overrides the Fn layer).

### Requirements

- ASUS Zenbook Duo (USB vendor `0B05`, product `1B2C`; the 2026 UX8407AA uses product `1CD7` and needs extra setup — see [Zenbook Duo 2026 (UX8407AA)](#asus-zenbook-duo-2026-ux8407aa))
- Linux with GNOME on Wayland, KDE Plasma on Wayland, or Niri (tested with Fedora)
- `systemd` for service management
- GNOME: `gdctl` (part of `mutter`) for display configuration
- KDE: `kscreen-doctor` (part of `kscreen`) for display configuration
- Niri: `niri msg` for display configuration

### What `./setup-gnome.sh` / `./setup-kde.sh` / `./setup-niri.sh` change

- Installs dependencies:
  - Common: `usbutils`, `iio-sensor-proxy`, `systemd`
  - GNOME: `mutter`/`gdctl` (via `setup-gnome.sh`)
  - KDE: `kscreen`/`kscreen-doctor` (via `setup-kde.sh`)
  - Niri: `niri` (via `setup-niri.sh`)
- Adds your user to the `input` group (logout/login required)
- Installs a udev rule for the Zenbook Duo keyboard
- Installs/enables Rust runtime units:
  - `zenbook-duo-rust-daemon.service` (system daemon)
  - `zenbook-duo-rust-lifecycle.service` (boot/shutdown + sleep hook)
  - `zenbook-duo-session-agent.service` (user session)
  - The session agent is enabled from the user manager's `default.target`, then syncs the current dock state when your graphical session comes up after reboot/login
- Installs Rust runtime binaries to `/usr/local/libexec/zenbook-duo`
- Adds sudoers rules for brightness writes used by the session agent

Contributor note: the desktop setup scripts are thin wrappers around `setup-common.sh`. When adding or changing supported systems, update the shared helper for common behavior and keep only package names/manual dependency hints in the per-desktop wrapper.

### Contributor compatibility checks

Use the version bump helper when preparing a release:

```bash
./bump-version.sh patch    # or minor, major, or an explicit version like 0.3.4
```

Use the root check script before changing installer, runtime, or UI behavior:

```bash
./check.sh installers   # shell syntax + installer smoke tests
./check.sh rust         # Rust runtime unit tests
./check.sh frontend     # React/TypeScript production build
./check.sh all          # full compatibility pass
```

Supported matrix covered by the installer smoke tests:

| Desktop backend | Setup wrapper | Display command |
|-----------------|---------------|-----------------|
| GNOME on Wayland | `setup-gnome.sh` | `gdctl` |
| KDE Plasma on Wayland | `setup-kde.sh` | `kscreen-doctor` |
| Hyprland | `setup-hyprland.sh` | `hyprctl` |
| Niri | `setup-niri.sh` | `niri msg` |

The Hyprland backend owns only the two internal Duo panels discovered through
DRM eDP metadata. External outputs are inspected but never reconfigured. Systems
with nonstandard internal connector names can set `ZENBOOK_DUO_UPPER_CONNECTOR`
and `ZENBOOK_DUO_LOWER_CONNECTOR` in the session-agent service environment.

| Distro family | Package manager |
|---------------|-----------------|
| Fedora / RHEL-based | `dnf` |
| Debian / Ubuntu-based | `apt` |
| Arch / CachyOS | `pacman` |

Compatibility checklist for maintainers:

- Keep common setup behavior in `setup-common.sh`; keep desktop wrappers limited to backend-specific packages and manual dependency hints.
- Keep settings defaults aligned across `setup-common.sh`, the Rust `DuoSettings` defaults, and the frontend default settings helper. The installer writes `setupCompleted=true`; a missing settings file should still show first-run setup.
- Preserve GNOME, KDE, and Niri command arguments when refactoring display code unless a backend-specific behavior change is intentional and tested.
- Keep desktop readiness probes centralized in the Rust session helpers so GNOME, KDE, Hyprland, and Niri fallback behavior stays consistent.
- Update `tests/install-stdin-test.sh` whenever supported desktops, package managers, service units, defaults, or installer entrypoints change.
- Run the narrow `./check.sh` target for the area you touched; run `./check.sh all` before handing off broad cross-area changes.

### Troubleshooting

- Nothing happens when docking/undocking:
  - Check the services are running: `systemctl status zenbook-duo-rust-daemon.service` and `systemctl --user status zenbook-duo-session-agent.service`
  - Watch daemon logs: `journalctl -u zenbook-duo-rust-daemon.service -f`
- Reboot/login or resume comes up in the wrong layout:
  - After login or resume, the lifecycle handler and session agent re-sync the current attached/detached state without a manual restart
  - Check `systemctl --user status zenbook-duo-session-agent.service`; an early `No supported session backend became ready before timeout; continuing to wait` warning is OK if the service remains active
  - Confirm your user manager has the desktop-session environment: `systemctl --user show-environment | grep -E 'DISPLAY|WAYLAND_DISPLAY|NIRI_SOCKET|HYPRLAND_INSTANCE_SIGNATURE|XDG_CURRENT_DESKTOP|XDG_SESSION_DESKTOP|DESKTOP_SESSION|XDG_SESSION_TYPE'`
  - If those variables are missing after reinstalling, rerun `./install.sh` from an active desktop session, then log out and back in once
- Keyboard media/Fn keys stop working after suspend or reattaching the keyboard:
  - The optional USB media remap helper is stopped before sleep and retried automatically after resume, so a manual service restart should not be needed.
  - If recovery still fails, check `journalctl -u zenbook-duo-rust-daemon.service -f` for `USB media remap auto-start failed` or repeated `No such device` messages.
  - You do not need a separate `/etc/udev/rules.d/*uinput*` rule for this project.
- `KBLIGHT - Device lost, re-scanning` in a loop:
  - You likely need to log out and back in so your session gets the `input` group membership

### Upgrading from older versions

If you previously installed a hwdb key remap, remove it so `Fn`+`F1`-`F12` works on USB:

```bash
sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb
sudo systemd-hwdb update
sudo udevadm trigger
```

### Supported distros

| Distro | Package Manager |
|--------|----------------|
| Fedora / RHEL-based | `dnf` |
| Debian / Ubuntu-based | `apt` |
| Arch / CachyOS | `pacman` |

Other distros: install dependencies manually and run the setup wrapper for GNOME, KDE, Hyprland, or Niri (it exits if it cannot detect your package manager).

### Control Panel UI (Tauri + React)

- Build & install: `./install-ui.sh`
- Arch / CachyOS note: `install-ui.sh` builds the UI locally, then installs `zenbook-duo-control` to `/usr/local/bin` and desktop assets under `/usr/share`
- Dev mode:

```bash
cd ui-tauri-react
npm install
npm run dev
```

## ASUS Zenbook Duo 2026 (UX8407AA)

The 2026 model (Intel Core Ultra "Panther Lake", Intel Arc iGPU on the `xe` driver, keyboard USB ID `0b05:1cd7`) works with this project, but the stock kernel and firmware have several bugs that must be fixed first. Verified on CachyOS + KDE Plasma on Wayland with a patched 7.2-rc5 kernel. The kernel patches come from [zenbook-duo26-Ubuntu26.04](https://github.com/therealarnold666/zenbook-duo26-Ubuntu26.04) (an Ubuntu-targeted derivative of this project) — use its patches, but do not run its installer on non-Ubuntu distros; run this project's installer instead.

This project detects the UX8407AA via DMI (`/sys/class/dmi/id/board_name`) and automatically compensates for its upside-down top panel in the KDE display backend, so no code changes are needed — only the system-level steps below.

### 1. Kernel patches — keyboard-detach freeze and audio

- **Detach freeze / bottom screen never re-enables**: the Port B C20 TCSS PHY loses its power ack when eDP-2 is toggled. Fix: DMI-gated patch `0001` (i915/xe shared display code) from the repo above.
- **No audio** (`aplay -l` shows no cards): a ghost RT722 SoundWire device makes `sof_sdw` probing fail with `-EEXIST` (duplicate `SDW3-Playback-SimpleJack`). Fix: patch `0002` (adds UX8407AA to the SoundWire DMI ghost-device quirk table).

Both apply cleanly to kernel 7.2-rc5. On Arch-based distros, append the patch files to your kernel PKGBUILD's `source` array (with `SKIP` checksums) and rebuild; then pin the patched kernel (e.g. `IgnorePkg` in `/etc/pacman.conf`) so an update does not replace it before the fixes land upstream.

### 2. Kernel command line

Add to your bootloader's kernel command line:

```
video=eDP-1:panel_orientation=upside_down xe.enable_psr=0 xe.enable_psr2_sel_fetch=0 xe.enable_panel_replay=0 xe.enable_dpcd_backlight=3
```

- `video=eDP-1:panel_orientation=upside_down` — the top panel is physically mounted 180° and there is no upstream DRM quirk yet; this fixes the console/LUKS-prompt orientation.
- `xe.enable_psr=0 xe.enable_psr2_sel_fetch=0 xe.enable_panel_replay=0` — avoids PSR/panel-replay flicker and hangs on Panther Lake.
- `xe.enable_dpcd_backlight=3` — enables brightness control (the default backlight path does not work on this panel).
- Do **not** add `xe.enable_dsb=0` on the patched kernel: it forces a slow CPU display path and causes multi-blink flicker on wake from screen-off. It was only ever an interim mitigation for the detach freeze that patch `0001` fixes properly.

### 3. Auto-rotation — ASUS ISH firmware

The accelerometer sits behind the Intel ISH, and the mainline `ish_ptl.bin` firmware fails to load on this machine (`ISH loader: cmd 2 failed` in dmesg, no IIO devices). Fix: extract the ASUS-signed ISH firmware from ASUS's Windows "Intel Integrated Sensor Solution Driver" package and install it (compressed) as `/usr/lib/firmware/intel/ish/ish_ptl.bin.zst`, keeping a backup of the mainline blob. After a reboot, `accel_3d` appears under `/sys/bus/iio/devices/` and `iio-sensor-proxy` works.

Note: linux-firmware package updates will restore the broken mainline blob — re-apply the ASUS blob after firmware updates (a pacman/apt post-transaction hook that compares and restores it automates this).

### 4. Rotation settings (KDE)

- Only the top panel (eDP-1) is mounted upside down; the app applies the per-panel 180° offset automatically on UX8407AA.
- Set `"invertSensorRotation": true` in `~/.config/zenbook-duo/settings.json` (or toggle it in the Control Panel UI).
- Set Screen Rotation to **Manual** for both screens in KDE System Settings → Display & Monitor. Otherwise KWin's own tablet-mode auto-rotate (which does not know about the flipped panel) fights this app whenever the keyboard is detached.
- In the KDE session, eDP-1 showing as "Rotated 180°" in display settings is the correct resting state — KWin ignores the kernel `panel_orientation` quirk, so do not manually reset it to normal.

### 5. Login screen orientation

The greeter runs its own KWin instance, which also ignores the panel-orientation quirk, so the login screen appears upside down. Fix: once your session displays correctly, copy your display config into the greeter's home:

```bash
sudo install -Dm644 -o sddm -g sddm ~/.config/kwinoutputconfig.json /var/lib/sddm/.config/kwinoutputconfig.json
```

On installs using `plasmalogin` instead of SDDM (some CachyOS setups), use `/var/lib/plasmalogin/.config/kwinoutputconfig.json` owned by `plasmalogin:plasmalogin`.

## Fedora: “Nobara-like” setup helper

If you’re on Fedora and want a more “Nobara-like” out-of-box experience (RPM Fusion, codecs, common gaming tools), there’s an optional helper script:

```bash
./nobara-like.sh
```

It can also add the Nobara COPR repo definitions **disabled by default**, so you can cherry-pick packages without mixing repos during normal upgrades.
