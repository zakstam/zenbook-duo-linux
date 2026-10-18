//! The detachable keyboard's battery level, read from UPower (which gets it from
//! BlueZ while the keyboard is connected over Bluetooth). Cached for the status
//! probe, and used for low-battery notifications while the keyboard is detached.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::ipc::protocol::SessionCommand;
use crate::models::ConnectionType;
use crate::runtime::{logger, state::RuntimeState};

const KEYBOARD_MODEL: &str = "Zenbook Duo Keyboard";
const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// Levels for the low and critical notifications.
pub(crate) const LOW_LEVEL: u8 = 20;
pub(crate) const CRITICAL_LEVEL: u8 = 10;
/// Above this the warnings are armed again.
const REARM_LEVEL: u8 = 25;
/// Cached level; UNKNOWN when the keyboard's battery is not reported.
const UNKNOWN: u8 = u8::MAX;
static CACHED_LEVEL: AtomicU8 = AtomicU8::new(UNKNOWN);

/// The last battery level read, if the keyboard reports one.
pub(crate) fn cached_level() -> Option<u8> {
    match CACHED_LEVEL.load(Ordering::Relaxed) {
        UNKNOWN => None,
        level => Some(level),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatteryWarning {
    Low,
    Critical,
}

/// Which warnings were already shown during the current discharge.
#[derive(Debug, Default)]
pub(crate) struct WarningState {
    previous: Option<u8>,
    low_shown: bool,
    critical_shown: bool,
}

impl WarningState {
    /// Decides whether a new reading should notify. Only while the keyboard is
    /// detached and not charging: the keyboard does not report charging, so a
    /// rising level (e.g. on its own cable) counts as charging.
    pub(crate) fn update(&mut self, level: u8, detached: bool) -> Option<BatteryWarning> {
        let rising = self.previous.is_some_and(|previous| level > previous);
        self.previous = Some(level);
        if !detached || rising || level > REARM_LEVEL {
            self.low_shown = false;
            self.critical_shown = false;
            return None;
        }
        if level <= CRITICAL_LEVEL && !self.critical_shown {
            self.critical_shown = true;
            self.low_shown = true;
            return Some(BatteryWarning::Critical);
        }
        if level <= LOW_LEVEL && !self.low_shown {
            self.low_shown = true;
            return Some(BatteryWarning::Low);
        }
        None
    }
}

pub(crate) fn start(state: Arc<RwLock<RuntimeState>>) {
    tokio::spawn(async move {
        let mut warnings = WarningState::default();
        loop {
            match read_level().await {
                Ok(level) => {
                    CACHED_LEVEL.store(level.unwrap_or(UNKNOWN), Ordering::Relaxed);
                    if let Some(level) = level {
                        let detached = matches!(
                            state.read().await.status.connection_type,
                            ConnectionType::Bluetooth
                        );
                        if let Some(warning) = warnings.update(level, detached) {
                            notify(&state, warning, level).await;
                        }
                    }
                }
                Err(err) => {
                    let _ = logger::append_line(format!(
                        "rust-daemon: keyboard battery read failed: {err}"
                    ));
                }
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

/// The keyboard's battery percentage from UPower, or None if it is not listed.
async fn read_level() -> Result<Option<u8>, zbus::Error> {
    let connection = zbus::Connection::system().await?;
    let upower = zbus::Proxy::new(
        &connection,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower",
        "org.freedesktop.UPower",
    )
    .await?;
    let devices: Vec<zbus::zvariant::OwnedObjectPath> =
        upower.call("EnumerateDevices", &()).await?;
    for path in devices {
        let device = zbus::Proxy::new(
            &connection,
            "org.freedesktop.UPower",
            path,
            "org.freedesktop.UPower.Device",
        )
        .await?;
        let model: String = device.get_property("Model").await.unwrap_or_default();
        if !model.contains(KEYBOARD_MODEL) {
            continue;
        }
        let percentage: f64 = device.get_property("Percentage").await?;
        return Ok(Some(percentage.round().clamp(0.0, 100.0) as u8));
    }
    Ok(None)
}

/// The notification's title and text for a warning at this level.
pub(crate) fn warning_text(level: u8, critical: bool) -> (&'static str, String) {
    let title = if critical {
        "Keyboard battery critically low"
    } else {
        "Keyboard battery low"
    };
    let message = format!(
        "The keyboard is at {level}%. Attach it or plug it in to charge."
    );
    (title, message)
}

/// Shows the warning in the desktop session: a bubble through the GNOME
/// extension that stays until acknowledged, or a notification without it.
pub(crate) fn show_warning(level: u8, critical: bool) -> Result<(), String> {
    let icon = if critical {
        "battery-caution-symbolic"
    } else {
        "battery-level-20-symbolic"
    };
    let (title, message) = warning_text(level, critical);
    if crate::runtime::osd::show_alert_in_session("keyboard-battery", icon, title, &message) {
        return Ok(());
    }
    crate::runtime::session_agent::send_runtime_notification(title, &message, critical)
}

async fn notify(state: &Arc<RwLock<RuntimeState>>, warning: BatteryWarning, level: u8) {
    let result = crate::runtime::daemon::forward_session_command(
        state,
        SessionCommand::KeyboardBatteryWarning {
            level,
            critical: warning == BatteryWarning::Critical,
        },
    )
    .await;
    let _ = logger::append_line(match result {
        Ok(()) => format!("rust-daemon: keyboard battery warning at {level}%"),
        Err(err) => format!("rust-daemon: keyboard battery warning failed: {err}"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_once_at_low_and_once_at_critical_while_discharging() {
        let mut state = WarningState::default();
        assert_eq!(state.update(30, true), None);
        assert_eq!(state.update(20, true), Some(BatteryWarning::Low));
        assert_eq!(state.update(15, true), None);
        assert_eq!(state.update(10, true), Some(BatteryWarning::Critical));
        assert_eq!(state.update(5, true), None);
    }

    #[test]
    fn a_first_reading_below_critical_warns_critically() {
        let mut state = WarningState::default();
        assert_eq!(state.update(8, true), Some(BatteryWarning::Critical));
        assert_eq!(state.update(7, true), None);
    }

    #[test]
    fn no_warnings_while_attached_or_charging() {
        let mut state = WarningState::default();
        assert_eq!(state.update(15, false), None);
        // Detached but rising (charging on its own cable).
        assert_eq!(state.update(1, true), Some(BatteryWarning::Critical));
        assert_eq!(state.update(4, true), None);
        assert_eq!(state.update(10, true), None);
    }

    #[test]
    fn warnings_are_armed_again_after_charging() {
        let mut state = WarningState::default();
        assert_eq!(state.update(18, true), Some(BatteryWarning::Low));
        assert_eq!(state.update(30, false), None);
        assert_eq!(state.update(19, true), Some(BatteryWarning::Low));
    }
}
