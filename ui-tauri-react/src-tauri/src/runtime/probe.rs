use std::process::Command;

use crate::hardware::{display_config, sysfs};
use crate::models::{DuoStatus, Orientation};

pub fn current_status() -> DuoStatus {
    let mut status = sysfs::get_full_status();
    status.keyboard_attached = keyboard_attached();
    status.connection_type = sysfs::detect_connection_type();
    status.wifi_enabled = wifi_enabled();
    status.bluetooth_enabled = bluetooth_enabled();
    let layout = display_config::get_display_layout().ok();
    status.monitor_count = monitor_count(layout.as_ref(), status.monitor_count);
    status.orientation = inferred_orientation(layout.as_ref()).unwrap_or(status.orientation);
    status
}

pub fn keyboard_attached() -> bool {
    Command::new("lsusb")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Zenbook Duo Keyboard"))
        .unwrap_or(false)
}

pub fn wifi_enabled() -> bool {
    Command::new("nmcli")
        .args(["radio", "wifi"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().next().unwrap_or_default().trim() == "enabled")
        .unwrap_or(false)
}

pub fn bluetooth_enabled() -> bool {
    Command::new("rfkill")
        .args(["-n", "-o", "SOFT", "list", "bluetooth"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().next().unwrap_or_default().trim() == "unblocked")
        .unwrap_or(false)
}

fn monitor_count(
    layout: Option<&crate::models::DisplayLayout>,
    current: u32,
) -> u32 {
    layout
        .map(|layout| layout.displays.len() as u32)
        .filter(|count| *count > 0)
        .or_else(|| {
            if current > 0 {
                Some(current)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn inferred_orientation(
    layout: Option<&crate::models::DisplayLayout>,
) -> Option<Orientation> {
    let layout = layout?;
    let display = layout
        .displays
        .iter()
        .find(|display| display.primary)
        .or_else(|| layout.displays.first())?;

    Some(match display.transform {
        90 => Orientation::Left,
        180 => Orientation::Inverted,
        270 => Orientation::Right,
        _ => Orientation::Normal,
    })
}
