use std::process::Command;

use crate::models::{DuoStatus, EventCategory, HardwareEvent};
use crate::runtime::state::RuntimeState;

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyAction {
    SetWifi(bool),
    SetBluetooth(bool),
    SetBacklight(u8),
    ApplyDisplayMode { attached: bool, scale: f64 },
}

pub fn apply_transition_policy(
    state: &mut RuntimeState,
    previous: &DuoStatus,
) -> Vec<PolicyAction> {
    let mut actions = Vec::new();

    if previous.keyboard_attached == state.status.keyboard_attached {
        if state.status.keyboard_attached {
            state.remembered_wifi_enabled = Some(state.status.wifi_enabled);
            state.remembered_bluetooth_enabled = Some(state.status.bluetooth_enabled);
        }
        return actions;
    }

    if !state.status.keyboard_attached {
        state.remembered_wifi_enabled = Some(previous.wifi_enabled);
        state.remembered_bluetooth_enabled = Some(previous.bluetooth_enabled);
    }

    if state.status.keyboard_attached {
        if let Some(wifi_enabled) = state.remembered_wifi_enabled {
            actions.push(PolicyAction::SetWifi(wifi_enabled));
            state.recent_events.push(HardwareEvent::info(
                EventCategory::Network,
                if wifi_enabled {
                    "Restoring Wi-Fi on attach"
                } else {
                    "Keeping Wi-Fi disabled on attach"
                },
                "rust-daemon",
            ));
            state.status.wifi_enabled = wifi_enabled;
        }

        if let Some(bluetooth_enabled) = state.remembered_bluetooth_enabled {
            actions.push(PolicyAction::SetBluetooth(bluetooth_enabled));
            state.recent_events.push(HardwareEvent::info(
                EventCategory::Bluetooth,
                if bluetooth_enabled {
                    "Restoring Bluetooth on attach"
                } else {
                    "Keeping Bluetooth disabled on attach"
                },
                "rust-daemon",
            ));
            state.status.bluetooth_enabled = bluetooth_enabled;
        }

        actions.push(PolicyAction::SetBacklight(state.settings.default_backlight));
        actions.push(PolicyAction::ApplyDisplayMode {
            attached: true,
            scale: state.settings.default_scale,
        });
    } else {
        actions.push(PolicyAction::SetBluetooth(true));
        actions.push(PolicyAction::ApplyDisplayMode {
            attached: false,
            scale: state.settings.default_scale,
        });
        state.status.bluetooth_enabled = true;
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Bluetooth,
            "Enabled Bluetooth on detach",
            "rust-daemon",
        ));
    }

    if state.recent_events.len() > 500 {
        let overflow = state.recent_events.len() - 500;
        state.recent_events.drain(0..overflow);
    }

    actions
}

pub fn set_wifi_enabled(enabled: bool) -> Result<(), String> {
    let target = if enabled { "on" } else { "off" };
    let output = Command::new("nmcli")
        .args(["radio", "wifi", target])
        .output()
        .map_err(|e| format!("Failed to run nmcli: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn set_bluetooth_enabled(enabled: bool) -> Result<(), String> {
    let action = if enabled { "unblock" } else { "block" };
    let output = Command::new("rfkill")
        .args([action, "bluetooth"])
        .output()
        .map_err(|e| format!("Failed to run rfkill: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ConnectionType;

    fn status(attached: bool, wifi: bool, bluetooth: bool) -> DuoStatus {
        DuoStatus {
            keyboard_attached: attached,
            connection_type: if attached {
                ConnectionType::Usb
            } else {
                ConnectionType::None
            },
            wifi_enabled: wifi,
            bluetooth_enabled: bluetooth,
            ..DuoStatus::default()
        }
    }

    #[test]
    fn restores_disabled_radios_after_detach_attach_cycle() {
        let mut state = RuntimeState {
            status: status(false, false, false),
            ..RuntimeState::default()
        };
        let previous_attached = status(true, false, false);

        let detach_actions = apply_transition_policy(&mut state, &previous_attached);

        assert_eq!(state.remembered_wifi_enabled, Some(false));
        assert_eq!(state.remembered_bluetooth_enabled, Some(false));
        assert!(detach_actions.contains(&PolicyAction::SetBluetooth(true)));
        assert!(state.status.bluetooth_enabled);

        let previous_detached = state.status.clone();
        state.status = status(true, true, true);

        let attach_actions = apply_transition_policy(&mut state, &previous_detached);

        assert!(attach_actions.contains(&PolicyAction::SetWifi(false)));
        assert!(attach_actions.contains(&PolicyAction::SetBluetooth(false)));
        assert!(!state.status.wifi_enabled);
        assert!(!state.status.bluetooth_enabled);
    }

    #[test]
    fn updates_remembered_radios_while_still_attached() {
        let mut state = RuntimeState {
            status: status(true, false, true),
            ..RuntimeState::default()
        };
        let previous = status(true, true, true);

        let actions = apply_transition_policy(&mut state, &previous);

        assert!(actions.is_empty());
        assert_eq!(state.remembered_wifi_enabled, Some(false));
        assert_eq!(state.remembered_bluetooth_enabled, Some(true));
    }
}
