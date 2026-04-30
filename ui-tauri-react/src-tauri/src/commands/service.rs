use std::process::Command;

use crate::hardware::sysfs;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse, PROTOCOL_VERSION};
use crate::models::VersionInfo;
use crate::runtime::client;

const LEGACY_DAEMON_RESTART_ERROR: &str = "Service restart not yet owned by rust-daemon";

#[tauri::command]
pub fn is_service_active() -> bool {
    match client::request(DaemonRequest::GetStatus) {
        Ok(DaemonResponse::Status { status }) => status.service_active,
        _ => sysfs::is_service_active(),
    }
}

#[tauri::command]
pub fn get_version_info() -> VersionInfo {
    let app_version = env!("CARGO_PKG_VERSION");

    match client::request(DaemonRequest::GetVersion) {
        Ok(DaemonResponse::Version { version }) => {
            VersionInfo::from_daemon(app_version, PROTOCOL_VERSION, version)
        }
        Ok(DaemonResponse::Error { message }) => parse_daemon_protocol_version(&message)
            .map(|daemon_protocol_version| {
                VersionInfo::protocol_mismatch(
                    app_version,
                    PROTOCOL_VERSION,
                    daemon_protocol_version,
                )
            })
            .unwrap_or_else(|| VersionInfo::unavailable(app_version, PROTOCOL_VERSION)),
        _ => VersionInfo::unavailable(app_version, PROTOCOL_VERSION),
    }
}

fn parse_daemon_protocol_version(message: &str) -> Option<u32> {
    message
        .strip_prefix("Protocol mismatch: expected ")?
        .split_once(',')?
        .0
        .parse()
        .ok()
}

#[tauri::command]
pub fn restart_service() -> Result<(), String> {
    match client::request(DaemonRequest::RestartService) {
        Ok(DaemonResponse::Ack) => return Ok(()),
        Ok(DaemonResponse::Error { message }) if message != LEGACY_DAEMON_RESTART_ERROR => {
            return Err(message);
        }
        Ok(DaemonResponse::Error { .. }) => {}
        Ok(_) => return Err("Unexpected daemon response while restarting service".into()),
        Err(_) => {}
    }

    let mut errors = Vec::new();

    if let Err(message) = restart_system_unit("zenbook-duo-rust-daemon.service") {
        errors.push(message);
    }

    if let Err(message) = restart_user_unit("zenbook-duo-session-agent.service") {
        errors.push(message);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn restart_user_unit(unit: &str) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["--user", "restart", unit])
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

fn restart_system_unit(unit: &str) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["restart", unit])
        .output()
        .map_err(|e| format!("Failed to restart {unit}: {e}"))?;

    if output.status.success() || unit_not_found(&output) {
        return Ok(());
    }

    if command_exists("pkexec") {
        let elevated = Command::new("pkexec")
            .args(["systemctl", "restart", unit])
            .output()
            .map_err(|e| format!("Failed to restart {unit} with pkexec: {e}"))?;

        if elevated.status.success() || unit_not_found(&elevated) {
            return Ok(());
        }

        return Err(format!(
            "Failed to restart {unit} with pkexec: {}",
            String::from_utf8_lossy(&elevated.stderr).trim()
        ));
    }

    Err(format!(
        "Failed to restart {unit}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn command_exists(program: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {program} >/dev/null 2>&1")])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn unit_not_found(output: &std::process::Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.contains("not loaded")
        || stderr.contains("could not be found")
        || stderr.contains("Unit ")
}
