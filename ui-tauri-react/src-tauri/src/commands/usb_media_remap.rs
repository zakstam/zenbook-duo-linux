use chrono::Local;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::Duration;

use nix::unistd::{Uid, User};

// Legacy path used by older builds that shared a global /tmp/duo directory.
const LEGACY_PID_PATH: &str = "/tmp/duo/usb_media_remap.pid";
const HELPER_FLAG: &str = "--usb-media-remap-helper";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbMediaRemapStatus {
    pub running: bool,
    pub pid: Option<u32>,
}

#[tauri::command]
pub fn usb_media_remap_status() -> UsbMediaRemapStatus {
    get_status()
}

#[tauri::command]
pub async fn usb_media_remap_start() -> Result<(), String> {
    // These operations can block (pkexec, sleeps, polling). Run off the main thread so the UI
    // stays responsive.
    tauri::async_runtime::spawn_blocking(start_remap)
        .await
        .unwrap_or_else(|e| Err(format!("Failed to join background task: {e}")))
}

#[tauri::command]
pub async fn usb_media_remap_stop() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(stop_remap)
        .await
        .unwrap_or_else(|e| Err(format!("Failed to join background task: {e}")))
}

pub fn get_status() -> UsbMediaRemapStatus {
    // Prefer per-user pid path, but also tolerate older installs.
    let pid = read_pid(&pid_path()).or_else(|| read_pid(LEGACY_PID_PATH));
    if let Some(pid) = pid {
        if is_pid_running(pid) {
            return UsbMediaRemapStatus {
                running: true,
                pid: Some(pid),
            };
        }
        let _ = fs::remove_file(pid_path());
        let _ = fs::remove_file(LEGACY_PID_PATH);
    }

    UsbMediaRemapStatus {
        running: false,
        pid: None,
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

    // Use the main executable as the pkexec target so we don't need to ship a separate helper
    // binary in bundles. The binary short-circuits in `main` when HELPER_FLAG is present.
    let helper_path = std::env::current_exe().map_err(|e| log_error(format!("Failed to find current exe: {e}")))?;
    let user = current_username().map_err(log_error)?;

    let mut cmd = Command::new("pkexec");
    cmd.arg(helper_path)
        .arg(HELPER_FLAG)
        .arg("--pid-file")
        .arg(&pid_path)
        .arg("--user")
        .arg(user);

    start_remap_spawn_and_wait(cmd, Duration::from_secs(START_TIMEOUT_SECS))
}

pub fn stop_remap() -> Result<(), String> {
    let pid_files = running_pid_files();
    if pid_files.is_empty() {
        // Nothing to stop.
        return Ok(());
    }

    let helper_path = std::env::current_exe()
        .map_err(|e| log_error(format!("Failed to find current exe: {e}")))?;

    for pid_path in pid_files {
        ensure_duo_dir_for_pid(&pid_path)?;

        let mut cmd = Command::new("pkexec");
        cmd.arg(&helper_path)
            .arg(HELPER_FLAG)
            .arg("--stop")
            .arg("--pid-file")
            .arg(&pid_path);

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
    let user = User::from_uid(Uid::current())
        .map_err(|e| format!("Failed to read current user: {e}"))?
        .ok_or_else(|| "Failed to resolve current user".to_string())?;
    Ok(user.name)
}

fn pid_path() -> String {
    let uid = Uid::current().as_raw();
    format!("/tmp/duo-{uid}/usb_media_remap.pid")
}

fn running_pid_files() -> Vec<String> {
    let mut out = Vec::new();

    let p1 = pid_path();
    if let Some(pid) = read_pid(&p1) {
        if is_pid_running(pid) {
            out.push(p1);
        }
    }

    if let Some(pid) = read_pid(LEGACY_PID_PATH) {
        if is_pid_running(pid) {
            out.push(LEGACY_PID_PATH.to_string());
        }
    }

    out
}

fn ensure_duo_dir_for_pid(pid_file: &str) -> Result<(), String> {
    let dir = std::path::Path::new(pid_file)
        .parent()
        .ok_or_else(|| format!("Invalid pid file path: {pid_file}"))?;

    fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

    // Per-user directory: only needs to be writable by the current user and root.
    // If the directory already exists with different ownership (e.g. created by an old version),
    // chmod may fail - don't hard fail on that.
    if let Err(e) = fs::set_permissions(dir, fs::Permissions::from_mode(0o700)) {
        if e.kind() != std::io::ErrorKind::PermissionDenied {
            return Err(format!(
                "Failed to set {} permissions: {e}",
                dir.display()
            ));
        }
    }

    Ok(())
}

fn log_error<T: Into<String>>(message: T) -> String {
    let message = message.into();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let log_path = std::path::Path::new(&pid_path())
        .parent()
        .map(|p| p.join("duo.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/duo.log"));

    let _ = fs::create_dir_all(log_path.parent().unwrap_or_else(|| std::path::Path::new("/tmp")));
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(file, "{} - USB-REMAP - ERROR: {}", timestamp, message);
    }
    message
}
