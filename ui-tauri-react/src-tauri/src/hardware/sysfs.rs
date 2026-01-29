use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::{ConnectionType, DuoStatus, Orientation};

const STATUS_PATH: &str = "/tmp/duo/status";
const KB_BACKLIGHT_PATH: &str = "/tmp/duo/kb_backlight_level";
const INTEL_BACKLIGHT_BRIGHTNESS: &str = "/sys/class/backlight/intel_backlight/brightness";
const INTEL_BACKLIGHT_MAX: &str = "/sys/class/backlight/intel_backlight/max_brightness";

pub fn read_status_file() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(contents) = fs::read_to_string(STATUS_PATH) {
        for line in contents.lines() {
            if let Some((key, value)) = line.split_once('=') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    map
}

pub fn read_backlight_level() -> u8 {
    fs::read_to_string(KB_BACKLIGHT_PATH)
        .ok()
        .and_then(|s| s.trim().parse().ok())
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
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "zenbook-duo-user.service"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

pub fn get_full_status() -> DuoStatus {
    let status_map = read_status_file();

    let keyboard_attached = status_map
        .get("KEYBOARD_ATTACHED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let monitor_count = status_map
        .get("MONITOR_COUNT")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let wifi_enabled = status_map
        .get("WIFI_BEFORE")
        .map(|v| v == "enabled" || v == "unblocked")
        .or_else(|| {
            status_map
                .get("WIFI_ENABLED")
                .map(|v| v == "true" || v == "1")
        })
        .unwrap_or(false);

    let bluetooth_enabled = status_map
        .get("BLUETOOTH_BEFORE")
        .map(|v| v == "enabled" || v == "unblocked")
        .or_else(|| {
            status_map
                .get("BT_ENABLED")
                .map(|v| v == "true" || v == "1")
        })
        .unwrap_or(false);

    DuoStatus {
        keyboard_attached,
        connection_type: detect_connection_type(),
        monitor_count,
        wifi_enabled,
        bluetooth_enabled,
        backlight_level: read_backlight_level(),
        display_brightness: read_display_brightness(),
        max_brightness: read_max_brightness(),
        service_active: is_service_active(),
        orientation: Orientation::Normal,
    }
}

pub fn clear_log() -> Result<(), String> {
    let log_path = "/tmp/duo/duo.log";
    fs::write(log_path, "").map_err(|e| e.to_string())
}

pub fn read_log_lines(count: usize) -> Vec<String> {
    let log_path = "/tmp/duo/duo.log";
    fs::read_to_string(log_path)
        .unwrap_or_default()
        .lines()
        .rev()
        .take(count)
        .map(String::from)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
