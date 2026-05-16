use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};
use nix::unistd::{Gid, Uid};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

use crate::ipc::protocol::{
    DaemonRequest, DaemonResponse, Envelope, LifecyclePhase, SessionCommand, SessionResponse,
    PROTOCOL_VERSION,
};
use crate::models::DaemonVersionInfo;
use crate::runtime::{logger, paths, state::RuntimeState};
use crate::{
    commands, hardware,
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

pub(crate) struct ServiceManager;

impl ServiceManager {
    fn restart_owned_services() -> Result<(), String> {
        restart_owned_services()
    }
}

fn restart_owned_services() -> Result<(), String> {
    let mut errors = Vec::new();

    if let Err(message) = restart_target_user_unit("zenbook-duo-session-agent.service") {
        errors.push(message);
    }

    if let Err(message) = queue_system_unit_restart("zenbook-duo-rust-daemon.service") {
        errors.push(message);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn restart_target_user_unit(unit: &str) -> Result<(), String> {
    let mut command = target_user_systemctl_command();
    command.args(["--user", "restart", unit]);

    let output = command
        .output()
        .map_err(|e| format!("Failed to restart {unit}: {e}"))?;

    if output.status.success() || unit_not_found(&output) {
        Ok(())
    } else {
        Err(format!(
            "Failed to restart {unit}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn target_user_systemctl_command() -> Command {
    let uid = target_uid();
    let gid = target_gid();
    let mut command = Command::new("systemctl");
    command
        .env("XDG_RUNTIME_DIR", format!("/run/user/{uid}"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path=/run/user/{uid}/bus"),
        );

    if Uid::current().is_root() {
        command.uid(uid).gid(gid);
    }

    command
}

fn queue_target_user_unit_restart(unit: &str) -> Result<(), String> {
    let mut command = target_user_systemctl_command();
    command
        .args(["--user", "restart", unit])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    tokio::task::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        command.status()
    });

    Ok(())
}

fn queue_system_unit_restart(unit: &str) -> Result<(), String> {
    let status = Command::new("systemctl")
        .args(["status", unit])
        .output()
        .map_err(|e| format!("Failed to inspect {unit}: {e}"))?;

    if !status.status.success() && unit_not_found(&status) {
        return Ok(());
    }

    Command::new("sh")
        .arg("-c")
        .arg("sleep 0.2; exec systemctl restart \"$1\"")
        .arg("zenbook-duo-restart")
        .arg(unit)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to queue restart for {unit}: {e}"))
}

fn target_uid() -> u32 {
    std::env::var("ZENBOOK_DUO_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_else(|| Uid::current().as_raw())
}

fn target_gid() -> u32 {
    std::env::var("ZENBOOK_DUO_GID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_else(|| Gid::current().as_raw())
}

fn unit_not_found(output: &Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.contains("not loaded")
        || stderr.contains("could not be found")
        || stderr.contains("Unit ")
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
            DaemonRequest::HandleLifecycle { phase } => match DisplayReplayPolicy::handle_lifecycle(&state, phase).await
            {
                Ok(()) => DaemonResponse::Ack,
                Err(message) => DaemonResponse::Error { message },
            },
            DaemonRequest::GetStatus => {
                let guard = state.read().await;
                let mut status = guard.status.clone();
                status.service_active = guard.session_agent.connected;
                DaemonResponse::Status { status }
            }
            DaemonRequest::GetVersion => DaemonResponse::Version {
                version: DaemonVersionInfo {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    protocol_version: PROTOCOL_VERSION,
                },
            },
            DaemonRequest::GetDisplayLayout => {
                match SessionBridge::request(state.clone(), SessionCommand::GetDisplayLayout, true).await {
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
            DaemonRequest::SetBacklight { level } => match hardware::hid::set_backlight(level) {
                Ok(()) => {
                    let mut guard = state.write().await;
                    guard.status.backlight_level = level;
                    let _ = logger::append_line(format!(
                        "rust-daemon: set backlight request -> {level}"
                    ));
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
            },
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
            DaemonRequest::RestartService => match ServiceManager::restart_owned_services() {
                Ok(()) => DaemonResponse::Ack,
                Err(message) => DaemonResponse::Error { message },
            },
            DaemonRequest::RegisterSessionAgent {
                session_id,
                backend,
                socket_path,
            } => {
                match handle_session_registration(&state, session_id, backend, socket_path).await {
                    Ok(()) => DaemonResponse::Ack,
                    Err(message) => DaemonResponse::Error { message },
                }
            }
            DaemonRequest::AppendLog { line } => match logger::append_line(line) {
                Ok(()) => DaemonResponse::Ack,
                Err(message) => DaemonResponse::Error { message },
            },
            DaemonRequest::TailLogs { lines } => DaemonResponse::Logs {
                lines: hardware::sysfs::read_log_lines(lines),
            },
            DaemonRequest::ClearLogs => match logger::clear() {
                Ok(()) => DaemonResponse::Ack,
                Err(message) => DaemonResponse::Error { message },
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
            DaemonRequest::ListTouchscreens => {
                let devices = hardware::touchscreen::list_touchscreens();
                DaemonResponse::Touchscreens { devices }
            }
            DaemonRequest::SetTouchscreenEnabled { connector, enabled } => {
                let devices = hardware::touchscreen::list_touchscreens();
                match devices.iter().find(|d| d.connector == connector) {
                    Some(dev) => {
                        match hardware::touchscreen::set_touchscreen_enabled(&dev.i2c_id, enabled) {
                            Ok(()) => DaemonResponse::Ack,
                            Err(message) => DaemonResponse::Error { message },
                        }
                    }
                    None => DaemonResponse::Error {
                        message: format!("No touchscreen found for connector {}", connector),
                    },
                }
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

pub(crate) struct DisplayReplayPolicy;

impl DisplayReplayPolicy {
    async fn handle_lifecycle(state: &Arc<RwLock<RuntimeState>>, phase: LifecyclePhase) -> Result<(), String> {
        handle_lifecycle(state, phase).await
    }

    pub(crate) async fn handle_lid_closed_change(
        state: &Arc<RwLock<RuntimeState>>,
        lid_closed: bool,
    ) -> Result<(), String> {
        handle_lid_closed_change(state, lid_closed).await
    }
}

async fn handle_lifecycle(
    state: &Arc<RwLock<RuntimeState>>,
    phase: LifecyclePhase,
) -> Result<(), String> {
    match phase {
        LifecyclePhase::Pre | LifecyclePhase::Hibernate | LifecyclePhase::Shutdown => {
            logger::append_line(format!("rust-daemon: lifecycle -> {:?}", phase)).ok();
            if lifecycle_should_stop_usb_media_remap(&phase) {
                if let Err(err) = commands::usb_media_remap::stop_remap() {
                    log::warn!(
                        "failed to stop usb media remap for lifecycle {:?}: {err}",
                        phase
                    );
                    logger::append_line(format!(
                        "rust-daemon: lifecycle usb media remap stop skipped: {}",
                        err
                    ))
                    .ok();
                }
            }
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
            if lifecycle_should_queue_usb_media_remap_retry(&phase) {
                crate::runtime::monitor::queue_usb_media_remap_resume_retry(state.clone());
            }

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

            // Restore touchscreen disabled state
            {
                let guard = state.read().await;
                let disabled = guard.settings.touchscreen_disabled.clone();
                drop(guard);
                for connector in &disabled {
                    let devices = hardware::touchscreen::list_touchscreens();
                    if let Some(dev) = devices.iter().find(|d| &d.connector == connector) {
                        if let Err(e) =
                            hardware::touchscreen::set_touchscreen_enabled(&dev.i2c_id, false)
                        {
                            eprintln!(
                                "rust-daemon: failed to restore touchscreen disabled for {}: {}",
                                connector, e
                            );
                        }
                    }
                }
            }

            refresh_lifecycle_display_mode(state, attached, scale).await
        }
    }
}

fn lifecycle_should_stop_usb_media_remap(phase: &LifecyclePhase) -> bool {
    matches!(
        phase,
        LifecyclePhase::Pre | LifecyclePhase::Hibernate | LifecyclePhase::Shutdown
    )
}

fn lifecycle_should_queue_usb_media_remap_retry(phase: &LifecyclePhase) -> bool {
    matches!(phase, LifecyclePhase::Post | LifecyclePhase::Thaw)
}

async fn refresh_lifecycle_display_mode(
    state: &Arc<RwLock<RuntimeState>>,
    attached: bool,
    scale: f64,
) -> Result<(), String> {
    match replay_current_display_mode_with_disconnect(state, attached, scale, false).await {
        Ok(()) => Ok(()),
        Err(err) if is_display_session_deferral(&err) => {
            logger::append_line(format!(
                "rust-daemon: lifecycle dock refresh deferred: {}",
                err
            ))
            .ok();
            queue_lifecycle_display_retry(state.clone(), attached, scale);
            Ok(())
        }
        Err(err) => {
            NotificationSink::runtime_error(
                state,
                "Zenbook Duo Runtime Error",
                &format!("Lifecycle dock refresh skipped: {err}"),
            )
            .await;
            logger::append_line(format!(
                "rust-daemon: lifecycle dock refresh skipped: {}",
                err
            ))
            .ok();
            Ok(())
        }
    }
}

async fn handle_session_registration(
    state: &Arc<RwLock<RuntimeState>>,
    session_id: String,
    backend: crate::ipc::protocol::SessionBackend,
    socket_path: String,
) -> Result<(), String> {
    let (attached, scale) = {
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
        (guard.status.keyboard_attached, guard.settings.default_scale)
    };

    let state = Arc::clone(state);
    tokio::spawn(async move {
        if let Err(err) = replay_current_display_mode(&state, attached, scale).await {
            if is_display_session_deferral(&err) {
                let _ = logger::append_line(format!(
                    "rust-daemon: session registration dock replay deferred: {}",
                    err
                ));
                queue_lifecycle_display_retry(state.clone(), attached, scale);
                return;
            }

            log::warn!("session registration dock replay failed: {err}");
            NotificationSink::runtime_error(
                &state,
                "Zenbook Duo Runtime Error",
                &format!("Session registration dock replay skipped: {err}"),
            )
            .await;
            let _ = logger::append_line(format!(
                "rust-daemon: session registration dock replay skipped: {}",
                err
            ));
        }
    });

    Ok(())
}

pub(crate) async fn replay_current_display_mode(
    state: &Arc<RwLock<RuntimeState>>,
    attached: bool,
    scale: f64,
) -> Result<(), String> {
    replay_current_display_mode_with_disconnect(state, attached, scale, true).await
}

async fn replay_current_display_mode_with_disconnect(
    state: &Arc<RwLock<RuntimeState>>,
    attached: bool,
    scale: f64,
    disconnect_on_failure: bool,
) -> Result<(), String> {
    if state.read().await.lid_closed {
        if apply_external_only_clamshell_layout(state, disconnect_on_failure).await? {
            let _ = logger::append_line(
                "rust-daemon: enforced external-only display layout while lid is closed",
            );
        } else {
            let _ = logger::append_line(
                "rust-daemon: skipped display replay while lid is closed and no external display is active",
            );
        }
        return Ok(());
    }

    let saved_layout = saved_layout_base(state).await;
    let exact_saved_layout = saved_layout
        .clone()
        .filter(|layout| saved_layout_matches_display_mode(layout, attached));

    if active_external_display_connected(state, attached).await
        && saved_layout
            .as_ref()
            .map(layout_manages_only_internal_displays)
            .unwrap_or(true)
    {
        let _ = logger::append_line(
            "rust-daemon: skipped internal display replay while an external display is active",
        );
        return Ok(());
    }

    if let Some(layout) = exact_saved_layout {
        return forward_session_command_with_disconnect(
            state,
            SessionCommand::ApplyDisplayLayout { layout },
            disconnect_on_failure,
        )
        .await;
    }

    replay_current_dock_mode_with_disconnect(
        state,
        attached,
        scale,
        saved_layout,
        disconnect_on_failure,
    )
    .await
}

pub(crate) async fn handle_lid_closed_change(
    state: &Arc<RwLock<RuntimeState>>,
    lid_closed: bool,
) -> Result<(), String> {
    let Some((attached, scale)) = record_lid_closed_state(state, lid_closed).await else {
        let _ = logger::append_line(format!(
            "rust-daemon: ignored duplicate lid {} signal",
            if lid_closed { "closed" } else { "opened" }
        ));
        return Ok(());
    };

    let result = apply_lid_display_state(state, lid_closed, attached, scale, true).await;
    if let Err(err) = &result {
        if is_display_session_deferral(err) {
            queue_lid_display_retry(state.clone(), lid_closed);
        }
    }
    result
}

async fn record_lid_closed_state(
    state: &Arc<RwLock<RuntimeState>>,
    lid_closed: bool,
) -> Option<(bool, f64)> {
    let mut guard = state.write().await;
    if guard.lid_closed == lid_closed {
        return None;
    }

    guard.lid_closed = lid_closed;
    guard.recent_events.push(HardwareEvent::info(
        EventCategory::Display,
        if lid_closed {
            "Lid closed"
        } else {
            "Lid opened"
        },
        "rust-daemon",
    ));
    if guard.recent_events.len() > 500 {
        let overflow = guard.recent_events.len() - 500;
        guard.recent_events.drain(0..overflow);
    }
    guard.touch();
    persist_state(&guard);
    Some((guard.status.keyboard_attached, guard.settings.default_scale))
}

async fn apply_lid_display_state(
    state: &Arc<RwLock<RuntimeState>>,
    lid_closed: bool,
    attached: bool,
    scale: f64,
    log_transition: bool,
) -> Result<(), String> {
    if lid_closed {
        if apply_external_only_clamshell_layout(state, false).await? {
            if log_transition {
                let _ = logger::append_line(
                    "rust-daemon: lid closed with external display; applied external-only layout",
                );
            }
        } else if log_transition {
            let _ = logger::append_line(
                "rust-daemon: lid closed without an active external display; no display layout change applied",
            );
        }
        Ok(())
    } else {
        if log_transition {
            let _ =
                logger::append_line("rust-daemon: lid opened; replaying current dock display mode");
        }
        replay_current_display_mode_with_disconnect(state, attached, scale, false).await
    }
}

fn queue_lid_display_retry(state: Arc<RwLock<RuntimeState>>, lid_closed: bool) {
    tokio::spawn(async move {
        const RETRY_ATTEMPTS: usize = 6;
        const RETRY_DELAY: Duration = Duration::from_secs(1);

        for attempt in 1..=RETRY_ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;

            let (current_lid_closed, attached, scale) = {
                let guard = state.read().await;
                (
                    guard.lid_closed,
                    guard.status.keyboard_attached,
                    guard.settings.default_scale,
                )
            };
            if current_lid_closed != lid_closed {
                let _ = logger::append_line(
                    "rust-daemon: cancelled lid display retry after lid state changed",
                );
                return;
            }

            match apply_lid_display_state(&state, lid_closed, attached, scale, false).await {
                Ok(()) => {
                    let _ = logger::append_line(format!(
                        "rust-daemon: lid display retry succeeded on attempt {attempt}"
                    ));
                    return;
                }
                Err(err) if is_display_session_deferral(&err) => {
                    let _ = logger::append_line(format!(
                        "rust-daemon: lid display retry deferred on attempt {attempt}: {err}"
                    ));
                }
                Err(err) => {
                    log::warn!("lid display retry failed: {err}");
                    let _ = logger::append_line(format!(
                        "rust-daemon: lid display retry failed on attempt {attempt}: {err}"
                    ));
                    return;
                }
            }
        }
    });
}

fn queue_lifecycle_display_retry(state: Arc<RwLock<RuntimeState>>, attached: bool, scale: f64) {
    tokio::spawn(async move {
        const RETRY_ATTEMPTS: usize = 6;
        const RETRY_DELAY: Duration = Duration::from_secs(1);

        for attempt in 1..=RETRY_ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;

            match replay_current_display_mode_with_disconnect(&state, attached, scale, false).await
            {
                Ok(()) => {
                    let _ = logger::append_line(format!(
                        "rust-daemon: lifecycle dock refresh retry succeeded on attempt {attempt}"
                    ));
                    return;
                }
                Err(err) if is_display_session_deferral(&err) => {
                    let _ = logger::append_line(format!(
                        "rust-daemon: lifecycle dock refresh retry deferred on attempt {attempt}: {err}"
                    ));
                }
                Err(err) => {
                    log::warn!("lifecycle dock refresh retry failed: {err}");
                    let _ = logger::append_line(format!(
                        "rust-daemon: lifecycle dock refresh retry failed on attempt {attempt}: {err}"
                    ));
                    return;
                }
            }
        }
    });
}

pub(crate) fn is_display_session_deferral(message: &str) -> bool {
    message == "No session agent registered"
        || message.starts_with("Timed out ")
        || message.starts_with("Failed to connect to session agent")
        || message == "Session agent closed before replying"
        || is_niri_socket_refused(message)
}

fn is_niri_socket_refused(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("error connecting to the niri socket")
        && message.contains("connection refused")
}

async fn apply_external_only_clamshell_layout(
    state: &Arc<RwLock<RuntimeState>>,
    disconnect_on_failure: bool,
) -> Result<bool, String> {
    let current_layout =
        session_display_layout_result(state.clone(), disconnect_on_failure).await?;
    let Some(layout) = external_only_layout(&current_layout) else {
        return Ok(false);
    };

    forward_session_command_with_disconnect(
        state,
        SessionCommand::ApplyDisplayLayout { layout },
        disconnect_on_failure,
    )
    .await?;
    Ok(true)
}

fn external_only_layout(layout: &DisplayLayout) -> Option<DisplayLayout> {
    let mut displays: Vec<_> = layout
        .displays
        .iter()
        .filter(|display| !hardware::duo::is_internal_connector(&display.connector))
        .cloned()
        .collect();

    if displays.is_empty() {
        return None;
    }

    if !displays.iter().any(|display| display.primary) {
        displays[0].primary = true;
    }
    for display in displays.iter_mut().skip(1) {
        display.primary = false;
    }

    Some(DisplayLayout { displays })
}

async fn saved_layout_base(state: &Arc<RwLock<RuntimeState>>) -> Option<DisplayLayout> {
    state
        .read()
        .await
        .settings
        .saved_display_layout
        .clone()
        .filter(|layout| !layout.displays.is_empty())
}

fn saved_layout_matches_display_mode(layout: &DisplayLayout, attached: bool) -> bool {
    let display_count = layout.displays.len();

    if display_count == 0 {
        return false;
    }

    if attached {
        display_count == 1
    } else {
        display_count > 1
    }
}

fn layout_manages_only_internal_displays(layout: &DisplayLayout) -> bool {
    layout
        .displays
        .iter()
        .all(|display| hardware::duo::is_internal_connector(&display.connector))
}

fn layout_has_external_display(layout: &DisplayLayout) -> bool {
    layout
        .displays
        .iter()
        .any(|display| !hardware::duo::is_internal_connector(&display.connector))
}

async fn active_external_display_connected(
    state: &Arc<RwLock<RuntimeState>>,
    attached: bool,
) -> bool {
    let expected_internal_count = if attached { 1 } else { 2 };
    if state.read().await.status.monitor_count <= expected_internal_count {
        return false;
    }

    session_display_layout(state.clone())
        .await
        .as_ref()
        .is_some_and(layout_has_external_display)
}

async fn replay_current_dock_mode_with_disconnect(
    state: &Arc<RwLock<RuntimeState>>,
    attached: bool,
    scale: f64,
    layout: Option<DisplayLayout>,
    disconnect_on_failure: bool,
) -> Result<(), String> {
    forward_session_command_with_disconnect(
        state,
        SessionCommand::SetDockMode {
            attached,
            scale,
            layout,
        },
        disconnect_on_failure,
    )
    .await
}

pub(crate) async fn forward_session_command(
    state: &Arc<RwLock<RuntimeState>>,
    command: SessionCommand,
) -> Result<(), String> {
    forward_session_command_with_disconnect(state, command, true).await
}

async fn forward_session_command_with_disconnect(
    state: &Arc<RwLock<RuntimeState>>,
    command: SessionCommand,
    disconnect_on_failure: bool,
) -> Result<(), String> {
    match SessionBridge::request(state.clone(), command, disconnect_on_failure).await? {
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
    session_display_layout_result(state, false).await.ok()
}

async fn session_display_layout_result(
    state: Arc<RwLock<RuntimeState>>,
    disconnect_on_failure: bool,
) -> Result<DisplayLayout, String> {
    match request_session(
        state,
        SessionCommand::GetDisplayLayout,
        disconnect_on_failure,
    )
    .await?
    {
        SessionResponse::DisplayLayout { layout } => Ok(layout),
        SessionResponse::Error { message } => Err(message),
        SessionResponse::Ack => Err("Unexpected ack response for display-layout request".into()),
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

pub(crate) struct SessionBridge;

impl SessionBridge {
    async fn request(
        state: Arc<RwLock<RuntimeState>>,
        command: SessionCommand,
        disconnect_on_failure: bool,
    ) -> Result<SessionResponse, String> {
        request_session(state, command, disconnect_on_failure).await
    }
}

async fn request_session(
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
    if let Err(e) = timeout(Duration::from_secs(3), writer.write_all(line.as_bytes()))
        .await
        .map_err(|_| "Timed out writing session command".to_string())?
    {
        if disconnect_on_failure {
            mark_session_agent_disconnected(
                &state,
                &format!("Failed to write session command: {e}"),
            )
            .await;
        }
        return Err(format!("Failed to write session command: {e}"));
    }
    if let Err(e) = timeout(Duration::from_secs(3), writer.write_all(b"\n"))
        .await
        .map_err(|_| "Timed out terminating session command".to_string())?
    {
        if disconnect_on_failure {
            mark_session_agent_disconnected(
                &state,
                &format!("Failed to terminate session command: {e}"),
            )
            .await;
        }
        return Err(format!("Failed to terminate session command: {e}"));
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

async fn mark_session_agent_disconnected(state: &Arc<RwLock<RuntimeState>>, reason: &str) {
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
    drop(guard);

    if was_connected {
        if let Err(err) = queue_target_user_unit_restart("zenbook-duo-session-agent.service") {
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
        if let Err(err) = send_runtime_notification_direct(
            "Zenbook Duo Runtime Error",
            &format!("Session agent disconnected: {reason}"),
        ) {
            log::warn!("failed to send direct disconnect notification: {err}");
        }
    }
}

fn should_notify_session_agent_disconnect(reason: &str) -> bool {
    !is_display_session_deferral(reason)
}

pub(crate) struct NotificationSink;

impl NotificationSink {
    async fn runtime_error(state: &Arc<RwLock<RuntimeState>>, title: &str, message: &str) {
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

fn should_notify_runtime_error(title: &str, message: &str) -> bool {
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
    match request_session(
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

fn send_runtime_notification_direct(title: &str, message: &str) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::duo::{PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR};
    use crate::ipc::protocol::SessionBackend;
    use crate::models::{DisplayInfo, DisplayLayout, DisplayMode, RefreshPolicy};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    const NIRI_SOCKET_REFUSED: &str = "Error: error connecting to the niri socket\n\nCaused by:\n    Connection refused (os error 111)";

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
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
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

    #[tokio::test]
    async fn session_registration_replays_current_detached_dock_state() {
        let socket_path = unique_test_socket_path("register-detached");
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
                SessionCommand::SetDockMode {
                    attached,
                    scale,
                    layout,
                } => {
                    assert!(!attached);
                    assert_eq!(scale, 1.66);
                    assert!(layout.is_none());
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = false;
            guard.settings.default_scale = 1.66;
        }

        handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Gnome,
            socket_path.to_string_lossy().into_owned(),
        )
        .await
        .expect("registration should succeed");

        let guard = state.read().await;
        assert!(guard.session_agent.connected);
        assert_eq!(
            guard.session_agent.session_id.as_deref(),
            Some("test-session")
        );
        drop(guard);

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn session_registration_replays_saved_display_layout_when_detached() {
        let socket_path = unique_test_socket_path("register-saved-layout");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");
        let saved_layout = dual_display_layout();

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
                SessionCommand::ApplyDisplayLayout { layout } => {
                    assert_eq!(layout.displays.len(), 2);
                    assert_eq!(layout.displays[0].connector, PRIMARY_INTERNAL_CONNECTOR);
                    assert_eq!(layout.displays[1].connector, SECONDARY_INTERNAL_CONNECTOR);
                    assert_eq!(layout.displays[1].y, 1200);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = false;
            guard.settings.default_scale = 1.66;
            guard.settings.saved_display_layout = Some(saved_layout);
        }

        handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Gnome,
            socket_path.to_string_lossy().into_owned(),
        )
        .await
        .expect("registration should succeed");

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn session_registration_passes_saved_layout_through_dock_replay_when_mode_shape_differs()
    {
        let socket_path = unique_test_socket_path("register-fallback-layout");
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
                SessionCommand::SetDockMode {
                    attached,
                    scale,
                    layout,
                } => {
                    assert!(attached);
                    assert_eq!(scale, 1.5);
                    let layout = layout.expect("fallback layout should be forwarded");
                    assert_eq!(layout.displays.len(), 2);
                    assert_eq!(layout.displays[0].current_mode.refresh_rate, 120.0);
                    assert_eq!(layout.displays[1].current_mode.refresh_rate, 120.0);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.5;
            guard.settings.saved_display_layout = Some(dual_display_layout_with_refresh(120.0));
        }

        handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Gnome,
            socket_path.to_string_lossy().into_owned(),
        )
        .await
        .expect("registration should succeed");

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn niri_socket_refused_session_registration_replay_defers_and_retries() {
        let socket_path = unique_test_socket_path("register-niri-refused");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener
                .accept()
                .await
                .expect("accept initial replay command");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read initial replay command")
                .expect("initial replay command line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode initial replay command");
            assert!(matches!(
                envelope.payload,
                SessionCommand::SetDockMode { .. }
            ));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::Error {
                message: NIRI_SOCKET_REFUSED.into(),
            }))
            .expect("encode niri refusal response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write niri refusal response");
            writer
                .write_all(b"\n")
                .await
                .expect("terminate niri refusal response");

            let (stream, _) = listener
                .accept()
                .await
                .expect("accept retry replay command");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read retry replay command")
                .expect("retry replay command line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode retry replay command");
            assert!(matches!(
                envelope.payload,
                SessionCommand::SetDockMode { .. }
            ));
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.5;
        }

        handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Niri,
            socket_path.to_string_lossy().into_owned(),
        )
        .await
        .expect("registration should succeed");

        assert!(state.read().await.session_agent.connected);

        timeout(Duration::from_secs(3), server)
            .await
            .expect("registration replay retry should finish")
            .expect("join retry server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn replay_skips_internal_saved_layout_when_external_display_is_active() {
        let socket_path = unique_test_socket_path("replay-external-skip");
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
            assert!(matches!(envelope.payload, SessionCommand::GetDisplayLayout));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::DisplayLayout {
                layout: external_display_layout(),
            }))
            .expect("encode layout response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write layout");
            writer.write_all(b"\n").await.expect("terminate layout");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.keyboard_attached = false;
            guard.status.monitor_count = 3;
            guard.settings.saved_display_layout = Some(dual_display_layout());
        }

        timeout(
            Duration::from_secs(1),
            replay_current_display_mode(&state, false, 1.66),
        )
        .await
        .expect("display replay should not hang")
        .expect("external display should cause a safe no-op");

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn replay_applies_saved_external_layout_with_refresh_when_external_display_is_active() {
        let socket_path = unique_test_socket_path("replay-external-saved");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept layout query client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read layout query")
                .expect("layout query line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode layout query");
            assert!(matches!(envelope.payload, SessionCommand::GetDisplayLayout));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::DisplayLayout {
                layout: external_display_layout(),
            }))
            .expect("encode layout response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write layout");
            writer.write_all(b"\n").await.expect("terminate layout");

            let (stream, _) = listener.accept().await.expect("accept layout apply client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read layout apply")
                .expect("layout apply line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode layout apply");
            match envelope.payload {
                SessionCommand::ApplyDisplayLayout { layout } => {
                    assert_eq!(layout.displays.len(), 3);
                    let external = layout
                        .displays
                        .iter()
                        .find(|display| display.connector == "HDMI-A-1")
                        .expect("external display should be preserved");
                    assert_eq!(external.current_mode.refresh_rate, 144.0);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.keyboard_attached = false;
            guard.status.monitor_count = 3;
            guard.settings.saved_display_layout = Some(external_display_layout_with_refresh(144.0));
        }

        timeout(
            Duration::from_secs(1),
            replay_current_display_mode(&state, false, 1.66),
        )
        .await
        .expect("display replay should not hang")
        .expect("saved external layout should apply");

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn lid_close_applies_external_only_layout() {
        let socket_path = unique_test_socket_path("lid-close-external");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept layout query client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read layout query")
                .expect("layout query line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode layout query");
            assert!(matches!(envelope.payload, SessionCommand::GetDisplayLayout));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::DisplayLayout {
                layout: external_display_layout(),
            }))
            .expect("encode layout response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write layout response");
            writer.write_all(b"\n").await.expect("terminate layout");

            let (stream, _) = listener.accept().await.expect("accept layout apply client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read layout apply")
                .expect("layout apply line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode layout apply");
            match envelope.payload {
                SessionCommand::ApplyDisplayLayout { layout } => {
                    assert_eq!(layout.displays.len(), 1);
                    assert_eq!(layout.displays[0].connector, "HDMI-A-1");
                    assert!(layout.displays[0].primary);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
        }

        timeout(
            Duration::from_secs(1),
            handle_lid_closed_change(&state, true),
        )
        .await
        .expect("lid close handling should not hang")
        .expect("external-only layout should apply");

        assert!(state.read().await.lid_closed);

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn session_registration_does_not_reenable_internal_displays_when_lid_is_closed() {
        let socket_path = unique_test_socket_path("register-lid-closed");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept layout query client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read layout query")
                .expect("layout query line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode layout query");
            assert!(matches!(envelope.payload, SessionCommand::GetDisplayLayout));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::DisplayLayout {
                layout: external_display_layout(),
            }))
            .expect("encode layout response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write layout response");
            writer.write_all(b"\n").await.expect("terminate layout");

            let (stream, _) = listener.accept().await.expect("accept layout apply client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read layout apply")
                .expect("layout apply line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode layout apply");
            match envelope.payload {
                SessionCommand::ApplyDisplayLayout { layout } => {
                    assert_eq!(layout.displays.len(), 1);
                    assert_eq!(layout.displays[0].connector, "HDMI-A-1");
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.lid_closed = true;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.25;
        }

        handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Gnome,
            socket_path.to_string_lossy().into_owned(),
        )
        .await
        .expect("registration should succeed");

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn duplicate_lid_open_signal_is_ignored_without_display_replay() {
        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some("/tmp/zenbook-duo-missing-session.sock".into());
            guard.lid_closed = false;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.5;
        }

        handle_lid_closed_change(&state, false)
            .await
            .expect("duplicate lid-open signal should be ignored");

        let guard = state.read().await;
        assert!(!guard.lid_closed);
        assert!(guard.recent_events.is_empty());
    }

    #[tokio::test]
    async fn duplicate_lid_closed_signal_is_ignored_without_display_replay() {
        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some("/tmp/zenbook-duo-missing-session.sock".into());
            guard.lid_closed = true;
        }

        handle_lid_closed_change(&state, true)
            .await
            .expect("duplicate lid-closed signal should be ignored");

        let guard = state.read().await;
        assert!(guard.lid_closed);
        assert!(guard.recent_events.is_empty());
    }

    #[tokio::test]
    async fn lid_open_replays_saved_layout_as_refresh_preserving_dock_base() {
        let socket_path = unique_test_socket_path("lid-open-replay");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept replay client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read replay request")
                .expect("replay request line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode replay request");
            match envelope.payload {
                SessionCommand::SetDockMode {
                    attached,
                    scale,
                    layout,
                } => {
                    assert!(attached);
                    assert_eq!(scale, 1.5);
                    let layout = layout.expect("saved layout should be forwarded as dock base");
                    assert_eq!(layout.displays.len(), 2);
                    assert_eq!(layout.displays[0].current_mode.refresh_rate, 120.0);
                    assert_eq!(layout.displays[1].current_mode.refresh_rate, 120.0);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.lid_closed = true;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.5;
            guard.settings.saved_display_layout = Some(dual_display_layout_with_refresh(120.0));
        }

        timeout(
            Duration::from_secs(1),
            handle_lid_closed_change(&state, false),
        )
        .await
        .expect("lid open handling should not hang")
        .expect("dock replay should succeed");

        assert!(!state.read().await.lid_closed);

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn missing_session_agent_during_lid_transition_defers_without_disconnect() {
        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.service_active = true;
        }

        let result = handle_lid_closed_change(&state, true).await;

        assert_eq!(result, Err("No session agent registered".into()));
        assert!(is_display_session_deferral(
            result.as_ref().expect_err("lid update should defer")
        ));
        let guard = state.read().await;
        assert!(guard.lid_closed);
        assert!(guard.status.service_active);
    }

    #[tokio::test]
    async fn timed_out_lid_transition_keeps_session_agent_connected_and_retries() {
        let socket_path = unique_test_socket_path("lid-timeout-retry");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener
                .accept()
                .await
                .expect("accept initial layout query client");
            let held_stream = stream;

            let (stream, _) = listener
                .accept()
                .await
                .expect("accept retry layout query client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read retry layout query")
                .expect("retry layout query line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode retry layout query");
            assert!(matches!(envelope.payload, SessionCommand::GetDisplayLayout));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::DisplayLayout {
                layout: external_display_layout(),
            }))
            .expect("encode retry layout response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write retry layout response");
            writer
                .write_all(b"\n")
                .await
                .expect("terminate retry layout");

            let (stream, _) = listener
                .accept()
                .await
                .expect("accept retry layout apply client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read retry layout apply")
                .expect("retry layout apply line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode retry layout apply");
            match envelope.payload {
                SessionCommand::ApplyDisplayLayout { layout } => {
                    assert_eq!(layout.displays.len(), 1);
                    assert_eq!(layout.displays[0].connector, "HDMI-A-1");
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");

            drop(held_stream);
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.service_active = true;
        }

        let result = timeout(
            Duration::from_secs(4),
            handle_lid_closed_change(&state, true),
        )
        .await
        .expect("lid close handling should return after the session timeout");

        assert_eq!(result, Err("Timed out waiting for session response".into()));
        assert!(is_display_session_deferral(
            result.as_ref().expect_err("lid update should defer")
        ));
        {
            let guard = state.read().await;
            assert!(guard.session_agent.connected);
            assert!(guard.status.service_active);
        }

        timeout(Duration::from_secs(5), server)
            .await
            .expect("lid retry should finish")
            .expect("join retry server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn niri_socket_refused_lifecycle_refresh_defers_without_disconnect_and_retries() {
        let socket_path = unique_test_socket_path("lifecycle-niri-refused");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener
                .accept()
                .await
                .expect("accept initial display command");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read initial display command")
                .expect("initial display command line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode initial display command");
            assert!(matches!(
                envelope.payload,
                SessionCommand::SetDockMode { .. }
            ));
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::Error {
                message: NIRI_SOCKET_REFUSED.into(),
            }))
            .expect("encode niri refusal response");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write niri refusal response");
            writer
                .write_all(b"\n")
                .await
                .expect("terminate niri refusal response");

            let (stream, _) = listener
                .accept()
                .await
                .expect("accept retry display command");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read retry display command")
                .expect("retry display command line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode retry display command");
            assert!(matches!(
                envelope.payload,
                SessionCommand::SetDockMode { .. }
            ));
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.service_active = true;
        }

        refresh_lifecycle_display_mode(&state, true, 1.66)
            .await
            .expect("niri refusal should defer lifecycle refresh");

        let guard = state.read().await;
        assert!(guard.session_agent.connected);
        assert!(guard.status.service_active);
        drop(guard);

        timeout(Duration::from_secs(3), server)
            .await
            .expect("lifecycle niri retry should finish")
            .expect("join retry server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn lifecycle_refresh_forwards_saved_layout_as_refresh_preserving_dock_base() {
        let socket_path = unique_test_socket_path("lifecycle-saved-layout");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener
                .accept()
                .await
                .expect("accept lifecycle replay client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read lifecycle replay request")
                .expect("lifecycle replay request line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode lifecycle replay request");
            match envelope.payload {
                SessionCommand::SetDockMode {
                    attached,
                    scale,
                    layout,
                } => {
                    assert!(attached);
                    assert_eq!(scale, 1.25);
                    let layout = layout.expect("saved layout should be forwarded as dock base");
                    assert_eq!(layout.displays.len(), 2);
                    assert!(layout
                        .displays
                        .iter()
                        .all(|display| display.current_mode.refresh_rate == 120.0));
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.settings.saved_display_layout = Some(dual_display_layout_with_refresh(120.0));
        }

        timeout(
            Duration::from_secs(1),
            refresh_lifecycle_display_mode(&state, true, 1.25),
        )
        .await
        .expect("lifecycle replay should not hang")
        .expect("lifecycle replay should succeed");

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn lifecycle_display_timeout_defers_without_disconnect() {
        let socket_path = unique_test_socket_path("lifecycle-timeout");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept test session client");
            let (_reader, _writer) = stream.into_split();
            tokio::time::sleep(Duration::from_secs(4)).await;
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.service_active = true;
        }

        timeout(
            Duration::from_secs(4),
            refresh_lifecycle_display_mode(&state, true, 1.66),
        )
        .await
        .expect("lifecycle refresh should return after session timeout")
        .expect("retryable lifecycle refresh should be handled");

        let guard = state.read().await;
        assert!(guard.session_agent.connected);
        assert!(guard.status.service_active);
        drop(guard);

        server.abort();
        let _ = fs::remove_file(&socket_path);
    }

    #[test]
    fn niri_socket_refused_is_display_session_deferral_but_other_compositor_errors_are_not() {
        assert!(is_display_session_deferral(NIRI_SOCKET_REFUSED));
        assert!(!is_display_session_deferral("gdctl set failed"));
        assert!(!is_display_session_deferral(
            "Dynamic refresh is disabled on Niri because it is unstable on this hardware (eDP-1)"
        ));
    }

    #[test]
    fn retryable_session_disconnect_reasons_are_not_directly_notified() {
        assert!(!should_notify_session_agent_disconnect(
            "Timed out waiting for session response"
        ));
        assert!(!should_notify_session_agent_disconnect(
            "No session agent registered"
        ));
        assert!(should_notify_session_agent_disconnect(
            "Failed to decode session response"
        ));
    }

    #[tokio::test]
    async fn passive_display_layout_timeout_keeps_session_agent_connected() {
        let socket_path = unique_test_socket_path("passive-layout-timeout");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept test session client");
            let (_reader, _writer) = stream.into_split();
            tokio::time::sleep(Duration::from_secs(4)).await;
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.service_active = true;
        }

        let layout = timeout(
            Duration::from_secs(4),
            session_display_layout(state.clone()),
        )
        .await
        .expect("passive layout probe should return after session timeout");

        assert!(layout.is_none());
        let guard = state.read().await;
        assert!(guard.session_agent.connected);
        assert!(guard.status.service_active);
        drop(guard);

        server.abort();
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn active_session_command_timeout_marks_session_agent_disconnected() {
        let socket_path = unique_test_socket_path("active-command-timeout");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept test session client");
            let (_reader, _writer) = stream.into_split();
            tokio::time::sleep(Duration::from_secs(4)).await;
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
            guard.status.service_active = true;
        }

        let result = timeout(
            Duration::from_secs(4),
            forward_session_command(
                &state,
                SessionCommand::SetDockMode {
                    attached: true,
                    scale: 1.66,
                    layout: None,
                },
            ),
        )
        .await
        .expect("active session command should return after session timeout");

        assert_eq!(result, Err("Timed out waiting for session response".into()));
        let guard = state.read().await;
        assert!(!guard.session_agent.connected);
        assert!(!guard.status.service_active);
        drop(guard);

        server.abort();
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn session_registration_acknowledges_before_dock_replay_completes() {
        let socket_path = unique_test_socket_path("register-async");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.5;
        }

        timeout(
            Duration::from_millis(500),
            handle_session_registration(
                &state,
                "test-session".into(),
                SessionBackend::Niri,
                socket_path.to_string_lossy().into_owned(),
            ),
        )
        .await
        .expect("registration should return before replay finishes")
        .expect("registration should succeed");

        let (stream, _) = timeout(Duration::from_secs(1), listener.accept())
            .await
            .expect("replay should connect after registration")
            .expect("accept replay connection");
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await
            .expect("read replay request")
            .expect("session request line");
        let envelope: Envelope<SessionCommand> =
            serde_json::from_str(&line).expect("decode session request");
        match envelope.payload {
            SessionCommand::SetDockMode {
                attached,
                scale,
                layout,
            } => {
                assert!(attached);
                assert_eq!(scale, 1.5);
                assert!(layout.is_none());
            }
            other => panic!("unexpected session command: {other:?}"),
        }
        let reply =
            serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
        writer.write_all(reply.as_bytes()).await.expect("write ack");
        writer.write_all(b"\n").await.expect("terminate ack");

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(state.read().await.session_agent.connected);

        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn session_registration_succeeds_even_if_dock_replay_fails() {
        let socket_path = unique_test_socket_path("register-fail");
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
                SessionCommand::SetDockMode {
                    attached,
                    scale,
                    layout,
                } => {
                    assert!(attached);
                    assert_eq!(scale, 2.0);
                    assert!(layout.is_none());
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply = serde_json::to_string(&Envelope::new(SessionResponse::Error {
                message: "display replay failed".into(),
            }))
            .expect("encode error");
            writer
                .write_all(reply.as_bytes())
                .await
                .expect("write error");
            writer.write_all(b"\n").await.expect("terminate error");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 2.0;
        }

        let result = handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Kde,
            socket_path.to_string_lossy().into_owned(),
        )
        .await;

        assert!(
            result.is_ok(),
            "registration should ignore dock replay errors"
        );
        assert!(state.read().await.session_agent.connected);

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn session_registration_replays_current_attached_dock_state() {
        let socket_path = unique_test_socket_path("register-attached");
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
                SessionCommand::SetDockMode {
                    attached,
                    scale,
                    layout,
                } => {
                    assert!(attached);
                    assert_eq!(scale, 1.25);
                    assert!(layout.is_none());
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.status.keyboard_attached = true;
            guard.settings.default_scale = 1.25;
        }

        handle_session_registration(
            &state,
            "test-session".into(),
            SessionBackend::Niri,
            socket_path.to_string_lossy().into_owned(),
        )
        .await
        .expect("registration should succeed");

        assert!(state.read().await.session_agent.connected);

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn runtime_error_notification_forwards_show_notification_command() {
        let socket_path = unique_test_socket_path("notify-runtime-error");
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
                SessionCommand::ShowNotification {
                    title,
                    message,
                    urgent,
                } => {
                    assert_eq!(title, "Zenbook Duo Runtime Error");
                    assert_eq!(message, "Dock-mode replay skipped");
                    assert!(urgent);
                }
                other => panic!("unexpected session command: {other:?}"),
            }
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
        }

        notify_runtime_error(
            &state,
            "Zenbook Duo Runtime Error",
            "Dock-mode replay skipped",
        )
        .await;

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn runtime_error_notification_suppresses_duplicate_messages() {
        let socket_path = unique_test_socket_path("notify-runtime-dedupe");
        let listener = UnixListener::bind(&socket_path).expect("bind test session socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept first test client");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("read first request")
                .expect("first session request line");
            let envelope: Envelope<SessionCommand> =
                serde_json::from_str(&line).expect("decode first session request");
            assert!(matches!(
                envelope.payload,
                SessionCommand::ShowNotification { .. }
            ));
            let reply =
                serde_json::to_string(&Envelope::new(SessionResponse::Ack)).expect("encode ack");
            writer.write_all(reply.as_bytes()).await.expect("write ack");
            writer.write_all(b"\n").await.expect("terminate ack");

            timeout(Duration::from_millis(250), listener.accept())
                .await
                .expect_err("duplicate notification should be suppressed");
        });

        let state = Arc::new(RwLock::new(RuntimeState::default()));
        {
            let mut guard = state.write().await;
            guard.session_agent.connected = true;
            guard.session_agent.socket_path = Some(socket_path.to_string_lossy().into_owned());
        }

        notify_runtime_error(
            &state,
            "Zenbook Duo Runtime Error",
            "Session agent disconnected",
        )
        .await;
        notify_runtime_error(
            &state,
            "Zenbook Duo Runtime Error",
            "Session agent disconnected",
        )
        .await;

        server.await.expect("join session server");
        let _ = fs::remove_file(&socket_path);
    }

    #[test]
    fn retryable_display_session_errors_are_not_notifiable() {
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            "Lifecycle dock refresh skipped: No session agent registered",
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            "Display-mode policy action failed: No session agent registered",
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            "Lifecycle dock refresh skipped: Timed out waiting for session response",
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            "Display-mode policy action failed: Timed out waiting for session response",
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            &format!("Lifecycle dock refresh skipped: {NIRI_SOCKET_REFUSED}"),
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            &format!("Display-mode policy action failed: {NIRI_SOCKET_REFUSED}"),
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            &format!("Session registration dock replay skipped: {NIRI_SOCKET_REFUSED}"),
        ));
        assert!(!should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            &format!("Lid state display update failed: {NIRI_SOCKET_REFUSED}"),
        ));
    }

    #[test]
    fn real_runtime_errors_remain_notifiable() {
        assert!(should_notify_runtime_error(
            "Zenbook Duo Runtime Error",
            "Display-mode policy action failed: gdctl set failed",
        ));
    }

    #[test]
    fn lifecycle_sleep_phases_stop_usb_media_remap_before_suspend() {
        assert!(lifecycle_should_stop_usb_media_remap(&LifecyclePhase::Pre));
        assert!(lifecycle_should_stop_usb_media_remap(
            &LifecyclePhase::Hibernate
        ));
        assert!(lifecycle_should_stop_usb_media_remap(
            &LifecyclePhase::Shutdown
        ));
        assert!(!lifecycle_should_stop_usb_media_remap(
            &LifecyclePhase::Post
        ));
        assert!(!lifecycle_should_stop_usb_media_remap(
            &LifecyclePhase::Thaw
        ));
        assert!(!lifecycle_should_stop_usb_media_remap(
            &LifecyclePhase::Boot
        ));
    }

    #[test]
    fn lifecycle_resume_phases_queue_usb_media_remap_retry() {
        assert!(lifecycle_should_queue_usb_media_remap_retry(
            &LifecyclePhase::Post
        ));
        assert!(lifecycle_should_queue_usb_media_remap_retry(
            &LifecyclePhase::Thaw
        ));
        assert!(!lifecycle_should_queue_usb_media_remap_retry(
            &LifecyclePhase::Pre
        ));
        assert!(!lifecycle_should_queue_usb_media_remap_retry(
            &LifecyclePhase::Hibernate
        ));
        assert!(!lifecycle_should_queue_usb_media_remap_retry(
            &LifecyclePhase::Boot
        ));
        assert!(!lifecycle_should_queue_usb_media_remap_retry(
            &LifecyclePhase::Shutdown
        ));
    }

    #[test]
    fn saved_layout_matching_respects_attached_display_count() {
        let single = single_display_layout();
        let dual = dual_display_layout();

        assert!(saved_layout_matches_display_mode(&single, true));
        assert!(!saved_layout_matches_display_mode(&single, false));
        assert!(!saved_layout_matches_display_mode(&dual, true));
        assert!(saved_layout_matches_display_mode(&dual, false));
    }

    #[test]
    fn display_replay_helpers_detect_external_outputs() {
        let internal = dual_display_layout();
        let external = external_display_layout();

        assert!(layout_manages_only_internal_displays(&internal));
        assert!(!layout_has_external_display(&internal));
        assert!(!layout_manages_only_internal_displays(&external));
        assert!(layout_has_external_display(&external));
    }

    fn single_display_layout() -> DisplayLayout {
        DisplayLayout {
            displays: vec![display(PRIMARY_INTERNAL_CONNECTOR, 0, 0, true)],
        }
    }

    fn dual_display_layout() -> DisplayLayout {
        dual_display_layout_with_refresh(60.0)
    }

    fn dual_display_layout_with_refresh(refresh_rate: f64) -> DisplayLayout {
        DisplayLayout {
            displays: vec![
                display_with_refresh(PRIMARY_INTERNAL_CONNECTOR, 0, 0, true, refresh_rate),
                display_with_refresh(SECONDARY_INTERNAL_CONNECTOR, 0, 1200, false, refresh_rate),
            ],
        }
    }

    fn external_display_layout() -> DisplayLayout {
        external_display_layout_with_refresh(60.0)
    }

    fn external_display_layout_with_refresh(external_refresh_rate: f64) -> DisplayLayout {
        DisplayLayout {
            displays: vec![
                display(PRIMARY_INTERNAL_CONNECTOR, 0, 0, true),
                display(SECONDARY_INTERNAL_CONNECTOR, 0, 1200, false),
                display_with_refresh("HDMI-A-1", 1920, 0, false, external_refresh_rate),
            ],
        }
    }

    fn display(connector: &str, x: i32, y: i32, primary: bool) -> DisplayInfo {
        display_with_refresh(connector, x, y, primary, 60.0)
    }

    fn display_with_refresh(
        connector: &str,
        x: i32,
        y: i32,
        primary: bool,
        refresh_rate: f64,
    ) -> DisplayInfo {
        let mode = DisplayMode {
            mode_id: format!("{connector}-mode-{refresh_rate}"),
            backend_mode_id: None,
            width: 1920,
            height: 1200,
            refresh_rate,
        };

        DisplayInfo {
            connector: connector.to_string(),
            width: mode.width,
            height: mode.height,
            refresh_rate: mode.refresh_rate,
            scale: 1.0,
            x,
            y,
            transform: 0,
            primary,
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        }
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
