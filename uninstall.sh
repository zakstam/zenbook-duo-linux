#!/bin/bash
# Uninstallation script for ASUS Zenbook Duo Linux dual-screen management.
# Reverses everything installed by setup.sh and (optionally) the UI app.

echo "Uninstalling Zenbook Duo Linux..."

# ============================================================================
# ARGUMENT PARSING
# ============================================================================

# Defaults: remove UI + user config for a clean reinstall.
KEEP_UI=false
KEEP_CONFIG=false

while [ "$#" -gt 0 ]; do
    case "$1" in
        --keep-ui)
            KEEP_UI=true
            shift
            ;;
        --keep-config)
            KEEP_CONFIG=true
            shift
            ;;
        -h|--help)
            echo "Usage: ./uninstall.sh [--keep-ui] [--keep-config]"
            echo ""
            echo "  --keep-ui     Keep the Zenbook Duo Control UI package installed"
            echo "  --keep-config Keep user config files (e.g. ~/.config/zenbook-duo)"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            echo "Run with --help for usage."
            exit 1
            ;;
    esac
done

# ============================================================================
# STOP RUNNING UI (BEST-EFFORT)
# ============================================================================

# Close the UI if it's running so we can safely update/remove binaries.
pkill -f zenbook-duo-control 2>/dev/null || true

# ============================================================================
# SYSTEMD SERVICES
# ============================================================================

# Stop running services
sudo systemctl stop zenbook-duo.service 2>/dev/null
systemctl --user stop zenbook-duo-user.service 2>/dev/null

# Disable services
sudo systemctl disable zenbook-duo.service 2>/dev/null
sudo systemctl --global disable zenbook-duo-user.service 2>/dev/null

# Remove service files and sleep hook
sudo rm -f /etc/systemd/system/zenbook-duo.service
sudo rm -f /etc/systemd/user/zenbook-duo-user.service
sudo rm -f /usr/lib/systemd/system-sleep/duo

# Reload systemd
sudo systemctl daemon-reload
systemctl --user daemon-reload

# ============================================================================
# UDEV & HWDB RULES
# ============================================================================

sudo rm -f /etc/udev/rules.d/90-zenbook-duo-keyboard.rules
sudo rm -f /etc/udev/hwdb.d/90-zenbook-duo-keyboard.hwdb
sudo systemd-hwdb update
sudo udevadm trigger

# ============================================================================
# SUDOERS ENTRIES
# ============================================================================

# Remove sudoers lines added by setup.sh (matching /tmp/duo/ paths)
if sudo grep -q "/tmp/duo/" /etc/sudoers; then
    sudo sed -i '\|/tmp/duo/|d' /etc/sudoers
fi
if sudo grep -q "card1-eDP-2-backlight/brightness" /etc/sudoers; then
    sudo sed -i '\|card1-eDP-2-backlight/brightness|d' /etc/sudoers
fi
if sudo grep -q "intel_backlight/brightness" /etc/sudoers; then
    sudo sed -i '\|intel_backlight/brightness|d' /etc/sudoers
fi

# ============================================================================
# INSTALLED SCRIPT & RUNTIME FILES
# ============================================================================

sudo rm -f /usr/local/bin/duo
rm -rf /tmp/duo
# Newer versions use a per-user directory (/tmp/duo-$UID). Remove only for current user.
UID_NUM="$(id -u 2>/dev/null || echo "")"
if [ -n "$UID_NUM" ]; then
    rm -rf "/tmp/duo-$UID_NUM"
fi

# ============================================================================
# UI APP (RPM/DEB) + DESKTOP ENTRIES
# ============================================================================

if [ "$KEEP_UI" = false ]; then
    # Remove the package if installed.
    if command -v dnf >/dev/null 2>&1; then
        sudo dnf remove -y zenbook-duo-control 2>/dev/null || true
    elif command -v apt >/dev/null 2>&1; then
        sudo apt remove -y zenbook-duo-control 2>/dev/null || true
        sudo apt autoremove -y 2>/dev/null || true
    elif command -v rpm >/dev/null 2>&1; then
        sudo rpm -e zenbook-duo-control 2>/dev/null || true
    elif command -v dpkg >/dev/null 2>&1; then
        sudo dpkg -r zenbook-duo-control 2>/dev/null || true
    fi

    # Remove any legacy user-local desktop entry created by older install-ui.sh versions.
    rm -f "$HOME/.local/share/applications/zenbook-duo-control.desktop" 2>/dev/null || true
    update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
fi

# ============================================================================
# USER CONFIG (UI SETTINGS)
# ============================================================================

if [ "$KEEP_CONFIG" = false ]; then
    rm -rf "$HOME/.config/zenbook-duo" 2>/dev/null || true
fi

# ============================================================================
# FINISH
# ============================================================================

echo "Uninstall complete."
echo "Note: Your user was not removed from the 'input' group."
echo "To remove manually: sudo gpasswd -d \$USER input"
