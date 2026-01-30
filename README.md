# Linux for the ASUS Zenbook Duo

![Zenbook Duo Control USB](sc.png)
![Zenbook Duo Control BLUETOOTH](sc2.png)

A script to manage features on the Zenbook Duo.

## Features

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

## Requirements

- ASUS Zenbook Duo (USB vendor `0B05`, product `1B2C`)
- Linux with GNOME on Wayland (tested with Fedora)
- `systemd` for service management
- `gdctl` (part of `mutter`) for display configuration

## Installation

1. Clone the repository:

```bash
git clone https://github.com/zakstam/zenbook-duo-linux.git
cd zenbook-duo-linux
```

2. Run the setup script:

```bash
./setup.sh
```

The setup script will:

- Prompt you for your preferred **keyboard backlight level** (0-3) and **display scale** (1-2)
- Install required packages (`inotify-tools`, `usbutils`, `mutter`/`gdctl`, `iio-sensor-proxy`, `python3-usb`, `evtest`)
- Copy `duo.sh` to `/usr/local/bin/duo`
- Configure passwordless `sudo` for brightness and backlight commands
- Add your user to the `input` group (logout/login required)
- Install udev rules for the Zenbook Duo keyboard
- Create systemd services for boot/shutdown and user session events

### Upgrading from older versions

If you previously installed a hwdb key remap, remove it so `Fn`+`F1`-`F12` works on USB:

```bash
sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb
sudo systemd-hwdb update
sudo udevadm trigger
```

3. Log out and back in (required for the `input` group change to take effect).

### Supported Distros

| Distro | Package Manager |
|--------|----------------|
| Fedora / RHEL-based | `dnf` |
| Debian / Ubuntu-based | `apt` |

For other distributions, install the dependencies manually and run the setup script — it will warn you and exit if it cannot detect your package manager.

## Uninstallation

Run the uninstall script from the repository:

```bash
./uninstall.sh
```

This removes all systemd services, udev/hwdb rules, sudoers entries, the installed script, and runtime files. Your user will not be removed from the `input` group automatically — the script prints instructions for that.

## Control Panel UI

An optional desktop control panel is available in the `ui-tauri-react/` directory, built with Tauri and React.

### Install (recommended)

Use the helper script to install prerequisites, build, and install the package for your distro:

```bash
./install-ui.sh
```

### One-line install (curl)

If you just want to install the UI without cloning the repo manually:

```bash
curl -fsSL https://raw.githubusercontent.com/zakstam/zenbook-duo-linux/main/install-ui.sh | bash
```

### Install from package

Build the package and install it:

```bash
cd ui-tauri-react
npm install
npm run build -- --bundles rpm   # Fedora / RHEL-based
# or:
# npm run build -- --bundles deb # Debian / Ubuntu-based
```

Then install the package for your distro:

```bash
# Fedora / RHEL-based
sudo dnf install src-tauri/target/release/bundle/rpm/Zenbook\ Duo\ Control-0.1.0-1.x86_64.rpm

# Debian / Ubuntu-based
sudo dpkg -i src-tauri/target/release/bundle/deb/Zenbook\ Duo\ Control_0.1.0_amd64.deb
```

### Run in development mode

```bash
cd ui-tauri-react
npm install
npm run dev
```

This launches the app with hot reload. The daemon must already be running (via `./setup.sh`).

### Hotkey note (Fedora Workstation / GNOME Wayland)

GNOME Wayland generally does not allow apps to register global hotkeys directly.
Instead, create a GNOME custom shortcut that runs:

```bash
zenbook-duo-control --toggle-usb-media-remap
```

## Development

To iterate on `duo.sh` without reinstalling, use dev mode:

```bash
./setup.sh --dev-mode
```

This skips package installation and configures systemd to run `duo.sh` directly from the repository directory instead of `/usr/local/bin/duo`.
