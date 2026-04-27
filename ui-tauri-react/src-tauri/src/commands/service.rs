use std::process::Command;

use crate::hardware::sysfs;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;

#[tauri::command]
pub fn is_service_active() -> bool {
    match client::request(DaemonRequest::GetStatus) {
        Ok(DaemonResponse::Status { status }) => status.service_active,
        _ => sysfs::is_service_active(),
    }
}

#[tauri::command]
pub fn restart_service() -> Result<(), String> {
    match client::request(DaemonRequest::RestartService) {
        Ok(DaemonResponse::Ack) => return Ok(()),
        Ok(DaemonResponse::Error { .. }) => {}
        Ok(_) => {}
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
