use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::hardware::duo::{
    is_internal_connector, PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR,
};
use evdev::{AbsoluteAxisType, Device, EventType};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command as TokioCommand;

use crate::ipc::protocol::{
    DaemonRequest, DaemonResponse, Envelope, SessionBackend, SessionCommand, SessionResponse,
};
use crate::models::{DisplayInfo, DisplayLayout, Orientation};
use crate::runtime::{client, compositor, paths, session, state::RuntimeState};

pub async fn run() -> Result<(), String> {
    ensure_user_runtime_dir()?;
    let listener = bind_session_listener(paths::current_user_session_socket_path().as_path())?;

    let backend = BackendReadiness::wait_for_ready_backend().await;
    register_with_daemon(backend).await?;
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
            SessionCommand::SetDockMode {
                attached,
                scale,
                layout,
            } => match DockModePlanner::apply(attached, scale, layout) {
                Ok(()) => SessionResponse::Ack,
                Err(message) => SessionResponse::Error { message },
            },
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

pub(crate) struct BackendReadiness;

impl BackendReadiness {
    async fn wait_for_ready_backend() -> SessionBackend {
        wait_for_ready_backend().await
    }

    #[cfg(test)]
    fn detect_ready_backend_from<F>(hinted: SessionBackend, is_ready: F) -> SessionBackend
    where
        F: FnMut(&session::BackendProbe) -> bool,
    {
        detect_ready_backend_from(hinted, is_ready)
    }
}

fn detect_backend() -> SessionBackend {
    session::detect_backend_from_env()
}

fn detect_ready_backend() -> SessionBackend {
    detect_ready_backend_from(detect_backend(), backend_is_ready)
}

fn detect_ready_backend_from<F>(hinted: SessionBackend, mut is_ready: F) -> SessionBackend
where
    F: FnMut(&session::BackendProbe) -> bool,
{
    for probe in session::backend_probe_sequence(&hinted) {
        if is_ready(&probe) {
            return probe.backend;
        }
    }
    SessionBackend::Unknown
}

fn backend_is_ready(probe: &session::BackendProbe) -> bool {
    if probe.requires_gui_session && !gui_session_env_ready() {
        return false;
    }

    match probe.readiness_runner {
        session::BackendCommandRunner::Compositor => {
            compositor::command_succeeds(probe.readiness_program, probe.readiness_args)
        }
        session::BackendCommandRunner::Niri => {
            compositor::niri_command_succeeds(probe.readiness_args)
        }
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
    R: FnMut(&session::BackendProbe) -> bool,
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

pub(crate) struct DockModePlanner;

impl DockModePlanner {
    fn apply(attached: bool, scale: f64, layout: Option<DisplayLayout>) -> Result<(), String> {
        apply_dock_mode(attached, scale, layout)
    }

    #[cfg(test)]
    fn layout_from_base(layout: &DisplayLayout, attached: bool, scale: f64) -> Option<DisplayLayout> {
        dock_layout_from_base(layout, attached, scale)
    }
}

fn apply_dock_mode(
    attached: bool,
    scale: f64,
    layout: Option<DisplayLayout>,
) -> Result<(), String> {
    let base_layout = layout.or_else(|| crate::hardware::display_config::get_display_layout().ok());

    if let Some(layout) = base_layout
        .as_ref()
        .and_then(|layout| dock_layout_from_base(layout, attached, scale))
    {
        crate::hardware::display_config::apply_display_layout(&layout)?;
    } else {
        log::warn!(
            "No saved or current display layout available for dock replay; using degraded mode-less dock fallback"
        );
        match detect_ready_backend() {
            SessionBackend::Gnome => apply_gnome_dock_mode(attached, scale),
            SessionBackend::Kde => apply_kde_dock_mode(attached),
            SessionBackend::Niri => apply_niri_dock_mode(attached),
            SessionBackend::Unknown => Err("Unsupported session backend for dock mode".into()),
        }?;
    }

    if let Err(err) = send_dock_mode_notification(attached) {
        log::warn!("failed to send dock-mode notification: {err}");
    }

    Ok(())
}

fn stacked_logical_height(display: &DisplayInfo) -> i32 {
    let rotated = display.transform == 90 || display.transform == 270;
    let physical_height = if rotated {
        display.width
    } else {
        display.height
    };
    let scale = display.scale.max(0.1);
    (physical_height as f64 / scale).ceil() as i32
}

fn dock_layout_from_base(
    layout: &DisplayLayout,
    attached: bool,
    scale: f64,
) -> Option<DisplayLayout> {
    let target_scale = scale.max(0.1);
    let mut primary = layout
        .displays
        .iter()
        .find(|display| display.connector == PRIMARY_INTERNAL_CONNECTOR)
        .cloned()
        .or_else(|| {
            layout
                .displays
                .iter()
                .find(|display| is_internal_connector(&display.connector))
                .cloned()
        })?;
    primary.scale = target_scale;
    primary.x = 0;
    primary.y = 0;
    primary.primary = true;

    if attached {
        return Some(DisplayLayout {
            displays: vec![primary],
        });
    }

    let mut secondary = layout
        .displays
        .iter()
        .find(|display| display.connector == SECONDARY_INTERNAL_CONNECTOR)
        .cloned()
        .or_else(|| {
            if primary.connector == SECONDARY_INTERNAL_CONNECTOR {
                None
            } else {
                let mut cloned = primary.clone();
                cloned.connector = SECONDARY_INTERNAL_CONNECTOR.to_string();
                cloned.primary = false;
                Some(cloned)
            }
        })?;
    secondary.scale = target_scale;
    secondary.x = 0;
    secondary.y = stacked_logical_height(&primary);
    secondary.primary = false;

    Some(DisplayLayout {
        displays: vec![primary, secondary],
    })
}

fn gnome_dock_mode_args(attached: bool, scale: f64) -> Vec<String> {
    let scale_str = format!("{scale:.6}");
    if attached {
        vec![
            "set".to_string(),
            "--logical-monitor".to_string(),
            "--primary".to_string(),
            "--scale".to_string(),
            scale_str,
            "--monitor".to_string(),
            PRIMARY_INTERNAL_CONNECTOR.to_string(),
        ]
    } else {
        vec![
            "set".to_string(),
            "--logical-monitor".to_string(),
            "--primary".to_string(),
            "--scale".to_string(),
            scale_str.clone(),
            "--monitor".to_string(),
            PRIMARY_INTERNAL_CONNECTOR.to_string(),
            "--logical-monitor".to_string(),
            "--scale".to_string(),
            scale_str,
            "--monitor".to_string(),
            SECONDARY_INTERNAL_CONNECTOR.to_string(),
            "--below".to_string(),
            PRIMARY_INTERNAL_CONNECTOR.to_string(),
        ]
    }
}

fn apply_gnome_dock_mode(attached: bool, scale: f64) -> Result<(), String> {
    run_command("gdctl", &gnome_dock_mode_args(attached, scale))
}

fn kde_dock_mode_args(attached: bool, primary_logical_height: i64) -> Vec<String> {
    if attached {
        vec![
            format!("output.{PRIMARY_INTERNAL_CONNECTOR}.enable"),
            format!("output.{SECONDARY_INTERNAL_CONNECTOR}.disable"),
        ]
    } else {
        vec![
            format!("output.{PRIMARY_INTERNAL_CONNECTOR}.enable"),
            format!("output.{SECONDARY_INTERNAL_CONNECTOR}.enable"),
            format!("output.{PRIMARY_INTERNAL_CONNECTOR}.position.0,0"),
            format!("output.{SECONDARY_INTERNAL_CONNECTOR}.position.0,{primary_logical_height}"),
        ]
    }
}

fn apply_kde_dock_mode(attached: bool) -> Result<(), String> {
    ensure_gui_session_env("KDE display control")?;
    let primary_logical_height = if attached {
        0
    } else {
        kde_output_logical_size(PRIMARY_INTERNAL_CONNECTOR)
            .unwrap_or((0, 0))
            .1
    };
    run_command(
        "kscreen-doctor",
        &kde_dock_mode_args(attached, primary_logical_height),
    )
}

fn string_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

fn niri_dock_mode_commands(attached: bool, primary_logical_height: i64) -> Vec<Vec<String>> {
    if attached {
        return vec![
            string_args(&["msg", "output", PRIMARY_INTERNAL_CONNECTOR, "on"]),
            string_args(&["msg", "output", SECONDARY_INTERNAL_CONNECTOR, "off"]),
        ];
    }

    vec![
        string_args(&["msg", "output", PRIMARY_INTERNAL_CONNECTOR, "on"]),
        string_args(&["msg", "output", SECONDARY_INTERNAL_CONNECTOR, "on"]),
        string_args(&[
            "msg",
            "output",
            PRIMARY_INTERNAL_CONNECTOR,
            "position",
            "set",
            "0",
            "0",
        ]),
        vec![
            "msg".to_string(),
            "output".to_string(),
            SECONDARY_INTERNAL_CONNECTOR.to_string(),
            "position".to_string(),
            "set".to_string(),
            "0".to_string(),
            primary_logical_height.to_string(),
        ],
    ]
}

fn apply_niri_dock_mode(attached: bool) -> Result<(), String> {
    let primary_logical_height = if attached {
        0
    } else {
        niri_output_logical_size(PRIMARY_INTERNAL_CONNECTOR)
            .unwrap_or((0, 0))
            .1
    };
    for args in niri_dock_mode_commands(attached, primary_logical_height) {
        run_niri_command_args(&args)?;
    }
    Ok(())
}

fn kde_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    ensure_gui_session_env("KDE display query")?;
    compositor::kde_output_logical_size_from_value(&compositor::kscreen_json()?, name)
}

fn niri_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    compositor::niri_output_logical_size_from_value(&compositor::niri_outputs_json()?, name)
}

fn run_command<S: AsRef<str>>(program: &str, args: &[S]) -> Result<(), String> {
    compositor::run_command(program, args)
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

fn run_niri_command(args: &[&str]) -> Result<(), String> {
    compositor::run_niri_command(args)
}

fn run_niri_command_args(args: &[String]) -> Result<(), String> {
    let borrowed_args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_niri_command(&borrowed_args)
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

pub(crate) struct BrightnessSync;

impl BrightnessSync {
    async fn watch() -> Result<(), String> {
        watch_brightness_sync().await
    }
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
    let Some(secondary) = crate::hardware::sysfs::secondary_backlight_dir() else {
        return Ok(());
    };
    let secondary_path = secondary.join("brightness");

    if fs::write(&secondary_path, level.to_string()).is_ok() {
        return Ok(());
    }

    let secondary_path_string = secondary_path.to_string_lossy().into_owned();
    let mut child = Command::new("sudo")
        .args(["/usr/bin/tee", secondary_path_string.as_str()])
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

pub(crate) struct HotkeyWatcher;

impl HotkeyWatcher {
    fn watch() -> Result<(), String> {
        watch_keyboard_hotkeys()
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

    let bl = crate::hardware::sysfs::primary_backlight_dir()
        .ok_or_else(|| "no primary backlight device found".to_string())?;

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

    let brightness_path = bl.join("brightness");
    let brightness_path_string = brightness_path.to_string_lossy().into_owned();
    let output = Command::new("sudo")
        .args(["/usr/bin/tee", brightness_path_string.as_str()])
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

fn runtime_log_info(message: impl AsRef<str>) {
    let message = message.as_ref();
    log::info!("{message}");
    append_runtime_log(format!("session-agent: {message}"));
}

fn runtime_log_warn(message: impl AsRef<str>) {
    let message = message.as_ref();
    log::warn!("{message}");
    append_runtime_log(format!("session-agent: warn: {message}"));
}

fn append_runtime_log(line: String) {
    if let Err(err) = client::request(DaemonRequest::AppendLog { line }) {
        log::warn!("failed to append session-agent log to runtime log: {err}");
    }
}

pub(crate) struct RotationWatcherSupervisor;

impl RotationWatcherSupervisor {
    async fn supervise() {
        supervise_rotation_watcher().await;
    }
}

async fn supervise_rotation_watcher() {
    let mut notified_failure = false;
    let mut consecutive_accelerometer_timeouts = 0;

    loop {
        let restart_delay = match watch_rotation().await {
            Ok(RotationWatchExit::Clean) => {
                consecutive_accelerometer_timeouts = 0;
                runtime_log_warn(format!(
                    "rotation watcher exited cleanly; restarting in {}s",
                    rotation_watcher_restart_delay().as_secs()
                ));
                rotation_watcher_restart_delay()
            }
            Ok(RotationWatchExit::AccelerometerClaimTimeout) => {
                consecutive_accelerometer_timeouts += 1;
                let delay = rotation_watcher_accelerometer_timeout_delay(
                    consecutive_accelerometer_timeouts,
                );
                runtime_log_warn(format!(
                    "monitor-sensor could not claim accelerometer; retrying in {}s",
                    delay.as_secs()
                ));
                delay
            }
            Err(err) => {
                consecutive_accelerometer_timeouts = 0;
                runtime_log_warn(format!(
                    "rotation watcher failed: {err}; restarting in {}s",
                    rotation_watcher_restart_delay().as_secs()
                ));
                if !notified_failure {
                    let _ = send_runtime_notification(
                        "Zenbook Duo Runtime Error",
                        &format!("Rotation watcher failed and will restart: {err}"),
                        true,
                    );
                    notified_failure = true;
                }
                rotation_watcher_restart_delay()
            }
        };

        tokio::time::sleep(restart_delay).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RotationWatchExit {
    Clean,
    AccelerometerClaimTimeout,
}

#[derive(Debug, Default)]
struct MonitorSensorStderrSummary {
    accelerometer_claim_timeout: bool,
}

fn rotation_watcher_restart_delay() -> Duration {
    Duration::from_secs(2)
}

fn rotation_watcher_accelerometer_timeout_delay(consecutive_timeouts: u32) -> Duration {
    let exponent = consecutive_timeouts.saturating_sub(1).min(4);
    Duration::from_secs(30 * 2_u64.pow(exponent))
}

async fn watch_rotation() -> Result<RotationWatchExit, String> {
    let mut child = TokioCommand::new("monitor-sensor")
        .arg("--accel")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start monitor-sensor: {e}"))?;

    runtime_log_info("rotation watcher started monitor-sensor --accel");

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "monitor-sensor stderr unavailable".to_string())?;
    let stderr_task = tokio::spawn(log_monitor_sensor_stderr(stderr));

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
        if let Some(sensor_orientation) = sensor_orientation_value(&line) {
            let invert_sensor_rotation =
                crate::commands::settings::load_settings_local().invert_sensor_rotation;
            match display_orientation_from_sensor_value(sensor_orientation, invert_sensor_rotation)
            {
                Some(orientation) => {
                    runtime_log_info(format!(
                        "monitor-sensor orientation changed: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}"
                    ));
                    if let Err(err) = crate::hardware::display_config::set_orientation(&orientation)
                    {
                        runtime_log_warn(format!(
                            "failed to apply accelerometer orientation: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}: {err}"
                        ));
                    } else {
                        runtime_log_info(format!(
                            "applied accelerometer orientation: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}"
                        ));
                    }
                }
                None => {
                    runtime_log_warn(format!(
                        "monitor-sensor reported unsupported accelerometer orientation: {sensor_orientation}"
                    ));
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting for monitor-sensor: {e}"))?;
    let stderr_summary = match stderr_task.await {
        Ok(summary) => summary,
        Err(err) => {
            runtime_log_warn(format!("monitor-sensor stderr logger task failed: {err}"));
            MonitorSensorStderrSummary::default()
        }
    };

    if status.success() {
        if stderr_summary.accelerometer_claim_timeout {
            Ok(RotationWatchExit::AccelerometerClaimTimeout)
        } else {
            Ok(RotationWatchExit::Clean)
        }
    } else {
        Err(format!("monitor-sensor exited with status {status}"))
    }
}

async fn log_monitor_sensor_stderr(
    stderr: tokio::process::ChildStderr,
) -> MonitorSensorStderrSummary {
    let mut lines = BufReader::new(stderr).lines();
    let mut summary = MonitorSensorStderrSummary::default();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                if monitor_sensor_accelerometer_claim_timeout(line) {
                    summary.accelerometer_claim_timeout = true;
                    continue;
                }

                runtime_log_warn(format!("monitor-sensor stderr: {line}"));
            }
            Ok(None) => break,
            Err(err) => {
                runtime_log_warn(format!("failed reading monitor-sensor stderr: {err}"));
                break;
            }
        }
    }
    summary
}

fn monitor_sensor_accelerometer_claim_timeout(line: &str) -> bool {
    line.contains("Failed to claim accelerometer") && line.contains("Timeout was reached")
}

fn parse_rotation_line(line: &str) -> Option<Orientation> {
    sensor_orientation_value(line)
        .and_then(|value| display_orientation_from_sensor_value(value, false))
}

fn sensor_orientation_value(line: &str) -> Option<&str> {
    line.split("Accelerometer orientation changed:")
        .nth(1)
        .map(str::trim)
}

fn display_orientation_from_sensor_value(
    value: &str,
    invert_sensor_rotation: bool,
) -> Option<Orientation> {
    match (value, invert_sensor_rotation) {
        ("left-up", false) => Some(Orientation::Left),
        ("left-up", true) => Some(Orientation::Right),
        ("right-up", false) => Some(Orientation::Right),
        ("right-up", true) => Some(Orientation::Left),
        ("bottom-up", _) => Some(Orientation::Inverted),
        ("normal", _) => Some(Orientation::Normal),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DisplayMode, RefreshPolicy};
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

    #[test]
    fn rotation_watcher_restart_delay_is_short_and_nonzero() {
        let delay = rotation_watcher_restart_delay();
        assert!(delay >= Duration::from_secs(1));
        assert!(delay <= Duration::from_secs(5));
    }

    #[test]
    fn accelerometer_claim_timeouts_use_bounded_backoff() {
        assert_eq!(
            rotation_watcher_accelerometer_timeout_delay(1),
            Duration::from_secs(30)
        );
        assert_eq!(
            rotation_watcher_accelerometer_timeout_delay(2),
            Duration::from_secs(60)
        );
        assert_eq!(
            rotation_watcher_accelerometer_timeout_delay(5),
            Duration::from_secs(480)
        );
        assert_eq!(
            rotation_watcher_accelerometer_timeout_delay(20),
            Duration::from_secs(480)
        );
    }

    #[test]
    fn recognizes_monitor_sensor_accelerometer_claim_timeout() {
        assert!(monitor_sensor_accelerometer_claim_timeout(
            "** (monitor-sensor:3388909): WARNING **: Failed to claim accelerometer: Timeout was reached"
        ));
        assert!(!monitor_sensor_accelerometer_claim_timeout(
            "** (monitor-sensor:3388909): WARNING **: Failed to claim light sensor: Timeout was reached"
        ));
        assert!(!monitor_sensor_accelerometer_claim_timeout(
            "Accelerometer orientation changed: normal"
        ));
    }

    #[test]
    fn parses_sensor_edge_orientation_as_display_transform() {
        assert_eq!(
            parse_rotation_line("    Accelerometer orientation changed: left-up"),
            Some(Orientation::Left)
        );
        assert_eq!(
            parse_rotation_line("    Accelerometer orientation changed: right-up"),
            Some(Orientation::Right)
        );
        assert_eq!(
            parse_rotation_line("    Accelerometer orientation changed: bottom-up"),
            Some(Orientation::Inverted)
        );
        assert_eq!(
            parse_rotation_line("    Accelerometer orientation changed: normal"),
            Some(Orientation::Normal)
        );
    }

    #[test]
    fn inverted_sensor_rotation_swaps_only_left_and_right() {
        assert_eq!(
            display_orientation_from_sensor_value("left-up", true),
            Some(Orientation::Right)
        );
        assert_eq!(
            display_orientation_from_sensor_value("right-up", true),
            Some(Orientation::Left)
        );
        assert_eq!(
            display_orientation_from_sensor_value("bottom-up", true),
            Some(Orientation::Inverted)
        );
        assert_eq!(
            display_orientation_from_sensor_value("normal", true),
            Some(Orientation::Normal)
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
    fn dock_mode_planner_interface_reuses_existing_refresh_modes_for_attached_replay() {
        let layout = dual_internal_layout(120.0);
        let planned = DockModePlanner::layout_from_base(&layout, false, 1.5).expect("planned layout");

        assert_eq!(planned.displays.len(), 2);
        assert_eq!(planned.displays[0].scale, 1.5);
        assert_eq!(planned.displays[1].scale, 1.5);
    }

    #[test]
    fn backend_readiness_interface_prefers_ready_hint() {
        let backend = BackendReadiness::detect_ready_backend_from(SessionBackend::Gnome, |_| true);

        assert_eq!(backend, SessionBackend::Gnome);
    }

    #[test]
    fn dock_layout_reuses_existing_refresh_modes_for_attached_replay() {
        let layout = dual_internal_layout(120.0);
        let docked = dock_layout_from_base(&layout, true, 1.5).expect("dock layout");

        assert_eq!(docked.displays.len(), 1);
        assert_eq!(docked.displays[0].connector, PRIMARY_INTERNAL_CONNECTOR);
        assert_eq!(docked.displays[0].current_mode.refresh_rate, 120.0);
        assert_eq!(docked.displays[0].scale, 1.5);
    }

    #[test]
    fn dock_layout_clones_primary_mode_for_missing_secondary_panel() {
        let layout = DisplayLayout {
            displays: vec![display(PRIMARY_INTERNAL_CONNECTOR, 120.0, 0, 0, true)],
        };
        let docked = dock_layout_from_base(&layout, false, 1.25).expect("dock layout");

        assert_eq!(docked.displays.len(), 2);
        assert_eq!(docked.displays[1].connector, SECONDARY_INTERNAL_CONNECTOR);
        assert_eq!(docked.displays[1].current_mode.refresh_rate, 120.0);
        assert_eq!(docked.displays[1].scale, 1.25);
        assert_eq!(docked.displays[1].y, 960);
    }

    #[test]
    fn degraded_gnome_dock_mode_arguments_are_mode_less_and_only_used_without_layout_base() {
        assert_eq!(
            gnome_dock_mode_args(true, 1.66),
            string_args(&[
                "set",
                "--logical-monitor",
                "--primary",
                "--scale",
                "1.660000",
                "--monitor",
                PRIMARY_INTERNAL_CONNECTOR,
            ])
        );
        let detached_args = gnome_dock_mode_args(false, 1.66);
        assert_eq!(
            detached_args,
            string_args(&[
                "set",
                "--logical-monitor",
                "--primary",
                "--scale",
                "1.660000",
                "--monitor",
                PRIMARY_INTERNAL_CONNECTOR,
                "--logical-monitor",
                "--scale",
                "1.660000",
                "--monitor",
                SECONDARY_INTERNAL_CONNECTOR,
                "--below",
                PRIMARY_INTERNAL_CONNECTOR,
            ])
        );
        assert!(!detached_args.iter().any(|arg| arg == "--mode"));
    }

    #[test]
    fn degraded_kde_dock_mode_arguments_are_mode_less_and_only_used_without_layout_base() {
        assert_eq!(
            kde_dock_mode_args(true, 1200),
            vec![
                format!("output.{PRIMARY_INTERNAL_CONNECTOR}.enable"),
                format!("output.{SECONDARY_INTERNAL_CONNECTOR}.disable"),
            ]
        );
        let detached_args = kde_dock_mode_args(false, 1200);
        assert_eq!(
            detached_args,
            vec![
                format!("output.{PRIMARY_INTERNAL_CONNECTOR}.enable"),
                format!("output.{SECONDARY_INTERNAL_CONNECTOR}.enable"),
                format!("output.{PRIMARY_INTERNAL_CONNECTOR}.position.0,0"),
                format!("output.{SECONDARY_INTERNAL_CONNECTOR}.position.0,1200"),
            ]
        );
        assert!(!detached_args.iter().any(|arg| arg.contains(".mode.")));
    }

    #[test]
    fn degraded_niri_dock_mode_commands_are_mode_less_and_only_used_without_layout_base() {
        assert_eq!(
            niri_dock_mode_commands(true, 1200),
            vec![
                string_args(&["msg", "output", PRIMARY_INTERNAL_CONNECTOR, "on"]),
                string_args(&["msg", "output", SECONDARY_INTERNAL_CONNECTOR, "off"]),
            ]
        );
        let detached_commands = niri_dock_mode_commands(false, 1200);
        assert_eq!(
            detached_commands,
            vec![
                string_args(&["msg", "output", PRIMARY_INTERNAL_CONNECTOR, "on"]),
                string_args(&["msg", "output", SECONDARY_INTERNAL_CONNECTOR, "on"]),
                string_args(&[
                    "msg",
                    "output",
                    PRIMARY_INTERNAL_CONNECTOR,
                    "position",
                    "set",
                    "0",
                    "0",
                ]),
                string_args(&[
                    "msg",
                    "output",
                    SECONDARY_INTERNAL_CONNECTOR,
                    "position",
                    "set",
                    "0",
                    "1200",
                ]),
            ]
        );
        assert!(!detached_commands.iter().flatten().any(|arg| arg == "mode"));
    }

    #[test]
    fn detect_ready_backend_prefers_hint_when_ready() {
        let ready = detect_ready_backend_from(SessionBackend::Kde, |probe| {
            probe.backend == SessionBackend::Kde
        });
        assert_eq!(ready, SessionBackend::Kde);
    }

    #[test]
    fn backend_readiness_metadata_preserves_existing_commands() {
        let gnome = session::backend_probe(&SessionBackend::Gnome).expect("gnome probe");
        assert_eq!(gnome.readiness_program, "gdctl");
        assert_eq!(gnome.readiness_args, ["show"]);
        assert!(gnome.requires_gui_session);

        let kde = session::backend_probe(&SessionBackend::Kde).expect("kde probe");
        assert_eq!(kde.readiness_program, "kscreen-doctor");
        assert_eq!(kde.readiness_args, ["-j"]);
        assert!(kde.requires_gui_session);

        let niri = session::backend_probe(&SessionBackend::Niri).expect("niri probe");
        assert_eq!(niri.readiness_program, "niri");
        assert_eq!(niri.readiness_args, ["msg", "--json", "outputs"]);
        assert!(!niri.requires_gui_session);
    }

    #[test]
    fn detect_ready_backend_falls_through_to_other_ready_backend() {
        let ready = detect_ready_backend_from(SessionBackend::Unknown, |probe| {
            probe.backend == SessionBackend::Niri
        });
        assert_eq!(ready, SessionBackend::Niri);
    }

    #[test]
    fn detect_ready_backend_uses_hinted_order_then_fallback_metadata() {
        let mut seen = Vec::new();
        let ready = detect_ready_backend_from(SessionBackend::Kde, |probe| {
            seen.push(probe.backend);
            probe.backend == SessionBackend::Gnome
        });

        assert_eq!(ready, SessionBackend::Gnome);
        assert_eq!(
            seen,
            vec![
                SessionBackend::Kde,
                SessionBackend::Niri,
                SessionBackend::Gnome,
            ]
        );
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
            |probe| {
                attempts += 1;
                probe.backend == SessionBackend::Kde && attempts >= 3
            },
            Duration::from_millis(1),
            Duration::from_millis(1),
        )
        .await;

        assert_eq!(backend, SessionBackend::Kde);
        assert!(attempts >= 3);
    }

    fn dual_internal_layout(refresh_rate: f64) -> DisplayLayout {
        DisplayLayout {
            displays: vec![
                display(PRIMARY_INTERNAL_CONNECTOR, refresh_rate, 0, 0, true),
                display(SECONDARY_INTERNAL_CONNECTOR, refresh_rate, 0, 1200, false),
            ],
        }
    }

    fn display(connector: &str, refresh_rate: f64, x: i32, y: i32, primary: bool) -> DisplayInfo {
        let mode = DisplayMode {
            mode_id: format!("1920x1200@{refresh_rate}"),
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
