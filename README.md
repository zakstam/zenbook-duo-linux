# Linux for the ASUS Zenbook Duo

This project adds better Linux support for the Zenbook Duo by running a small background service that reacts to the keyboard being attached/detached (USB or Bluetooth) and keeps the dual-screen experience usable.

## Quick Start (Non-technical)

### What you need

- An ASUS Zenbook Duo
- GNOME on Wayland or KDE Plasma on Wayland (tested on Fedora; Ubuntu GNOME should also work)
- A Terminal and your sudo password (the installer needs to change system settings)

### Install (recommended)

1. Download this repo (GitHub "Code" → "Download ZIP"), then extract it.
2. Open a Terminal in the extracted folder.
3. Run the installer and answer the prompts:

- GNOME: `./setup-gnome.sh`
- KDE: `./setup-kde.sh`

Notes:
- If you prefer to run it with sudo, use `sudo -E ./setup-gnome.sh` or `sudo -E ./setup-kde.sh` (so per-user setup targets your user session).
- If you re-run the installer, restart the user service: `systemctl --user restart zenbook-duo-user.service`

4. Log out and back in (needed for permission changes).

### Optional: install the Control Panel app (UI)

If you want a desktop app to toggle settings easily (run the appropriate setup script first):

```bash
./install-ui.sh
```

You can also do a one-line install (downloads the repo to a temp folder, builds, and installs):

```bash
curl -fsSLo /tmp/install-ui.sh https://raw.githubusercontent.com/zakstam/zenbook-duo-linux/main/install-ui.sh && bash /tmp/install-ui.sh
```

### Uninstall

To remove the background service and system changes:

```bash
./uninstall.sh
```

To remove the optional UI app:

- Fedora / RHEL-based: `sudo dnf remove zenbook-duo-control`
- Debian / Ubuntu-based: `sudo apt remove zenbook-duo-control`

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
- USB top row defaults to media keys; hold `Fn` for `F1`-`F12`.
- Do not install hwdb remaps for `KEYBOARD_KEY_7003*` on USB (it overrides the Fn layer).

### Requirements

- ASUS Zenbook Duo (USB vendor `0B05`, product `1B2C`)
- Linux with GNOME on Wayland or KDE Plasma on Wayland (tested with Fedora)
- `systemd` for service management
- GNOME: `gdctl` (part of `mutter`) for display configuration
- KDE: `kscreen-doctor` (part of `kscreen`) for display configuration

### What `./setup-gnome.sh` / `./setup-kde.sh` change

- Installs dependencies:
  - Common: `inotify-tools`, `usbutils`, `iio-sensor-proxy`, `python3-usb`/`python3-pyusb`, `evtest`
  - GNOME: `mutter`/`gdctl` (via `setup-gnome.sh`)
  - KDE: `kscreen`/`kscreen-doctor` (via `setup-kde.sh`)
- Installs `duo.sh` to `/usr/local/bin/duo` (or uses repo path in `--dev-mode`)
- Installs helper scripts to `/usr/local/libexec/zenbook-duo` and adds sudoers rules for brightness/backlight helper commands
- Adds your user to the `input` group (logout/login required)
- Installs a udev rule for the Zenbook Duo keyboard
- Installs/enables systemd units:
  - `zenbook-duo.service` (system boot/shutdown)
  - `zenbook-duo-user.service` (user session)

### Troubleshooting

- Nothing happens when docking/undocking:
  - Check the user service is running: `systemctl --user status zenbook-duo-user.service`
  - Watch logs while docking/undocking: `journalctl --user -u zenbook-duo-user.service -f`
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

Other distros: install dependencies manually and run `./setup-gnome.sh` or `./setup-kde.sh` (it exits if it cannot detect your package manager).

### Control Panel UI (Tauri + React)

- Build & install: `./install-ui.sh`
- Dev mode:

```bash
cd ui-tauri-react
npm install
npm run dev
```

### Development

To iterate on `duo.sh` without reinstalling:

```bash
./setup-gnome.sh --dev-mode
```

---

## Fedora: “Nobara-like” setup helper

If you’re on Fedora and want a more “Nobara-like” out-of-box experience (RPM Fusion, codecs, common gaming tools), there’s an optional helper script:

```bash
./nobara-like.sh
```

It can also add the Nobara COPR repo definitions **disabled by default**, so you can cherry-pick packages without mixing repos during normal upgrades.
