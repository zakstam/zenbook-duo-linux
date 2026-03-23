use std::process::Command;

use crate::models::{DuoStatus, EventCategory, HardwareEvent};
use crate::runtime::state::RuntimeState;

#[derive(Debug, Clone)]
pub enum PolicyAction {
    SetWifi(bool),
    SetBluetooth(bool),
    SetBacklight(u8),
    SetDockMode { attached: bool, scale: f64 },
}

pub fn apply_transition_policy(
    state: &mut RuntimeState,
    previous: &DuoStatus,
) -> Vec<PolicyAction> {
    let mut actions = Vec::new();

    if state.status.keyboard_attached {
        state.remembered_wifi_enabled = Some(state.status.wifi_enabled);
        state.remembered_bluetooth_enabled = Some(state.status.bluetooth_enabled);
    }

    if previous.keyboard_attached == state.status.keyboard_attached {
        return actions;
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
        actions.push(PolicyAction::SetDockMode {
            attached: true,
            scale: state.settings.default_scale,
        });
    } else {
        actions.push(PolicyAction::SetBluetooth(true));
        actions.push(PolicyAction::SetDockMode {
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
