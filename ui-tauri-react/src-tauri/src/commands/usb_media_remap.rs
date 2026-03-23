use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::Command;
use std::time::Duration;

use nix::unistd::{Uid, User};

use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;
use crate::runtime::paths;

const HELPER_BINARY_NAME: &str = "zenbook-duo-usb-remap-helper";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbMediaRemapStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub paused: bool,
}

#[tauri::command]
pub fn usb_media_remap_status() -> UsbMediaRemapStatus {
    daemon_first_status()
}

#[tauri::command]
pub async fn usb_media_remap_start() -> Result<(), String> {
    daemon_first_start()
}

#[tauri::command]
pub async fn usb_media_remap_stop() -> Result<(), String> {
    daemon_first_stop()
}

pub fn get_status() -> UsbMediaRemapStatus {
    let pid = read_pid(&pid_path());
    if let Some(pid) = pid {
        if is_pid_running(pid) {
            return UsbMediaRemapStatus {
                running: true,
                pid: Some(pid),
                paused: std::path::Path::new(&pause_file_path()).exists(),
            };
        }
        let _ = fs::remove_file(pid_path());
    }

    UsbMediaRemapStatus {
        running: false,
        pid: None,
        paused: false,
    }
}

pub fn start_remap() -> Result<(), String> {
    if get_status().running {
        return Ok(());
    }

    // `pkexec` can take a few seconds before the auth prompt appears, and the user then needs
    // time to enter their password. Treat this as part of "startup" so the UI doesn't show a
    // spurious timeout error before authentication is even possible.
    const START_TIMEOUT_SECS: u64 = 90;

    let pid_path = pid_path();
    ensure_duo_dir_for_pid(&pid_path)?;

    let helper_path = helper_binary_path()?;
    let user = current_username().map_err(log_error)?;

    let mut cmd = if running_as_root() {
        Command::new(&helper_path)
    } else {
        let mut cmd = Command::new("pkexec");
        cmd.arg(&helper_path);
        cmd
    };
    cmd.arg("--pid-file")
        .arg(&pid_path)
        .arg("--user")
        .arg(user);

    start_remap_spawn_and_wait(cmd, Duration::from_secs(START_TIMEOUT_SECS))
}

pub fn stop_remap() -> Result<(), String> {
    // Clean up pause file on stop.
    let _ = fs::remove_file(pause_file_path());

    let pid_files = running_pid_files();
    if pid_files.is_empty() {
        // Nothing to stop.
        return Ok(());
    }

    let helper_path = helper_binary_path()?;

    for pid_path in pid_files {
        ensure_duo_dir_for_pid(&pid_path)?;

        let mut cmd = if running_as_root() {
            Command::new(&helper_path)
        } else {
            let mut cmd = Command::new("pkexec");
            cmd.arg(&helper_path);
            cmd
        };
        cmd.arg("--stop").arg("--pid-file").arg(&pid_path);

        let status = cmd
            .status()
            .map_err(|e| log_error(format!("Failed to stop remapper (pkexec): {e}")))?;
        if !status.success() {
            return Err(log_error(format!(
                "Failed to stop remapper (pkexec exited with {status})"
            )));
        }
    }

    // Wait briefly for pid-file removal / process exit so the UI status doesn't bounce.
    for _ in 0..30 {
        if !get_status().running {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

fn start_remap_spawn_and_wait(mut cmd: Command, timeout: Duration) -> Result<(), String> {
    let mut child = cmd
        .spawn()
        .map_err(|e| log_error(format!("Failed to start remapper (pkexec): {e}")))?;

    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if get_status().running {
            // Ensure we reap the pkexec child process when it eventually exits (avoid zombies).
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(log_error(format!(
                "Remapper failed to start (pkexec exited with {status})"
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Timeout elapsed. Keep pkexec running in the background (it might still be waiting for auth).
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Err(log_error(format!(
        "Timed out waiting for remapper to start (waited {}s). If an authentication prompt appeared, complete it and try again.",
        timeout.as_secs()
    )))
}

fn read_pid(path: &str) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

fn is_pid_running(pid: u32) -> bool {
    let res = unsafe { libc::kill(pid as i32, 0) };
    if res == 0 {
        return true;
    }

    let err = std::io::Error::last_os_error();
    matches!(err.raw_os_error(), Some(code) if code == libc::EPERM)
}

fn current_username() -> Result<String, String> {
    if let Ok(user) = std::env::var("SUDO_USER") {
        if !user.is_empty() && user != "root" {
            return Ok(user);
        }
    }
    if let Ok(user) = std::env::var("ZENBOOK_DUO_USER") {
        if !user.is_empty() {
            return Ok(user);
        }
    }

    let user = User::from_uid(Uid::current())
        .map_err(|e| format!("Failed to read current user: {e}"))?
        .ok_or_else(|| "Failed to resolve current user".to_string())?;
    Ok(user.name)
}

fn pid_path() -> String {
    runtime_dir_for_target_user()
        .join("usb_media_remap.pid")
        .to_string_lossy()
        .into_owned()
}

pub fn pause_file_path() -> String {
    runtime_dir_for_target_user()
        .join("usb_media_remap.paused")
        .to_string_lossy()
        .into_owned()
}

pub fn toggle_pause() -> Result<(), String> {
    let path = pause_file_path();
    if std::path::Path::new(&path).exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to remove pause file: {e}"))?;
    } else {
        ensure_duo_dir_for_pid(&pid_path())?;
        fs::write(&path, "").map_err(|e| format!("Failed to create pause file: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn usb_media_remap_toggle_pause() -> Result<(), String> {
    daemon_first_toggle_pause()
}

pub fn daemon_first_status() -> UsbMediaRemapStatus {
    match client::request(DaemonRequest::UsbMediaRemapStatus) {
        Ok(DaemonResponse::UsbMediaRemapStatus { status }) => status,
        _ => get_status(),
    }
}

pub fn daemon_first_start() -> Result<(), String> {
    let result = match client::request(DaemonRequest::UsbMediaRemapStart) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Err("Unexpected daemon response while starting USB remap".into()),
        Err(_) => start_remap(),
    };

    if result.is_ok() {
        let _ = send_desktop_notification("USB Media Remap enabled");
    }

    result
}

pub fn daemon_first_stop() -> Result<(), String> {
    let result = match client::request(DaemonRequest::UsbMediaRemapStop) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Err("Unexpected daemon response while stopping USB remap".into()),
        Err(_) => stop_remap(),
    };

    if result.is_ok() {
        let _ = send_desktop_notification("USB Media Remap disabled");
    }

    result
}

pub fn daemon_first_toggle_pause() -> Result<(), String> {
    let was_paused = daemon_first_status().paused;
    let result = match client::request(DaemonRequest::UsbMediaRemapTogglePause) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Err("Unexpected daemon response while toggling USB remap pause".into()),
        Err(_) => toggle_pause(),
    };

    if result.is_ok() {
        let msg = if was_paused {
            "USB Media Remap resumed"
        } else {
            "USB Media Remap paused"
        };
        let _ = send_desktop_notification(msg);
    }

    result
}

fn running_pid_files() -> Vec<String> {
    let mut out = Vec::new();

    let p1 = pid_path();
    if let Some(pid) = read_pid(&p1) {
        if is_pid_running(pid) {
            out.push(p1);
        }
    }

    out
}

fn runtime_dir_for_target_user() -> std::path::PathBuf {
    paths::user_runtime_dir(target_uid())
}

fn target_uid() -> u32 {
    std::env::var("SUDO_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .or_else(|| {
            std::env::var("ZENBOOK_DUO_UID")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
        })
        .unwrap_or_else(|| Uid::current().as_raw())
}

fn send_desktop_notification(message: &str) -> Result<(), String> {
    let user = current_username()?;
    let uid = target_uid();
    let runtime_dir = format!("/run/user/{uid}");
    let bus_address = format!("unix:path={runtime_dir}/bus");

    let mut cmd = if running_as_root() {
        let mut cmd = Command::new("runuser");
        cmd.args(["-u", &user, "--", "env"]);
        cmd
    } else {
        Command::new("env")
    };

    let status = cmd
        .args([
            &format!("XDG_RUNTIME_DIR={runtime_dir}"),
            &format!("DBUS_SESSION_BUS_ADDRESS={bus_address}"),
            "notify-send",
            "-a",
            "Zenbook Duo Control",
            "-i",
            "input-keyboard",
            message,
        ])
        .status()
        .map_err(|e| format!("Failed to launch desktop notification: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("notify-send exited with {status}"))
    }
}

fn running_as_root() -> bool {
    Uid::current().is_root()
}

fn helper_binary_path() -> Result<std::path::PathBuf, String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| log_error(format!("Failed to find current exe: {e}")))?;
    let sibling = current_exe.with_file_name(HELPER_BINARY_NAME);
    if sibling.exists() {
        return Ok(sibling);
    }
    Err(log_error(format!(
        "Failed to find {} next to {}",
        HELPER_BINARY_NAME,
        current_exe.display()
    )))
}

fn ensure_duo_dir_for_pid(pid_file: &str) -> Result<(), String> {
    let dir = std::path::Path::new(pid_file)
        .parent()
        .ok_or_else(|| format!("Invalid pid file path: {pid_file}"))?;
    crate::runtime::runtime_dir::ensure_dir_owned_like_parent(dir)
}

fn log_error<T: Into<String>>(message: T) -> String {
    let message = message.into();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let log_path = std::path::Path::new(&pid_path())
        .parent()
        .map(|p| p.join("duo.log"))
        .unwrap_or_else(|| std::env::temp_dir().join("zenbook-duo-usb-remap.log"));

    let _ = fs::create_dir_all(
        log_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("/tmp")),
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{} - USB-REMAP - ERROR: {}", timestamp, message);
    }
    message
}
