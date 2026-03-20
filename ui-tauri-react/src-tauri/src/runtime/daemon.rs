use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

use crate::ipc::protocol::{
    DaemonRequest, DaemonResponse, Envelope, LifecyclePhase, PROTOCOL_VERSION, SessionCommand,
    SessionResponse,
};
use crate::runtime::{logger, paths, state::RuntimeState};
use crate::{
    commands,
    hardware,
    models::{DisplayLayout, EventCategory, HardwareEvent, Orientation},
};

pub async fn run() -> Result<(), String> {
    crate::runtime::runtime_dir::ensure_target_user_runtime_dir()?;
    ensure_parent(paths::daemon_socket_path().as_path())
        .map_err(|e| format!("Failed to prepare daemon runtime dir: {e}"))?;
    remove_stale_socket(paths::daemon_socket_path().as_path());

    let listener = UnixListener::bind(paths::daemon_socket_path())
        .map_err(|e| format!("Failed to bind daemon socket: {e}"))?;
    configure_daemon_socket(paths::daemon_socket_path().as_path())
        .map_err(|e| format!("Failed to configure daemon socket: {e}"))?;
    let state = Arc::new(RwLock::new(initialize_state()));
    crate::runtime::monitor::start(state.clone());
    crate::runtime::logind::start(state.clone());

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("Failed to accept daemon client: {e}"))?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, state).await {
                log::warn!("daemon client error: {err}");
            }
        });
    }
}

fn configure_daemon_socket(path: &Path) -> Result<(), String> {
    let uid = std::env::var("ZENBOOK_DUO_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
    let gid = std::env::var("ZENBOOK_DUO_GID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());

    if let (Some(uid), Some(gid)) = (uid, gid) {
        let path_cstr = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| format!("Socket path contains interior NUL: {}", path.display()))?;
        let result = unsafe { libc::chown(path_cstr.as_ptr(), uid, gid) };
        if result != 0 {
            return Err(format!(
                "chown({}, {}, {}) failed: {}",
                path.display(),
                uid,
                gid,
                io::Error::last_os_error()
            ));
        }
    }

    fs::set_permissions(path, fs::Permissions::from_mode(0o660))
        .map_err(|e| format!("chmod {} failed: {e}", path.display()))?;

    Ok(())
}

fn initialize_state() -> RuntimeState {
    let mut state = RuntimeState::load();
    state.status = crate::runtime::probe::current_status();
    state.status.service_active = false;
    state.settings = commands::settings::load_settings_local();
    state.session_agent = Default::default();
    state.touch();
    persist_state(&state);
    let _ = logger::append_line("rust-daemon: initialized runtime state");
    state
}

async fn handle_client(stream: UnixStream, state: Arc<RwLock<RuntimeState>>) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed to read daemon request: {e}"))?
    {
        let envelope: Envelope<DaemonRequest> =
            serde_json::from_str(&line).map_err(|e| format!("Invalid daemon request JSON: {e}"))?;

        if envelope.protocol_version != PROTOCOL_VERSION {
            write_response(
                &mut writer,
                DaemonResponse::Error {
                    message: format!(
                        "Protocol mismatch: expected {}, got {}",
                        PROTOCOL_VERSION, envelope.protocol_version
                    ),
                },
            )
            .await?;
            continue;
        }

        let response = match envelope.payload {
            DaemonRequest::Ping => DaemonResponse::Pong,
            DaemonRequest::HandleLifecycle { phase } => match handle_lifecycle(&state, phase).await {
                Ok(()) => DaemonResponse::Ack,
                Err(message) => DaemonResponse::Error { message },
            },
            DaemonRequest::GetStatus => {
                let guard = state.read().await;
                let mut status = guard.status.clone();
                status.service_active = guard.session_agent.connected;
                DaemonResponse::Status {
                    status,
                }
            }
            DaemonRequest::GetDisplayLayout => {
                match request_session(state.clone(), SessionCommand::GetDisplayLayout).await {
                    Ok(SessionResponse::DisplayLayout { layout }) => {
                        DaemonResponse::DisplayLayout { layout }
                    }
                    Ok(SessionResponse::Error { message }) => DaemonResponse::Error { message },
                    Ok(_) => DaemonResponse::Error {
                        message: "Unexpected session-agent response while reading display layout"
                            .into(),
                    },
                    Err(_) => match hardware::display_config::get_display_layout() {
                        Ok(layout) => DaemonResponse::DisplayLayout { layout },
                        Err(message) => DaemonResponse::Error { message },
                    },
                }
            }
            DaemonRequest::GetSettings => {
                let guard = state.read().await;
                DaemonResponse::Settings {
                    settings: guard.settings.clone(),
                }
            }
            DaemonRequest::UsbMediaRemapStatus => DaemonResponse::UsbMediaRemapStatus {
                status: commands::usb_media_remap::get_status(),
            },
            DaemonRequest::SaveSettings { settings } => {
                let mut guard = state.write().await;
                guard.settings = settings;
                guard.touch();
                persist_state(&guard);
                DaemonResponse::Ack
            }
            DaemonRequest::SetBacklight { level } => {
                match hardware::hid::set_backlight(level) {
                    Ok(()) => {
                        let mut guard = state.write().await;
                        guard.status.backlight_level = level;
                        let _ = logger::append_line(format!("rust-daemon: set backlight request -> {level}"));
                        guard.recent_events.push(HardwareEvent::info(
                            EventCategory::Keyboard,
                            format!("Backlight set to {level}"),
                            "rust-daemon",
                        ));
                        guard.touch();
                        persist_state(&guard);
                        DaemonResponse::Ack
                    }
                    Err(message) => DaemonResponse::Error { message },
                }
            }
            DaemonRequest::SetOrientation { orientation } => {
                apply_orientation(&state, orientation).await
            }
            DaemonRequest::ApplyDisplayLayout { layout } => {
                apply_display_layout_request(&state, layout).await
            }
            DaemonRequest::UsbMediaRemapStart => {
                let _ = logger::append_line("rust-daemon: start usb media remap request");
                match commands::usb_media_remap::start_remap() {
                    Ok(()) => DaemonResponse::Ack,
                    Err(message) => DaemonResponse::Error { message },
                }
            }
            DaemonRequest::UsbMediaRemapStop => {
                let _ = logger::append_line("rust-daemon: stop usb media remap request");
                match commands::usb_media_remap::stop_remap() {
                    Ok(()) => DaemonResponse::Ack,
                    Err(message) => DaemonResponse::Error { message },
                }
            }
            DaemonRequest::UsbMediaRemapTogglePause => {
                let _ = logger::append_line("rust-daemon: toggle usb media remap pause request");
                match commands::usb_media_remap::toggle_pause() {
                    Ok(()) => DaemonResponse::Ack,
                    Err(message) => DaemonResponse::Error { message },
                }
            }
            DaemonRequest::RestartService => DaemonResponse::Error {
                message: "Service restart not yet owned by rust-daemon".into(),
            },
            DaemonRequest::RegisterSessionAgent {
                session_id,
                backend,
                socket_path,
            } => {
                let mut guard = state.write().await;
                guard.session_agent.connected = true;
                guard.session_agent.session_id = Some(session_id);
                guard.session_agent.backend = Some(backend);
                guard.session_agent.socket_path = Some(socket_path);
                guard.status.service_active = true;
                let _ = logger::append_line(format!(
                    "rust-daemon: session agent registered ({:?})",
                    guard.session_agent.backend
                ));
                guard.touch();
                persist_state(&guard);
                DaemonResponse::Ack
            }
            DaemonRequest::TailLogs { lines } => DaemonResponse::Logs {
                lines: hardware::sysfs::read_log_lines(lines),
            },
            DaemonRequest::GetRecentEvents { limit } => {
                let guard = state.read().await;
                let events = guard
                    .recent_events
                    .iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                DaemonResponse::Events { events }
            }
        };

        write_response(&mut writer, response).await?;
    }

    Ok(())
}

async fn write_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: DaemonResponse,
) -> Result<(), String> {
    let line = serde_json::to_string(&Envelope::new(response))
        .map_err(|e| format!("Failed to encode daemon response: {e}"))?;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("Failed to write daemon response: {e}"))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|e| format!("Failed to terminate daemon response: {e}"))
}

fn ensure_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn remove_stale_socket(path: &Path) {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_socket() {
            let _ = fs::remove_file(path);
        }
    }
}

fn persist_state(state: &RuntimeState) {
    if let Err(err) = state.save() {
        log::warn!("failed to persist runtime state: {err}");
        let _ = logger::append_line(format!("rust-daemon: failed to persist state: {err}"));
    }
}

async fn handle_lifecycle(
    state: &Arc<RwLock<RuntimeState>>,
    phase: LifecyclePhase,
) -> Result<(), String> {
    match phase {
        LifecyclePhase::Pre | LifecyclePhase::Hibernate | LifecyclePhase::Shutdown => {
            logger::append_line(format!("rust-daemon: lifecycle -> {:?}", phase)).ok();
            hardware::hid::set_backlight(0)?;

            let mut guard = state.write().await;
            guard.recent_events.push(HardwareEvent::info(
                EventCategory::Service,
                format!("Lifecycle event: {:?}", phase),
                "rust-daemon",
            ));
            guard.touch();
            persist_state(&guard);
            Ok(())
        }
        LifecyclePhase::Post | LifecyclePhase::Thaw | LifecyclePhase::Boot => {
            logger::append_line(format!("rust-daemon: lifecycle -> {:?}", phase)).ok();

            let (restore_level, scale) = {
                let guard = state.read().await;
                (guard.status.backlight_level, guard.settings.default_scale)
            };
            hardware::hid::set_backlight(restore_level)?;

            let refreshed = crate::runtime::probe::current_status();
            let attached = refreshed.keyboard_attached;

            {
                let mut guard = state.write().await;
                let previous_level = guard.status.backlight_level;
                guard.status = refreshed;
                guard.status.backlight_level = previous_level;
                guard.status.service_active = guard.session_agent.connected;
                guard.recent_events.push(HardwareEvent::info(
                    EventCategory::Service,
                    format!("Lifecycle event: {:?}", phase),
                    "rust-daemon",
                ));
                guard.touch();
                persist_state(&guard);
            }

            match forward_session_command(
                state,
                SessionCommand::SetDockMode { attached, scale },
            )
            .await
            {
                Ok(()) => Ok(()),
                Err(err) => {
                    logger::append_line(format!(
                        "rust-daemon: lifecycle dock refresh skipped: {}",
                        err
                    ))
                    .ok();
                    Ok(())
                }
            }
        }
    }
}

pub(crate) async fn forward_session_command(
    state: &Arc<RwLock<RuntimeState>>,
    command: SessionCommand,
) -> Result<(), String> {
    match request_session(state.clone(), command).await? {
        SessionResponse::Ack => Ok(()),
        SessionResponse::Error { message } => Err(message),
        SessionResponse::DisplayLayout { .. } => {
            Err("Unexpected display-layout response for command request".into())
        }
    }
}

pub(crate) async fn session_display_layout(
    state: Arc<RwLock<RuntimeState>>,
) -> Option<DisplayLayout> {
    match request_session(state, SessionCommand::GetDisplayLayout).await {
        Ok(SessionResponse::DisplayLayout { layout }) => Some(layout),
        _ => None,
    }
}

async fn apply_orientation(
    state: &Arc<RwLock<RuntimeState>>,
    orientation: Orientation,
) -> DaemonResponse {
    match forward_session_command(
        state,
        SessionCommand::SetOrientation {
            orientation: orientation.clone(),
        },
    )
    .await
    {
        Ok(()) => {
            let mut guard = state.write().await;
            guard.status.orientation = orientation;
            let _ = logger::append_line(format!(
                "rust-daemon: applied orientation -> {:?}",
                guard.status.orientation
            ));
            guard.touch();
            persist_state(&guard);
            DaemonResponse::Ack
        }
        Err(message) => DaemonResponse::Error { message },
    }
}

async fn apply_display_layout_request(
    state: &Arc<RwLock<RuntimeState>>,
    layout: DisplayLayout,
) -> DaemonResponse {
    match forward_session_command(
        state,
        SessionCommand::ApplyDisplayLayout {
            layout: layout.clone(),
        },
    )
    .await
    {
        Ok(()) => {
            let mut guard = state.write().await;
            crate::runtime::probe::apply_layout_to_status(&mut guard.status, Some(&layout));
            let _ = logger::append_line(format!(
                "rust-daemon: applied display layout with {} displays",
                guard.status.monitor_count
            ));
            guard.touch();
            persist_state(&guard);
            DaemonResponse::Ack
        }
        Err(message) => DaemonResponse::Error { message },
    }
}

async fn request_session(
    state: Arc<RwLock<RuntimeState>>,
    command: SessionCommand,
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
            mark_session_agent_disconnected(
                &state,
                &format!("Failed to connect to session agent: {e}"),
            )
            .await;
            return Err(format!("Failed to connect to session agent: {e}"));
        }
        Err(_) => {
            mark_session_agent_disconnected(&state, "Timed out connecting to session agent").await;
            return Err("Timed out connecting to session agent".to_string());
        }
    };
    let (reader, mut writer) = stream.into_split();

    let line = serde_json::to_string(&Envelope::new(command))
        .map_err(|e| format!("Failed to encode session command: {e}"))?;
    if let Err(e) = timeout(Duration::from_secs(3), writer.write_all(line.as_bytes()))
        .await
        .map_err(|_| "Timed out writing session command".to_string())?
    {
        mark_session_agent_disconnected(
            &state,
            &format!("Failed to write session command: {e}"),
        )
        .await;
        return Err(format!("Failed to write session command: {e}"));
    }
    if let Err(e) = timeout(Duration::from_secs(3), writer.write_all(b"\n"))
        .await
        .map_err(|_| "Timed out terminating session command".to_string())?
    {
        mark_session_agent_disconnected(
            &state,
            &format!("Failed to terminate session command: {e}"),
        )
        .await;
        return Err(format!("Failed to terminate session command: {e}"));
    }

    let mut lines = BufReader::new(reader).lines();
    let reply = match timeout(Duration::from_secs(3), lines.next_line()).await {
        Err(_) => {
            mark_session_agent_disconnected(
                &state,
                "Timed out waiting for session response",
            )
            .await;
            return Err("Timed out waiting for session response".to_string());
        }
        Ok(Ok(Some(reply))) => reply,
        Ok(Ok(None)) => {
            mark_session_agent_disconnected(
                &state,
                "Session agent closed before replying",
            )
            .await;
            return Err("Session agent closed before replying".to_string());
        }
        Ok(Err(e)) => {
            mark_session_agent_disconnected(
                &state,
                &format!("Failed to read session response: {e}"),
            )
            .await;
            return Err(format!("Failed to read session response: {e}"));
        }
    };

    let envelope: Envelope<SessionResponse> = serde_json::from_str(&reply)
        .map_err(|e| format!("Invalid session response JSON: {e}"))?;

    Ok(envelope.payload)
}

async fn mark_session_agent_disconnected(
    state: &Arc<RwLock<RuntimeState>>,
    reason: &str,
) {
    let mut guard = state.write().await;
    let was_connected = guard.session_agent.connected || guard.status.service_active;
    guard.session_agent = Default::default();
    guard.status.service_active = false;
    if was_connected {
        guard.recent_events.push(HardwareEvent::warning(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn apply_orientation_does_not_deadlock_and_updates_state() {
        let socket_path = unique_test_socket_path("orientation");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept test session client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read test request")
                .expect("session request line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode session request");
            match envelope.payload {
                SessionCommand::SetOrientation { orientation } => {
                    assert_eq!(orientation, Orientation::Left);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::Ack))
                .expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
        }

        let response = timeout(
            Duration::from_secs(1),
            apply_orientation(&state, Orientation::Left),
        )
        .await
        .expect("orientation request should not hang");

        assert!(matches!(response, DaemonResponse::Ack));
        assert_eq!(state.read().await.status.orientation, Orientation::Left);

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    fn unique_test_socket_path(label: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("zenbook-duo-{label}-{nanos}-{id}.sock"))
    }
}
