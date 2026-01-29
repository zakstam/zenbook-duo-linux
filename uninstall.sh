#!/bin/bash
# Uninstallation script for ASUS Zenbook Duo Linux dual-screen management.
# Reverses everything installed by setup.sh.

echo "Uninstalling Zenbook Duo Linux..."

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

# ============================================================================
# FINISH
# ============================================================================

echo "Uninstall complete."
echo "Note: Your user was not removed from the 'input' group."
echo "To remove manually: sudo gpasswd -d \$USER input"
