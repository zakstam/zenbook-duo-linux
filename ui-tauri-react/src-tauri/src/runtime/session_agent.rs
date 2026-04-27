use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use evdev::{AbsoluteAxisType, Device, EventType};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command as TokioCommand;

use crate::ipc::protocol::{
    DaemonRequest, DaemonResponse, Envelope, SessionBackend, SessionCommand, SessionResponse,
};
use crate::models::Orientation;
use crate::runtime::{paths, state::RuntimeState};

pub async fn run() -> Result<(), String> {
    ensure_user_runtime_dir()?;
    let listener = bind_session_listener(paths::current_user_session_socket_path().as_path())?;

    let backend = wait_for_ready_backend().await;
    register_with_daemon(backend).await?;
    tokio::spawn(async {
        if let Err(err) = watch_rotation().await {
            log::warn!("session-agent rotation watcher failed: {err}");
            let _ = send_runtime_notification(
                "Zenbook Duo Runtime Error",
                &format!("Rotation watcher failed: {err}"),
                true,
            );
        }
    });
    tokio::spawn(async {
        if let Err(err) = watch_brightness_sync().await {
            log::warn!("session-agent brightness watcher failed: {err}");
            let _ = send_runtime_notification(
                "Zenbook Duo Runtime Error",
                &format!("Brightness sync watcher failed: {err}"),
                true,
            );
        }
    });
    tokio::task::spawn_blocking(|| {
        if let Err(err) = watch_keyboard_hotkeys() {
            log::warn!("session-agent keyboard hotkey watcher failed: {err}");
            let _ = send_runtime_notification(
                "Zenbook Duo Runtime Error",
                &format!("Keyboard hotkey watcher failed: {err}"),
                true,
            );
        }
    });

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("Failed to accept session-agent client: {e}"))?;
        tokio::spawn(async move {
            if let Err(err) = handle_session_command(stream).await {
                log::warn!("session-agent client error: {err}");
            }
        });
    }
}

async fn register_with_daemon(backend: SessionBackend) -> Result<(), String> {
    let stream = UnixStream::connect(paths::daemon_socket_path())
        .await
        .map_err(|e| format!("Failed to connect to daemon socket: {e}"))?;
    let (reader, mut writer) = stream.into_split();

    let request = Envelope::new(DaemonRequest::RegisterSessionAgent {
        session_id: detect_session_id(),
        backend,
        socket_path: paths::current_user_session_socket_path()
            .to_string_lossy()
            .into_owned(),
    });
    let line = serde_json::to_string(&request)
        .map_err(|e| format!("Failed to encode registration: {e}"))?;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("Failed to send registration: {e}"))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|e| format!("Failed to terminate registration: {e}"))?;

    let mut lines = BufReader::new(reader).lines();
    let reply = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed reading daemon registration reply: {e}"))?
        .ok_or_else(|| "Daemon closed before replying to session registration".to_string())?;

    let envelope: Envelope<DaemonResponse> = serde_json::from_str(&reply)
        .map_err(|e| format!("Invalid daemon registration response: {e}"))?;
    match envelope.payload {
        DaemonResponse::Ack => Ok(()),
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!(
            "Unexpected daemon registration response: {other:?}"
        )),
    }
}

async fn handle_session_command(stream: UnixStream) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed to read session command: {e}"))?
    {
        let envelope: Envelope<SessionCommand> = serde_json::from_str(&line)
            .map_err(|e| format!("Invalid session command JSON: {e}"))?;
        let response = match envelope.payload {
            SessionCommand::GetDisplayLayout => {
                match crate::hardware::display_config::get_display_layout() {
                    Ok(layout) => SessionResponse::DisplayLayout { layout },
                    Err(message) => SessionResponse::Error { message },
                }
            }
            SessionCommand::SetDockMode { attached, scale } => {
                match apply_dock_mode(attached, scale) {
                    Ok(()) => SessionResponse::Ack,
                    Err(message) => SessionResponse::Error { message },
                }
            }
            SessionCommand::ApplyDisplayLayout { layout } => {
                match crate::hardware::display_config::apply_display_layout(&layout) {
                    Ok(()) => SessionResponse::Ack,
                    Err(message) => SessionResponse::Error { message },
                }
            }
            SessionCommand::SetOrientation { orientation } => {
                match crate::hardware::display_config::set_orientation(&orientation) {
                    Ok(()) => SessionResponse::Ack,
                    Err(message) => SessionResponse::Error { message },
                }
            }
            SessionCommand::ShowNotification {
                title,
                message,
                urgent,
            } => match send_runtime_notification(&title, &message, urgent) {
                Ok(()) => SessionResponse::Ack,
                Err(message) => SessionResponse::Error { message },
            },
            SessionCommand::OpenEmojiPicker => SessionResponse::Ack,
        };
        let line = serde_json::to_string(&Envelope::new(response))
            .map_err(|e| format!("Failed to encode session response: {e}"))?;
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("Failed to write session response: {e}"))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|e| format!("Failed to terminate session response: {e}"))?;
    }

    Ok(())
}

fn bind_session_listener(path: &Path) -> Result<UnixListener, String> {
    remove_stale_socket(path);
    UnixListener::bind(path).map_err(|e| format!("Failed to bind session agent socket: {e}"))
}

fn niri_runtime_dir() -> Option<std::path::PathBuf> {
    env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
}

fn resolve_niri_socket() -> Option<std::path::PathBuf> {
    let env_socket = env::var_os("NIRI_SOCKET").map(std::path::PathBuf::from);
    resolve_niri_socket_from(env_socket.as_deref(), niri_runtime_dir().as_deref())
}

fn resolve_niri_socket_from(
    env_socket: Option<&Path>,
    runtime_dir: Option<&Path>,
) -> Option<std::path::PathBuf> {
    if let Some(env_socket) = env_socket {
        if env_socket.exists() {
            return Some(env_socket.to_path_buf());
        }
    }

    let runtime_dir = runtime_dir?;
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;

    for entry in fs::read_dir(runtime_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if !name.starts_with("niri.") || !name.ends_with(".sock") {
            continue;
        }

        let metadata = entry.metadata().ok()?;
        let modified = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &newest {
            Some((current_modified, _)) if *current_modified >= modified => {}
            _ => newest = Some((modified, path)),
        }
    }

    newest.map(|(_, path)| path)
}

fn ensure_user_runtime_dir() -> Result<(), String> {
    crate::runtime::runtime_dir::ensure_current_user_runtime_dir()
}

fn remove_stale_socket(path: &Path) {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_socket() {
            let _ = fs::remove_file(path);
        }
    }
}

fn detect_session_id() -> String {
    env::var("XDG_SESSION_ID").unwrap_or_else(|_| "unknown-session".to_string())
}

fn detect_backend() -> SessionBackend {
    let current = env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| env::var("XDG_SESSION_DESKTOP"))
        .or_else(|_| env::var("DESKTOP_SESSION"))
        .unwrap_or_default()
        .to_lowercase();
    detect_backend_from(current, resolve_niri_socket().is_some())
}

fn detect_backend_from(current: String, has_niri_socket: bool) -> SessionBackend {
    if current.contains("gnome") {
        SessionBackend::Gnome
    } else if current.contains("plasma") || current.contains("kde") {
        SessionBackend::Kde
    } else if current.contains("niri") || has_niri_socket {
        SessionBackend::Niri
    } else {
        SessionBackend::Unknown
    }
}

fn detect_ready_backend() -> SessionBackend {
    detect_ready_backend_from(detect_backend(), backend_is_ready)
}

fn detect_ready_backend_from<F>(hinted: SessionBackend, mut is_ready: F) -> SessionBackend
where
    F: FnMut(SessionBackend) -> bool,
{
    for backend in backend_probe_order(hinted) {
        if is_ready(backend.clone()) {
            return backend;
        }
    }
    SessionBackend::Unknown
}

fn backend_probe_order(hinted: SessionBackend) -> Vec<SessionBackend> {
    let mut order = Vec::new();
    if hinted != SessionBackend::Unknown {
        order.push(hinted);
    }
    for backend in [
        SessionBackend::Niri,
        SessionBackend::Gnome,
        SessionBackend::Kde,
    ] {
        if !order.contains(&backend) {
            order.push(backend);
        }
    }
    order
}

fn backend_is_ready(backend: SessionBackend) -> bool {
    match backend {
        SessionBackend::Gnome => {
            if !gui_session_env_ready() {
                return false;
            }
            Command::new("gdctl")
                .arg("show")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
        SessionBackend::Kde => {
            if !gui_session_env_ready() {
                return false;
            }
            Command::new("kscreen-doctor")
                .arg("-j")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
        SessionBackend::Niri => build_niri_command(&["msg", "--json", "outputs"])
            .and_then(|mut command| {
                command
                    .output()
                    .map_err(|e| format!("Failed to run niri readiness probe: {e}"))
            })
            .map(|output| output.status.success())
            .unwrap_or(false),
        SessionBackend::Unknown => false,
    }
}

fn gui_session_env_ready() -> bool {
    let has_runtime_dir = env::var_os("XDG_RUNTIME_DIR").is_some();
    let has_wayland = env::var_os("WAYLAND_DISPLAY").is_some();
    let has_x11 = env::var_os("DISPLAY").is_some();
    has_runtime_dir && (has_wayland || has_x11)
}

async fn wait_for_ready_backend() -> SessionBackend {
    wait_for_ready_backend_with(
        detect_backend,
        backend_is_ready,
        Duration::from_secs(15),
        Duration::from_millis(500),
    )
    .await
}

async fn wait_for_ready_backend_with<D, R>(
    mut detect_hint: D,
    mut is_ready: R,
    first_notice_after: Duration,
    retry_delay: Duration,
) -> SessionBackend
where
    D: FnMut() -> SessionBackend,
    R: FnMut(SessionBackend) -> bool,
{
    let first_notice_at = tokio::time::Instant::now() + first_notice_after;
    let mut warned_after_first_timeout = false;

    loop {
        let hinted_backend = detect_hint();
        let backend = detect_ready_backend_from(hinted_backend, &mut is_ready);
        if backend != SessionBackend::Unknown {
            return backend;
        }

        if !warned_after_first_timeout && tokio::time::Instant::now() >= first_notice_at {
            log::warn!(
                "No supported session backend became ready before timeout; continuing to wait"
            );
            warned_after_first_timeout = true;
        }

        tokio::time::sleep(retry_delay).await;
    }
}

fn apply_dock_mode(attached: bool, scale: f64) -> Result<(), String> {
    match detect_ready_backend() {
        SessionBackend::Gnome => apply_gnome_dock_mode(attached, scale),
        SessionBackend::Kde => apply_kde_dock_mode(attached),
        SessionBackend::Niri => apply_niri_dock_mode(attached),
        SessionBackend::Unknown => Err("Unsupported session backend for dock mode".into()),
    }?;

    if let Err(err) = send_dock_mode_notification(attached) {
        log::warn!("failed to send dock-mode notification: {err}");
    }

    Ok(())
}

fn apply_gnome_dock_mode(attached: bool, scale: f64) -> Result<(), String> {
    let scale_str = format!("{scale:.6}");
    let args = if attached {
        vec![
            "set".to_string(),
            "--logical-monitor".to_string(),
            "--primary".to_string(),
            "--scale".to_string(),
            scale_str,
            "--monitor".to_string(),
            "eDP-1".to_string(),
        ]
    } else {
        vec![
            "set".to_string(),
            "--logical-monitor".to_string(),
            "--primary".to_string(),
            "--scale".to_string(),
            scale_str.clone(),
            "--monitor".to_string(),
            "eDP-1".to_string(),
            "--logical-monitor".to_string(),
            "--scale".to_string(),
            scale_str,
            "--monitor".to_string(),
            "eDP-2".to_string(),
            "--below".to_string(),
            "eDP-1".to_string(),
        ]
    };
    run_command("gdctl", &args)
}

fn apply_kde_dock_mode(attached: bool) -> Result<(), String> {
    ensure_gui_session_env("KDE display control")?;
    if attached {
        run_command(
            "kscreen-doctor",
            &["output.eDP-1.enable", "output.eDP-2.disable"],
        )
    } else {
        let (_, h) = kde_output_logical_size("eDP-1").unwrap_or((0, 0));
        run_command(
            "kscreen-doctor",
            &[
                "output.eDP-1.enable",
                "output.eDP-2.enable",
                "output.eDP-1.position.0,0",
                &format!("output.eDP-2.position.0,{h}"),
            ],
        )
    }
}

fn apply_niri_dock_mode(attached: bool) -> Result<(), String> {
    if attached {
        run_niri_command(&["msg", "output", "eDP-1", "on"])?;
        run_niri_command(&["msg", "output", "eDP-2", "off"])
    } else {
        run_niri_command(&["msg", "output", "eDP-1", "on"])?;
        run_niri_command(&["msg", "output", "eDP-2", "on"])?;
        let (_, h) = niri_output_logical_size("eDP-1").unwrap_or((0, 0));
        run_niri_command(&["msg", "output", "eDP-1", "position", "set", "0", "0"])?;
        run_niri_command(&[
            "msg",
            "output",
            "eDP-2",
            "position",
            "set",
            "0",
            &h.to_string(),
        ])
    }
}

fn kde_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    ensure_gui_session_env("KDE display query")?;
    let output = Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .map_err(|e| format!("Failed to run kscreen-doctor: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid kscreen JSON: {e}"))?;
    let outputs = value
        .get("outputs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing KDE outputs array".to_string())?;
    for output in outputs {
        if output.get("name").and_then(|v| v.as_str()) == Some(name) {
            let size = output
                .get("size")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "Missing KDE output size".to_string())?;
            let scale = output.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let width = size.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
            let height = size.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
            return Ok((
                (width as f64 / scale).round() as i64,
                (height as f64 / scale).round() as i64,
            ));
        }
    }
    Err(format!("KDE output {name} not found"))
}

fn niri_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    let output = build_niri_command(&["msg", "--json", "outputs"])?
        .output()
        .map_err(|e| format!("Failed to run niri msg: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid niri JSON: {e}"))?;
    let outputs = if let Some(arr) = value.as_array() {
        arr.clone()
    } else if let Some(obj) = value.as_object() {
        obj.values().cloned().collect()
    } else {
        return Err("Unexpected niri outputs shape".into());
    };
    for output in outputs {
        if output.get("name").and_then(|v| v.as_str()) == Some(name) {
            let logical = output
                .get("logical")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "Missing niri logical size".to_string())?;
            let width = logical.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
            let height = logical.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
            return Ok((width, height));
        }
    }
    Err(format!("Niri output {name} not found"))
}

fn run_command<S: AsRef<str>>(program: &str, args: &[S]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .map_err(|e| format!("Failed to run {program}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn ensure_gui_session_env(action: &str) -> Result<(), String> {
    if gui_session_env_ready() {
        Ok(())
    } else {
        Err(format!(
            "{action} requires XDG_RUNTIME_DIR and either WAYLAND_DISPLAY or DISPLAY"
        ))
    }
}

fn build_niri_command(args: &[&str]) -> Result<Command, String> {
    let mut command = Command::new("niri");
    command.args(args);
    if let Some(socket) = resolve_niri_socket() {
        command.env("NIRI_SOCKET", socket);
    }
    Ok(command)
}

fn run_niri_command(args: &[&str]) -> Result<(), String> {
    let output = build_niri_command(args)?
        .output()
        .map_err(|e| format!("Failed to run niri msg: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn dock_mode_notification_message(attached: bool) -> &'static str {
    if attached {
        "Keyboard attached: bottom screen disabled"
    } else {
        "Keyboard detached: bottom screen enabled"
    }
}

fn send_dock_mode_notification(attached: bool) -> Result<(), String> {
    send_runtime_notification(
        "Zenbook Duo Control",
        dock_mode_notification_message(attached),
        false,
    )
}

fn send_runtime_notification(title: &str, message: &str, urgent: bool) -> Result<(), String> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "XDG_RUNTIME_DIR is not set for runtime notifications".to_string())?;
    let bus_address = env::var("DBUS_SESSION_BUS_ADDRESS")
        .unwrap_or_else(|_| format!("unix:path={runtime_dir}/bus"));
    let urgency = if urgent { "critical" } else { "normal" };

    Command::new("notify-send")
        .args([
            "-a",
            "Zenbook Duo Control",
            "-u",
            urgency,
            "-i",
            "input-keyboard",
            title,
            message,
        ])
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .env("DBUS_SESSION_BUS_ADDRESS", bus_address)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch runtime notification: {e}"))
}

async fn watch_brightness_sync() -> Result<(), String> {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut last_seen: Option<u32> = None;

    loop {
        interval.tick().await;

        if !brightness_sync_enabled() || keyboard_attached_from_runtime() {
            continue;
        }

        let level = crate::hardware::sysfs::read_display_brightness();
        if last_seen == Some(level) {
            continue;
        }

        sync_secondary_brightness(level)?;
        last_seen = Some(level);
    }
}

fn brightness_sync_enabled() -> bool {
    crate::commands::settings::load_settings_local().sync_brightness
}

fn keyboard_attached_from_runtime() -> bool {
    let Ok(raw) = fs::read_to_string(paths::state_file_path()) else {
        return false;
    };
    let Ok(state) = serde_json::from_str::<RuntimeState>(&raw) else {
        return false;
    };
    state.status.keyboard_attached
}

fn sync_secondary_brightness(level: u32) -> Result<(), String> {
    const SECONDARY_PATH: &str = "/sys/class/backlight/card1-eDP-2-backlight/brightness";

    if !Path::new(SECONDARY_PATH).exists() {
        return Ok(());
    }

    if fs::write(SECONDARY_PATH, level.to_string()).is_ok() {
        return Ok(());
    }

    let mut child = Command::new("sudo")
        .args(["/usr/bin/tee", SECONDARY_PATH])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run sudo tee for brightness sync: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(level.to_string().as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|e| format!("Failed to write brightness sync value: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for brightness sync helper: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Brightness sync failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn watch_keyboard_hotkeys() -> Result<(), String> {
    loop {
        let device_paths = find_keyboard_abs_devices()?;
        if device_paths.is_empty() {
            std::thread::sleep(Duration::from_secs(5));
            continue;
        }

        let mut opened = Vec::new();
        for path in &device_paths {
            match Device::open(path) {
                Ok(device) => opened.push((path.clone(), device)),
                Err(err) => {
                    log::warn!("failed to open {}: {err}", path.display());
                }
            }
        }

        if opened.is_empty() {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }

        loop {
            let mut lost_device = false;
            for (path, device) in &mut opened {
                match device.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            if event.event_type() == EventType::ABSOLUTE
                                && is_hotkey_abs_code(event.code())
                            {
                                let value = event.value();
                                if let Err(err) = handle_abs_misc_value(value) {
                                    log::warn!(
                                        "failed to handle hotkey ABS value {} (code {}) from {}: {}",
                                        value,
                                        event.code(),
                                        path.display(),
                                        err
                                    );
                                }
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!("keyboard hotkey device lost ({}): {err}", path.display());
                        lost_device = true;
                        break;
                    }
                }
            }

            if lost_device {
                break;
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        std::thread::sleep(Duration::from_secs(2));
    }
}

fn find_keyboard_abs_devices() -> Result<Vec<std::path::PathBuf>, String> {
    let mut devices = Vec::new();
    let entries =
        fs::read_dir("/dev/input").map_err(|e| format!("Failed to read /dev/input: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }

        let Ok(device) = Device::open(&path) else {
            continue;
        };
        let device_name = device
            .name()
            .map(str::to_string)
            .unwrap_or_default()
            .to_lowercase();
        if !device_name.contains("zenbook duo keyboard") && !device_name.contains("asus_duo") {
            continue;
        }

        let Some(abs_axes) = device.supported_absolute_axes() else {
            continue;
        };
        if supported_hotkey_abs_codes()
            .into_iter()
            .any(|axis| abs_axes.contains(axis))
        {
            devices.push(path);
        }
    }

    Ok(devices)
}

fn supported_hotkey_abs_codes() -> [AbsoluteAxisType; 2] {
    [AbsoluteAxisType(0x28), AbsoluteAxisType::ABS_VOLUME]
}

fn is_hotkey_abs_code(code: u16) -> bool {
    supported_hotkey_abs_codes()
        .into_iter()
        .any(|axis| axis.0 == code)
}

fn handle_abs_misc_value(value: i32) -> Result<(), String> {
    match value {
        199 => cycle_backlight(),
        16 => step_brightness("down"),
        32 => step_brightness("up"),
        _ => Ok(()),
    }
}

fn cycle_backlight() -> Result<(), String> {
    let current = crate::hardware::sysfs::read_backlight_level();
    let next = match current {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => 0,
    };
    crate::commands::backlight::set_backlight_daemon_first(next)
}

fn step_brightness(direction: &str) -> Result<(), String> {
    if let Ok(output) = Command::new("brightnessctl")
        .args(["set", if direction == "up" { "5%+" } else { "5%-" }])
        .output()
    {
        if output.status.success() {
            return Ok(());
        }
    }

    let bl = Path::new("/sys/class/backlight/intel_backlight");
    if !bl.exists() {
        return Err("no intel_backlight device found".into());
    }

    let max = fs::read_to_string(bl.join("max_brightness"))
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(0);
    let current = fs::read_to_string(bl.join("brightness"))
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(0);
    let step = (max / 20).max(1);
    let next = if direction == "up" {
        (current + step).min(max)
    } else {
        (current - step).max(0)
    };

    if fs::write(bl.join("brightness"), next.to_string()).is_ok() {
        return Ok(());
    }

    let output = Command::new("sudo")
        .args([
            "/usr/bin/tee",
            "/sys/class/backlight/intel_backlight/brightness",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run sudo tee for brightness step: {e}"))?;

    let mut child = output;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(next.to_string().as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|e| format!("Failed to write brightness value: {e}"))?;
    }
    let result = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for brightness helper: {e}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "brightness step failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
    }
}

async fn watch_rotation() -> Result<(), String> {
    let mut child = TokioCommand::new("monitor-sensor")
        .arg("--accel")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start monitor-sensor: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "monitor-sensor stdout unavailable".to_string())?;
    let mut lines = BufReader::new(stdout).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed reading monitor-sensor output: {e}"))?
    {
        if let Some(orientation) = parse_rotation_line(&line) {
            if let Err(err) = crate::hardware::display_config::set_orientation(&orientation) {
                log::warn!("failed to apply accelerometer orientation: {err}");
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting for monitor-sensor: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("monitor-sensor exited with status {status}"))
    }
}

fn parse_rotation_line(line: &str) -> Option<Orientation> {
    let value = line
        .split("Accelerometer orientation changed:")
        .nth(1)?
        .trim();
    match value {
        "left-up" => Some(Orientation::Left),
        "right-up" => Some(Orientation::Right),
        "bottom-up" => Some(Orientation::Inverted),
        "normal" => Some(Orientation::Normal),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn watches_both_known_hotkey_abs_codes() {
        assert!(is_hotkey_abs_code(0x28));
        assert!(is_hotkey_abs_code(AbsoluteAxisType::ABS_VOLUME.0));
        assert!(!is_hotkey_abs_code(0x27));
    }

    #[test]
    fn dock_mode_notification_mentions_bottom_screen_enabled_on_detach() {
        assert_eq!(
            dock_mode_notification_message(false),
            "Keyboard detached: bottom screen enabled"
        );
    }

    #[test]
    fn dock_mode_notification_mentions_bottom_screen_disabled_on_attach() {
        assert_eq!(
            dock_mode_notification_message(true),
            "Keyboard attached: bottom screen disabled"
        );
    }

    #[tokio::test]
    async fn bind_session_listener_creates_socket_before_registration() {
        let socket_path = unique_test_socket_path("session-listener");
        let listener = bind_session_listener(&socket_path).expect("bind test session listener");

        assert!(
            socket_path.exists(),
            "listener should create the socket path"
        );

        drop(listener);
        let _ = fs::remove_file(&socket_path);
    }

    #[test]
    fn resolve_niri_socket_prefers_existing_env_socket() {
        let runtime_dir = temp_runtime_dir("niri-env");
        let env_socket = runtime_dir.join("niri.wayland-1.env.sock");
        let listener =
            std::os::unix::net::UnixListener::bind(&env_socket).expect("bind env socket");

        let resolved =
            resolve_niri_socket_from(Some(env_socket.as_path()), Some(runtime_dir.as_path()))
                .expect("resolve niri socket");

        assert_eq!(resolved, env_socket);

        drop(listener);
        let _ = fs::remove_file(&env_socket);
        let _ = fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn resolve_niri_socket_falls_back_to_latest_runtime_socket() {
        let runtime_dir = temp_runtime_dir("niri-fallback");
        let older_socket = runtime_dir.join("niri.wayland-1.older.sock");
        let newer_socket = runtime_dir.join("niri.wayland-1.newer.sock");
        let older_listener =
            std::os::unix::net::UnixListener::bind(&older_socket).expect("bind older socket");
        std::thread::sleep(Duration::from_millis(10));
        let newer_listener =
            std::os::unix::net::UnixListener::bind(&newer_socket).expect("bind newer socket");

        let resolved = resolve_niri_socket_from(None, Some(runtime_dir.as_path()))
            .expect("resolve niri socket");

        assert_eq!(resolved, newer_socket);

        drop(older_listener);
        drop(newer_listener);
        let _ = fs::remove_file(&older_socket);
        let _ = fs::remove_file(&newer_socket);
        let _ = fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn detect_backend_falls_back_to_niri_when_socket_is_available() {
        assert_eq!(
            detect_backend_from("".to_string(), true),
            SessionBackend::Niri
        );
    }

    #[test]
    fn detect_backend_remains_unknown_without_desktop_hint_or_socket() {
        assert_eq!(
            detect_backend_from("".to_string(), false),
            SessionBackend::Unknown
        );
    }

    #[test]
    fn detect_ready_backend_prefers_hint_when_ready() {
        let ready = detect_ready_backend_from(SessionBackend::Kde, |backend| {
            backend == SessionBackend::Kde
        });
        assert_eq!(ready, SessionBackend::Kde);
    }

    #[test]
    fn detect_ready_backend_falls_through_to_other_ready_backend() {
        let ready = detect_ready_backend_from(SessionBackend::Unknown, |backend| {
            backend == SessionBackend::Niri
        });
        assert_eq!(ready, SessionBackend::Niri);
    }

    #[test]
    fn detect_ready_backend_returns_unknown_when_nothing_is_ready() {
        let ready = detect_ready_backend_from(SessionBackend::Gnome, |_| false);
        assert_eq!(ready, SessionBackend::Unknown);
    }

    #[tokio::test]
    async fn wait_for_ready_backend_keeps_retrying_after_initial_timeout() {
        let mut attempts = 0;
        let backend = wait_for_ready_backend_with(
            || SessionBackend::Kde,
            |backend| {
                attempts += 1;
                backend == SessionBackend::Kde && attempts >= 3
            },
            Duration::from_millis(1),
            Duration::from_millis(1),
        )
        .await;

        assert_eq!(backend, SessionBackend::Kde);
        assert!(attempts >= 3);
    }

    fn unique_test_socket_path(label: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("zenbook-duo-{label}-{nanos}-{id}.sock"))
    }

    fn temp_runtime_dir(label: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("zenbook-duo-{label}-{nanos}-{id}"));
        fs::create_dir_all(&dir).expect("create temp runtime dir");
        dir
    }
}
