use super::*;

pub(crate) struct HotkeyWatcher;

impl HotkeyWatcher {
    pub(crate) fn watch() -> Result<(), String> {
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

pub(super) fn is_hotkey_abs_code(code: u16) -> bool {
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


