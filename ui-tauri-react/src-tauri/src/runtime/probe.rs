use crate::hardware::{display_layout, sysfs};
use crate::runtime::host::{CommandRunner, ProcessCommandRunner};
use crate::models::{ConnectionType, DisplayLayout, DuoStatus, Orientation};

pub fn current_status() -> DuoStatus {
    let mut status = sysfs::get_full_status();
    let connection_type = sysfs::detect_connection_type();
    status.keyboard_attached = keyboard_attached(&connection_type);
    status.connection_type = connection_type;
    let host = ProcessCommandRunner;
    status.wifi_enabled = wifi_enabled_with(&host);
    status.bluetooth_enabled = bluetooth_enabled_with(&host);
    apply_layout_to_status(
        &mut status,
        display_layout::get_display_layout().ok().as_ref(),
    );
    status
}

pub fn apply_layout_to_status(status: &mut DuoStatus, layout: Option<&DisplayLayout>) {
    status.monitor_count = monitor_count(layout, status.monitor_count);
    status.orientation = inferred_orientation(layout).unwrap_or(status.orientation.clone());
}

pub fn keyboard_attached(connection_type: &ConnectionType) -> bool {
    matches!(connection_type, ConnectionType::Usb)
}

pub fn wifi_enabled() -> bool {
    wifi_enabled_with(&ProcessCommandRunner)
}

pub fn wifi_enabled_with(host: &impl CommandRunner) -> bool {
    host.output("nmcli", &["radio", "wifi"])
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().next().unwrap_or_default().trim() == "enabled")
        .unwrap_or(false)
}

pub fn bluetooth_enabled() -> bool {
    bluetooth_enabled_with(&ProcessCommandRunner)
}

pub fn bluetooth_enabled_with(host: &impl CommandRunner) -> bool {
    host.output("rfkill", &["-n", "-o", "SOFT", "list", "bluetooth"])
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
    fn reads_radio_status_through_host_adapter() {
        let host = crate::runtime::host::tests::FakeCommandRunner::new([
            Ok(crate::runtime::host::tests::FakeCommandRunner::success("enabled\n")),
            Ok(crate::runtime::host::tests::FakeCommandRunner::success("unblocked\n")),
        ]);

        assert!(wifi_enabled_with(&host));
        assert!(bluetooth_enabled_with(&host));
        assert_eq!(
            host.calls(),
            vec![
                ("nmcli".to_string(), vec!["radio".to_string(), "wifi".to_string()]),
                (
                    "rfkill".to_string(),
                    vec![
                        "-n".to_string(),
                        "-o".to_string(),
                        "SOFT".to_string(),
                        "list".to_string(),
                        "bluetooth".to_string(),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn usb_connection_counts_as_attached_keyboard() {
        assert!(keyboard_attached(&ConnectionType::Usb));
        assert!(!keyboard_attached(&ConnectionType::Bluetooth));
        assert!(!keyboard_attached(&ConnectionType::None));
    }

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
