use super::*;

pub(crate) struct SessionBridge;

impl SessionBridge {
    pub(crate) async fn request(
        state: Arc<RwLock<RuntimeState>>,
        command: SessionCommand,
        disconnect_on_failure: bool,
    ) -> Result<SessionResponse, String> {
        request_session(state, command, disconnect_on_failure).await
    }
}

pub(super) async fn request_session(
    state: Arc<RwLock<RuntimeState>>,
    command: SessionCommand,
    disconnect_on_failure: bool,
) -> Result<SessionResponse, String> {
    let socket_path = {
        let guard = state.read().await;
        guard
            .session_agent
            .socket_path
            .clone()
            .ok_or_else(|| "No session agent registered".to_string())?
    };

    let stream = match timeout(Duration::from_secs(3), UnixStream::connect(&socket_path)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(
                    &state,
                    &format!("Failed to connect to session agent: {e}"),
                )
                .await;
            }
            return Err(format!("Failed to connect to session agent: {e}"));
        }
        Err(_) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(&state, "Timed out connecting to session agent")
                    .await;
            }
            return Err("Timed out connecting to session agent".to_string());
        }
    };
    let (reader, mut writer) = stream.into_split();

    let line = serde_json::to_string(&Envelope::new(command))
        .map_err(|e| format!("Failed to encode session command: {e}"))?;
    match timeout(Duration::from_secs(3), writer.write_all(line.as_bytes())).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(
                    &state,
                    &format!("Failed to write session command: {e}"),
                )
                .await;
            }
            return Err(format!("Failed to write session command: {e}"));
        }
        Err(_) => {
            return handle_session_write_timeout(
                &state,
                "Timed out writing session command",
                disconnect_on_failure,
            )
            .await;
        }
    }
    match timeout(Duration::from_secs(3), writer.write_all(b"\n")).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(
                    &state,
                    &format!("Failed to terminate session command: {e}"),
                )
                .await;
            }
            return Err(format!("Failed to terminate session command: {e}"));
        }
        Err(_) => {
            return handle_session_write_timeout(
                &state,
                "Timed out terminating session command",
                disconnect_on_failure,
            )
            .await;
        }
    }

    let mut lines = BufReader::new(reader).lines();
    let reply = match timeout(Duration::from_secs(3), lines.next_line()).await {
        Err(_) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(&state, "Timed out waiting for session response")
                    .await;
            }
            return Err("Timed out waiting for session response".to_string());
        }
        Ok(Ok(Some(reply))) => reply,
        Ok(Ok(None)) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(&state, "Session agent closed before replying")
                    .await;
            }
            return Err("Session agent closed before replying".to_string());
        }
        Ok(Err(e)) => {
            if disconnect_on_failure {
                mark_session_agent_disconnected(
                    &state,
                    &format!("Failed to read session response: {e}"),
                )
                .await;
            }
            return Err(format!("Failed to read session response: {e}"));
        }
    };

    let envelope: Envelope<SessionResponse> =
        serde_json::from_str(&reply).map_err(|e| format!("Invalid session response JSON: {e}"))?;

    Ok(envelope.payload)
}

pub(super) async fn handle_session_write_timeout<T>(
    state: &Arc<RwLock<RuntimeState>>,
    reason: &str,
    disconnect_on_failure: bool,
) -> Result<T, String> {
    if disconnect_on_failure {
        mark_session_agent_disconnected(state, reason).await;
    }
    Err(reason.to_string())
}

async fn mark_session_agent_disconnected(state: &Arc<RwLock<RuntimeState>>, reason: &str) {
    let mut guard = state.write().await;
    let was_connected = guard.session_agent.connected || guard.status.service_active;
    guard.session_agent = Default::default();
    guard.status.service_active = false;
    if was_connected {
        guard.push_recent_event(HardwareEvent::warning(
            EventCategory::Service,
            format!("Session agent disconnected: {reason}"),
            "rust-daemon",
        ));
    }
    guard.touch();
    persist_state(&guard);
    let _ = logger::append_line(format!(
        "rust-daemon: session agent disconnected: {}",
        reason
    ));
    drop(guard);

    if was_connected {
        if let Err(err) = ServiceController::queue_target_user_unit_restart("zenbook-duo-session-agent.service") {
            log::warn!("failed to queue session agent restart: {err}");
            let _ = logger::append_line(format!(
                "rust-daemon: failed to queue session agent restart: {}",
                err
            ));
        } else {
            let _ = logger::append_line("rust-daemon: queued session agent restart");
        }
    }

    if should_notify_session_agent_disconnect(reason) {
        if let Err(err) = notification_sink::send_runtime_notification_direct(
            "Zenbook Duo Runtime Error",
            &format!("Session agent disconnected: {reason}"),
        ) {
            log::warn!("failed to send direct disconnect notification: {err}");
        }
    }
}

pub(super) fn should_notify_session_agent_disconnect(reason: &str) -> bool {
    !is_display_session_deferral(reason)
}

