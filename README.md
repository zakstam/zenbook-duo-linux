# Linux for the ASUS Zenbook Duo

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
| Keyboard backlight cycle (F4) | ✅ | ✅ |
| Correct state on boot/resume (suspend & hibernate) | ✅ | ✅ |
| Auto rotation | ✅ | ✅ |
| Function keys (F1 mute, F2 volume down, F3 volume up, F10 bluetooth) | ✅ | ✅ |
| Function keys (F5 brightness down, F6 brightness up) | ✅ | ✅ |
| Function keys (F7 swap displays) | ✅ | ✅ |
| Function keys (F9 mic mute) | ✅ | ❌ |
| Function keys (F8 airplane mode, F11 emojis, F12 ASUS software) | ❌ | ❌ |

### Tested on

- **Models**
    - 2025 Zenbook Duo (UX8406CA)

- **Distros**
    - Ubuntu 25.10
    - Fedora

While I typically recommend Debian installs, and many items worked out of the box with `debian-backports`, Ubuntu 25.10 has so far proven to be the best option for compatibility of newer hardware, such as the Bluetooth module. Once Backports incorporates kernel 6.14, I may personally redo testing in Debian Bookworm.

## Installation

Run the setup script, choose a default keyboard backlight level of 0 (off) to 3 (high), and a default resolution scale (1 = 100%, 1.5 = 150%, 1.66 = 166%, 2 = 200%).

```bash
./setup.sh
What would you like to use for the default keyboard backlight brightness [0-3]? 1
What would you like to use for monitor scale (1 = 100%, 1.5 = 150%, 2=200%) [1-2]? 1
...
```

This will set up the required systemd services to handle all the above functionality. A log file will be created in `/tmp/duo/` when the services are running.
