use std::fs;
use std::path::Path;

use crate::models::{ConnectionType, DuoStatus, Orientation};
use crate::runtime::{paths, state::RuntimeState};

const INTEL_BACKLIGHT_BRIGHTNESS: &str = "/sys/class/backlight/intel_backlight/brightness";
const INTEL_BACKLIGHT_MAX: &str = "/sys/class/backlight/intel_backlight/max_brightness";

fn load_runtime_state() -> Option<RuntimeState> {
    let path = paths::state_file_path();
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn read_backlight_level() -> u8 {
    load_runtime_state()
        .map(|state| state.status.backlight_level)
        .unwrap_or(0)
}

pub fn read_display_brightness() -> u32 {
    fs::read_to_string(INTEL_BACKLIGHT_BRIGHTNESS)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn read_max_brightness() -> u32 {
    fs::read_to_string(INTEL_BACKLIGHT_MAX)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1)
}

pub fn detect_connection_type() -> ConnectionType {
    let hidraw_dir = Path::new("/sys/class/hidraw");
    if !hidraw_dir.exists() {
        return ConnectionType::None;
    }

    let mut saw_usb = false;
    let mut saw_bluetooth = false;

    if let Ok(entries) = fs::read_dir(hidraw_dir) {
        for entry in entries.flatten() {
            let uevent_path = entry.path().join("device/uevent");
            if let Ok(contents) = fs::read_to_string(&uevent_path) {
                if contents.contains("Zenbook Duo Keyboard") || contents.contains("ASUS_DUO") {
                    // Prefer bus id detection: HID_ID=0005:... is Bluetooth HID.
                    if contents.contains("HID_ID=0005:") {
                        saw_bluetooth = true;
                        continue;
                    }

                    // HID_ID=0003:... is USB HID; treat unknown as USB-ish.
                    saw_usb = true;
                }
            }
        }
    }

    if saw_bluetooth {
        ConnectionType::Bluetooth
    } else if saw_usb {
        ConnectionType::Usb
    } else {
        ConnectionType::None
    }
}

pub fn is_service_active() -> bool {
    is_unit_active_system("zenbook-duo-rust-daemon.service")
        && is_unit_active_user("zenbook-duo-session-agent.service")
}

fn is_unit_active_user(unit: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", unit])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

fn is_unit_active_system(unit: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

pub fn get_full_status() -> DuoStatus {
    if let Some(mut status) = load_runtime_state().map(|state| state.status) {
        status.display_brightness = read_display_brightness();
        status.max_brightness = read_max_brightness();
        status.connection_type = detect_connection_type();
        status.service_active = is_service_active();
        return status;
    }

    DuoStatus {
        keyboard_attached: false,
        connection_type: detect_connection_type(),
        monitor_count: 0,
        wifi_enabled: false,
        bluetooth_enabled: false,
        backlight_level: read_backlight_level(),
        display_brightness: read_display_brightness(),
        max_brightness: read_max_brightness(),
        service_active: is_service_active(),
        orientation: Orientation::Normal,
    }
}

pub fn clear_log() -> Result<(), String> {
    let runtime_path = paths::log_file_path();
    if let Some(parent) = runtime_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(runtime_path, "").map_err(|e| e.to_string())
}

pub fn read_log_lines(count: usize) -> Vec<String> {
    let runtime_path = paths::log_file_path();
    let contents = fs::read_to_string(&runtime_path).unwrap_or_default();

    contents
        .lines()
        .rev()
        .take(count)
        .map(String::from)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
