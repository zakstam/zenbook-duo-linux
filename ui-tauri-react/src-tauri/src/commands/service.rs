use std::process::Command;

use crate::hardware::sysfs;

#[tauri::command]
pub fn is_service_active() -> bool {
    sysfs::is_service_active()
}

#[tauri::command]
pub fn restart_service() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["--user", "restart", "zenbook-duo-user.service"])
        .output()
        .map_err(|e| format!("Failed to restart service: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Service restart failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}
