use crate::ipc::protocol::SessionBackend;
use crate::runtime::session_agent::{
    backlight_idle, brightness_sync::BrightnessSync, mic_mute, rotation::RotationWatcherSupervisor,
    send_runtime_notification,
};

/// Supervises session-agent watcher lifecycles.
///
/// The concrete watchers still own their OS-specific implementations; this
/// Module owns when and how they are spawned and how failures are surfaced.
pub(crate) fn start_all() {
    tokio::spawn(RotationWatcherSupervisor::supervise());
    tokio::spawn(async {
        if let Err(err) = mic_mute::watch().await {
            log::warn!("session-agent microphone watcher failed: {err}");
        }
    });
    if crate::runtime::session::detect_backend_from_env() == SessionBackend::Gnome {
        tokio::spawn(async {
            if let Err(err) = backlight_idle::watch().await {
                log::warn!("session-agent keyboard backlight sleep failed: {err}");
            }
        });
        tokio::spawn(async {
            if let Err(err) = backlight_idle::watch_screen_power().await {
                log::warn!("session-agent screen power watcher failed: {err}");
            }
        });
    }
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
}
