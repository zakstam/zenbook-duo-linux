use super::*;

pub(crate) struct NotificationSink;

impl NotificationSink {
    pub(crate) async fn runtime_error(state: &Arc<RwLock<RuntimeState>>, title: &str, message: &str) {
        notify_runtime_error(state, title, message).await;
    }
}

pub(crate) async fn notify_runtime_error(
    state: &Arc<RwLock<RuntimeState>>,
    title: &str,
    message: &str,
) {
    if !should_notify_runtime_error(title, message) {
        return;
    }

    if !should_emit_runtime_notification(state, title, message).await {
        return;
    }

    if let Err(err) = send_runtime_notification_via_session(state, title, message).await {
        log::warn!("failed to send runtime notification via session agent: {err}");
        if let Err(fallback_err) = send_runtime_notification_direct(title, message) {
            log::warn!("failed to send runtime notification directly: {fallback_err}");
        }
    }
}

pub(super) fn should_notify_runtime_error(title: &str, message: &str) -> bool {
    if title != "Zenbook Duo Runtime Error" {
        return true;
    }

    !runtime_error_is_display_session_deferral(message)
}

fn runtime_error_is_display_session_deferral(message: &str) -> bool {
    [
        "Lifecycle dock refresh skipped: ",
        "Display-mode policy action failed: ",
        "Session registration dock replay skipped: ",
        "Lid state display update failed: ",
    ]
    .iter()
    .any(|prefix| {
        message
            .strip_prefix(prefix)
            .is_some_and(is_display_session_deferral)
    })
}

async fn should_emit_runtime_notification(
    state: &Arc<RwLock<RuntimeState>>,
    title: &str,
    message: &str,
) -> bool {
    let key = format!("{title}\n{message}");
    let now = Utc::now();
    let mut guard = state.write().await;
    if let Some(last) = &guard.last_runtime_notification {
        if last.key == key && now - last.emitted_at < ChronoDuration::seconds(30) {
            return false;
        }
    }
    guard.last_runtime_notification = Some(crate::runtime::state::RuntimeNotificationState {
        key,
        emitted_at: now,
    });
    persist_state(&guard);
    true
}

async fn send_runtime_notification_via_session(
    state: &Arc<RwLock<RuntimeState>>,
    title: &str,
    message: &str,
) -> Result<(), String> {
    match session_bridge::request_session(
        state.clone(),
        SessionCommand::ShowNotification {
            title: title.to_string(),
            message: message.to_string(),
            urgent: true,
        },
        false,
    )
    .await?
    {
        SessionResponse::Ack => Ok(()),
        SessionResponse::Error { message } => Err(message),
        SessionResponse::DisplayLayout { .. } => {
            Err("Unexpected display-layout response for notification request".into())
        }
    }
}

pub(super) fn send_runtime_notification_direct(title: &str, message: &str) -> Result<(), String> {
    let target_user = std::env::var("ZENBOOK_DUO_USER")
        .map_err(|_| "ZENBOOK_DUO_USER is not set for runtime notifications".to_string())?;
    let target_uid = std::env::var("ZENBOOK_DUO_UID")
        .map_err(|_| "ZENBOOK_DUO_UID is not set for runtime notifications".to_string())?;
    let runtime_dir = format!("/run/user/{target_uid}");
    let bus_address = format!("unix:path={runtime_dir}/bus");

    let status = Command::new("sudo")
        .args(["-u", &target_user, "env"])
        .arg(format!("XDG_RUNTIME_DIR={runtime_dir}"))
        .arg(format!("DBUS_SESSION_BUS_ADDRESS={bus_address}"))
        .args([
            "notify-send",
            "-a",
            "Zenbook Duo Control",
            "-u",
            "critical",
            "-i",
            "dialog-error",
            title,
            message,
        ])
        .status()
        .map_err(|e| format!("Failed to launch runtime notification: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Runtime notification exited with {status}"))
    }
}

