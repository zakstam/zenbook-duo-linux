use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::ipc::protocol::SessionCommand;
use crate::models::{ConnectionType, EventCategory, HardwareEvent};
use crate::runtime::logger;
use crate::runtime::policy::PolicyAction;
use crate::runtime::state::RuntimeState;

pub fn start(state: Arc<RwLock<RuntimeState>>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;

            let mut next_status = crate::runtime::probe::current_status();
            let session_connected = {
                let guard = state.read().await;
                guard.session_agent.connected
            };
            next_status.service_active = session_connected;

            if let Some(layout) =
                crate::runtime::daemon::session_display_layout(state.clone()).await
            {
                crate::runtime::probe::apply_layout_to_status(&mut next_status, Some(&layout));
                next_status.service_active = true;
            }

            let mut guard = state.write().await;
            let previous = guard.status.clone();

            if previous != next_status {
                let updated = next_status.clone();
                let _ = logger::append_line(format!(
                    "rust-daemon: status transition attached={} monitors={} wifi={} bluetooth={} connection={}",
                    updated.keyboard_attached,
                    updated.monitor_count,
                    updated.wifi_enabled,
                    updated.bluetooth_enabled,
                    connection_label(&updated.connection_type),
                ));
                guard.status = next_status;
                let actions =
                    crate::runtime::policy::apply_transition_policy(&mut guard, &previous);
                push_status_events(&mut guard, &previous, &updated);
                guard.touch();
                if let Err(err) = guard.save() {
                    log::warn!("failed to save monitored runtime state: {err}");
                }
                drop(guard);
                apply_policy_actions(state.clone(), actions).await;
            } else {
                drop(guard);
            }

            reconcile_usb_media_remap(state.clone()).await;
        }
    });
}

async fn reconcile_usb_media_remap(state: Arc<RwLock<RuntimeState>>) {
    let (should_run, is_running) = {
        let guard = state.read().await;
        let should_run = guard.status.keyboard_attached
            && matches!(guard.status.connection_type, ConnectionType::Usb)
            && guard.settings.usb_media_remap_enabled;
        let is_running = crate::commands::usb_media_remap::get_status().running;
        (should_run, is_running)
    };

    if should_run == is_running {
        return;
    }

    if should_run {
        match crate::commands::usb_media_remap::start_remap() {
            Ok(()) => {
                let _ = logger::append_line("rust-daemon: reconciled usb media remap -> started");
            }
            Err(err) => {
                log::warn!("failed to auto-start usb media remap: {err}");
                crate::runtime::daemon::notify_runtime_error(
                    &state,
                    "Zenbook Duo Runtime Error",
                    &format!("USB media remap auto-start failed: {err}"),
                )
                .await;
                let _ = logger::append_line(format!(
                    "rust-daemon: usb media remap auto-start failed: {}",
                    err
                ));
            }
        }
    } else if let Err(err) = crate::commands::usb_media_remap::stop_remap() {
        log::warn!("failed to auto-stop usb media remap: {err}");
        crate::runtime::daemon::notify_runtime_error(
            &state,
            "Zenbook Duo Runtime Error",
            &format!("USB media remap auto-stop failed: {err}"),
        )
        .await;
        let _ = logger::append_line(format!(
            "rust-daemon: usb media remap auto-stop failed: {}",
            err
        ));
    } else {
        let _ = logger::append_line("rust-daemon: reconciled usb media remap -> stopped");
    }
}

async fn apply_policy_actions(state: Arc<RwLock<RuntimeState>>, actions: Vec<PolicyAction>) {
    for action in actions {
        match action {
            PolicyAction::SetWifi(enabled) => {
                if let Err(err) = crate::runtime::policy::set_wifi_enabled(enabled) {
                    log::warn!("failed to set wifi policy action: {err}");
                    crate::runtime::daemon::notify_runtime_error(
                        &state,
                        "Zenbook Duo Runtime Error",
                        &format!("Wi-Fi policy action failed: {err}"),
                    )
                    .await;
                    let _ = logger::append_line(format!(
                        "rust-daemon: wifi policy action failed (enabled={}): {}",
                        enabled, err
                    ));
                } else {
                    let _ = logger::append_line(format!(
                        "rust-daemon: applied wifi policy action -> {}",
                        enabled
                    ));
                }
            }
            PolicyAction::SetBluetooth(enabled) => {
                if let Err(err) = crate::runtime::policy::set_bluetooth_enabled(enabled) {
                    log::warn!("failed to set bluetooth policy action: {err}");
                    crate::runtime::daemon::notify_runtime_error(
                        &state,
                        "Zenbook Duo Runtime Error",
                        &format!("Bluetooth policy action failed: {err}"),
                    )
                    .await;
                    let _ = logger::append_line(format!(
                        "rust-daemon: bluetooth policy action failed (enabled={}): {}",
                        enabled, err
                    ));
                } else {
                    let _ = logger::append_line(format!(
                        "rust-daemon: applied bluetooth policy action -> {}",
                        enabled
                    ));
                }
            }
            PolicyAction::SetBacklight(level) => {
                if let Err(err) = crate::hardware::hid::set_backlight(level) {
                    log::warn!("failed to set backlight policy action: {err}");
                    crate::runtime::daemon::notify_runtime_error(
                        &state,
                        "Zenbook Duo Runtime Error",
                        &format!("Backlight policy action failed: {err}"),
                    )
                    .await;
                    let _ = logger::append_line(format!(
                        "rust-daemon: backlight policy action failed (level={}): {}",
                        level, err
                    ));
                } else {
                    {
                        let mut guard = state.write().await;
                        guard.status.backlight_level = level;
                        guard.recent_events.push(HardwareEvent::info(
                            EventCategory::Keyboard,
                            format!("Backlight set to {}", level),
                            "rust-daemon",
                        ));
                        guard.touch();
                        if let Err(err) = guard.save() {
                            log::warn!("failed to save backlight policy state: {err}");
                        }
                    }
                    let _ = logger::append_line(format!(
                        "rust-daemon: applied backlight policy action -> {}",
                        level
                    ));
                }
            }
            PolicyAction::SetDockMode { attached, scale } => {
                if let Err(err) = crate::runtime::daemon::forward_session_command(
                    &state,
                    SessionCommand::SetDockMode { attached, scale },
                )
                .await
                {
                    log::warn!("failed to apply dock-mode policy action: {err}");
                    crate::runtime::daemon::notify_runtime_error(
                        &state,
                        "Zenbook Duo Runtime Error",
                        &format!("Dock-mode policy action failed: {err}"),
                    )
                    .await;
                    let _ = logger::append_line(format!(
                        "rust-daemon: dock-mode policy action failed (attached={}, scale={}): {}",
                        attached, scale, err
                    ));
                } else {
                    let _ = logger::append_line(format!(
                        "rust-daemon: applied dock-mode policy action (attached={}, scale={})",
                        attached, scale
                    ));
                }
            }
        }
    }
}

fn push_status_events(
    state: &mut RuntimeState,
    old: &crate::models::DuoStatus,
    new: &crate::models::DuoStatus,
) {
    if old.keyboard_attached != new.keyboard_attached {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Usb,
            if new.keyboard_attached {
                "Keyboard attached"
            } else {
                "Keyboard detached"
            },
            "rust-daemon",
        ));
    }

    if old.connection_type != new.connection_type {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Keyboard,
            format!(
                "Connection type changed to {}",
                connection_label(&new.connection_type)
            ),
            "rust-daemon",
        ));
    }

    if old.wifi_enabled != new.wifi_enabled {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Network,
            if new.wifi_enabled {
                "Wi-Fi enabled"
            } else {
                "Wi-Fi disabled"
            },
            "rust-daemon",
        ));
    }

    if old.bluetooth_enabled != new.bluetooth_enabled {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Bluetooth,
            if new.bluetooth_enabled {
                "Bluetooth enabled"
            } else {
                "Bluetooth disabled"
            },
            "rust-daemon",
        ));
    }

    if old.monitor_count != new.monitor_count {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Display,
            format!("Monitor count changed to {}", new.monitor_count),
            "rust-daemon",
        ));
    }

    if old.orientation != new.orientation {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Rotation,
            format!(
                "Orientation changed to {}",
                orientation_label(&new.orientation)
            ),
            "rust-daemon",
        ));
    }

    if old.backlight_level != new.backlight_level {
        state.recent_events.push(HardwareEvent::info(
            EventCategory::Keyboard,
            format!("Backlight level changed to {}", new.backlight_level),
            "rust-daemon",
        ));
    }

    if state.recent_events.len() > 500 {
        let overflow = state.recent_events.len() - 500;
        state.recent_events.drain(0..overflow);
    }
}

fn connection_label(connection_type: &ConnectionType) -> &'static str {
    match connection_type {
        ConnectionType::Usb => "usb",
        ConnectionType::Bluetooth => "bluetooth",
        ConnectionType::None => "none",
    }
}

fn orientation_label(orientation: &crate::models::Orientation) -> &'static str {
    match orientation {
        crate::models::Orientation::Normal => "normal",
        crate::models::Orientation::Left => "left",
        crate::models::Orientation::Right => "right",
        crate::models::Orientation::Inverted => "inverted",
    }
}
