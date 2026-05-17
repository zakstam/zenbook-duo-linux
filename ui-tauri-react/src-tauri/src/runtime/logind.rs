use std::sync::Arc;

use tokio::sync::RwLock;

use crate::models::{EventCategory, HardwareEvent};
use crate::runtime::{logger, state::RuntimeState};

pub fn start(state: Arc<RwLock<RuntimeState>>) {
    tokio::spawn(async move {
        if let Err(err) = watch_logind(state.clone()).await {
            log::warn!("logind runtime watcher failed: {err}");
            crate::runtime::daemon::notify_runtime_error(
                &state,
                "Zenbook Duo Runtime Error",
                &format!("Logind watcher failed: {err}"),
            )
            .await;
            let _ = logger::append_line(format!("rust-daemon: logind watcher failed: {err}"));
        }
    });
}

async fn watch_logind(state: Arc<RwLock<RuntimeState>>) -> Result<(), zbus::Error> {
    let connection = zbus::Connection::system().await?;

    let manager = zbus::fdo::PropertiesProxy::builder(&connection)
        .destination("org.freedesktop.login1")?
        .path("/org/freedesktop/login1")?
        .build()
        .await?;

    let login_manager_interface = "org.freedesktop.login1.Manager"
        .try_into()
        .expect("login1 manager interface name should be valid");
    if let Ok(value) = manager.get(login_manager_interface, "LidClosed").await {
        if let Ok(lid_closed) = value.downcast_ref::<bool>() {
            sync_initial_lid_closed_state(&state, lid_closed).await;
        }
    }

    let mut stream = manager.receive_properties_changed().await?;

    use futures_util::StreamExt;
    while let Some(signal) = stream.next().await {
        let args = signal.args().ok();
        if let Some(args) = args {
            if args.changed_properties().contains_key("LockedHint") {
                let locked = args
                    .changed_properties()
                    .get("LockedHint")
                    .and_then(|value| value.downcast_ref::<bool>().ok());
                let message = match locked {
                    Some(true) => "Screen locked",
                    Some(false) => "Screen unlocked",
                    None => "Screen lock state changed",
                };

                let mut guard = state.write().await;
                guard.push_recent_event(HardwareEvent::info(
                    EventCategory::Display,
                    message,
                    "rust-daemon",
                ));
                guard.touch();
                if let Err(err) = guard.save() {
                    log::warn!("failed to save logind runtime state: {err}");
                    let _ = logger::append_line(format!(
                        "rust-daemon: failed to persist logind state: {err}"
                    ));
                }
                let _ = logger::append_line(format!("rust-daemon: {}", message));
            }

            if let Some(lid_closed) = args
                .changed_properties()
                .get("LidClosed")
                .and_then(|value| value.downcast_ref::<bool>().ok())
            {
                if let Err(err) =
                    crate::runtime::daemon::handle_lid_closed_change(&state, lid_closed).await
                {
                    if crate::runtime::daemon::is_display_session_deferral(&err) {
                        let _ = logger::append_line(format!(
                            "rust-daemon: lid state display update deferred: {err}"
                        ));
                        continue;
                    }

                    log::warn!("failed to handle lid state change: {err}");
                    crate::runtime::daemon::notify_runtime_error(
                        &state,
                        "Zenbook Duo Runtime Error",
                        &format!("Lid state display update failed: {err}"),
                    )
                    .await;
                    let _ = logger::append_line(format!(
                        "rust-daemon: lid state display update failed: {err}"
                    ));
                }
            }
        }
    }

    Ok(())
}

async fn sync_initial_lid_closed_state(state: &Arc<RwLock<RuntimeState>>, lid_closed: bool) {
    if let Err(err) = crate::runtime::daemon::handle_lid_closed_change(state, lid_closed).await {
        if crate::runtime::daemon::is_display_session_deferral(&err) {
            let _ = logger::append_line(format!(
                "rust-daemon: initial lid state display update deferred: {err}"
            ));
            return;
        }

        log::warn!("failed to sync initial lid state: {err}");
        crate::runtime::daemon::notify_runtime_error(
            state,
            "Zenbook Duo Runtime Error",
            &format!("Initial lid state display update failed: {err}"),
        )
        .await;
        let _ = logger::append_line(format!(
            "rust-daemon: initial lid state display update failed: {err}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn initial_lid_closed_true_records_runtime_state_even_when_display_defers() {
        let state = Arc::new(RwLock::new(RuntimeState::default()));

        sync_initial_lid_closed_state(&state, true).await;

        assert!(state.read().await.lid_closed);
    }
}
