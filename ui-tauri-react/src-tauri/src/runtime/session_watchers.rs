use crate::runtime::session_agent::{
    brightness_sync::BrightnessSync, hotkeys::HotkeyWatcher, rotation::RotationWatcherSupervisor,
    send_runtime_notification,
};

/// Supervises session-agent watcher lifecycles.
///
/// The concrete watchers still own their OS-specific implementations; this
/// Module owns when and how they are spawned and how failures are surfaced.
pub(crate) fn start_all() {
    tokio::spawn(RotationWatcherSupervisor::supervise());
    tokio::spawn(async {
        if let Err(err) = BrightnessSync::watch().await {
            log::warn!("session-agent brightness watcher failed: {err}");
            let _ = send_runtime_notification(
                "Zenbook Duo Runtime Error",
                &format!("Brightness sync watcher failed: {err}"),
                true,
            );
        }
    });
    tokio::task::spawn_blocking(|| {
        if let Err(err) = HotkeyWatcher::watch() {
            log::warn!("session-agent keyboard hotkey watcher failed: {err}");
            let _ = send_runtime_notification(
                "Zenbook Duo Runtime Error",
                &format!("Keyboard hotkey watcher failed: {err}"),
                true,
            );
        }
    });
}
