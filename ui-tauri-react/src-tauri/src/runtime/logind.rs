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
                guard.recent_events.push(HardwareEvent::info(
                    EventCategory::Display,
                    message,
                    "rust-daemon",
                ));
                if guard.recent_events.len() > 500 {
                    let overflow = guard.recent_events.len() - 500;
                    guard.recent_events.drain(0..overflow);
                }
                guard.touch();
                if let Err(err) = guard.save() {
                    log::warn!("failed to save logind runtime state: {err}");
                    let _ = logger::append_line(format!(
                        "rust-daemon: failed to persist logind state: {err}"
                    ));
                }
                let _ = logger::append_line(format!("rust-daemon: {}", message));
            }
        }
    }

    Ok(())
}
