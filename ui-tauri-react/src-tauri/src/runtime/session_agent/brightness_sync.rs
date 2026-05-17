use super::*;

pub(crate) struct BrightnessSync;

impl BrightnessSync {
    pub(crate) async fn watch() -> Result<(), String> {
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

