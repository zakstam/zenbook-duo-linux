use std::process::Command;

use crate::hardware::{display_config, sysfs};
use crate::models::{DisplayLayout, DuoStatus, Orientation};

pub fn current_status() -> DuoStatus {
    let mut status = sysfs::get_full_status();
    status.keyboard_attached = keyboard_attached();
    status.connection_type = sysfs::detect_connection_type();
    status.wifi_enabled = wifi_enabled();
    status.bluetooth_enabled = bluetooth_enabled();
    apply_layout_to_status(
        &mut status,
        display_config::get_display_layout().ok().as_ref(),
    );
    status
}

pub fn apply_layout_to_status(status: &mut DuoStatus, layout: Option<&DisplayLayout>) {
    status.monitor_count = monitor_count(layout, status.monitor_count);
    status.orientation = inferred_orientation(layout).unwrap_or(status.orientation.clone());
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

fn monitor_count(layout: Option<&crate::models::DisplayLayout>, current: u32) -> u32 {
    layout
        .map(|layout| layout.displays.len() as u32)
        .filter(|count| *count > 0)
        .or_else(|| if current > 0 { Some(current) } else { None })
        .unwrap_or(0)
}

fn inferred_orientation(layout: Option<&crate::models::DisplayLayout>) -> Option<Orientation> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_primary_display_transform_to_status() {
        let mut status = DuoStatus::default();
        let layout = DisplayLayout {
            displays: vec![
                crate::models::DisplayInfo {
                    connector: "eDP-1".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.25,
                    x: 0,
                    y: 0,
                    transform: 90,
                    primary: true,
                    current_mode: crate::models::DisplayMode {
                        mode_id: "2880x1800@120".into(),
                        backend_mode_id: None,
                        width: 2880,
                        height: 1800,
                        refresh_rate: 120.0,
                    },
                    available_modes: vec![crate::models::DisplayMode {
                        mode_id: "2880x1800@120".into(),
                        backend_mode_id: None,
                        width: 2880,
                        height: 1800,
                        refresh_rate: 120.0,
                    }],
                    refresh_policy: crate::models::RefreshPolicy::Fixed,
                    supports_dynamic_refresh: false,
                },
                crate::models::DisplayInfo {
                    connector: "eDP-2".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.25,
                    x: 0,
                    y: 1800,
                    transform: 0,
                    primary: false,
                    current_mode: crate::models::DisplayMode {
                        mode_id: "2880x1800@120".into(),
                        backend_mode_id: None,
                        width: 2880,
                        height: 1800,
                        refresh_rate: 120.0,
                    },
                    available_modes: vec![crate::models::DisplayMode {
                        mode_id: "2880x1800@120".into(),
                        backend_mode_id: None,
                        width: 2880,
                        height: 1800,
                        refresh_rate: 120.0,
                    }],
                    refresh_policy: crate::models::RefreshPolicy::Fixed,
                    supports_dynamic_refresh: false,
                },
            ],
        };

        apply_layout_to_status(&mut status, Some(&layout));

        assert_eq!(status.orientation, Orientation::Left);
        assert_eq!(status.monitor_count, 2);
    }
}
