use std::os::unix::process::CommandExt;
use std::process::{Command, Output};

use nix::unistd::{Gid, Uid};

/// Adapter for systemd service-control behavior used by daemon requests and
/// session lifecycle recovery.
pub(crate) struct ServiceController;

impl ServiceController {
    pub(crate) fn restart_owned_services() -> Result<(), String> {
        restart_owned_services()
    }

    pub(crate) fn queue_target_user_unit_restart(unit: &str) -> Result<(), String> {
        queue_target_user_unit_restart(unit)
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
    #[cfg(test)]
    {
        let _ = unit;
        return Ok(());
    }

    #[cfg(not(test))]
    let mut command = target_user_systemctl_command();
    #[cfg(not(test))]
    {
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
