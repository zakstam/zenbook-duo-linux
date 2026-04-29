use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{ConnectionType, DuoStatus, Orientation};
use crate::runtime::{paths, state::RuntimeState};

const BACKLIGHT_ROOT: &str = "/sys/class/backlight";

fn load_runtime_state() -> Option<RuntimeState> {
    let path = paths::state_file_path();
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn backlight_dirs_from(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| path.join("brightness").exists())
        .collect();
    dirs.sort();
    dirs
}

fn backlight_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_secondary_backlight_name(name: &str) -> bool {
    name.contains("edp-2") || name.contains("edp2")
}

fn primary_backlight_dir_from(root: &Path) -> Option<PathBuf> {
    let dirs = backlight_dirs_from(root);
    dirs.iter()
        .find(|path| backlight_name(path) == "intel_backlight")
        .or_else(|| {
            dirs.iter()
                .find(|path| backlight_name(path).contains("edp-1"))
        })
        .or_else(|| {
            dirs.iter()
                .find(|path| !is_secondary_backlight_name(&backlight_name(path)))
        })
        .cloned()
}

fn secondary_backlight_dir_from(root: &Path) -> Option<PathBuf> {
    backlight_dirs_from(root)
        .into_iter()
        .find(|path| is_secondary_backlight_name(&backlight_name(path)))
}

pub fn primary_backlight_dir() -> Option<PathBuf> {
    primary_backlight_dir_from(Path::new(BACKLIGHT_ROOT))
}

pub fn secondary_backlight_dir() -> Option<PathBuf> {
    secondary_backlight_dir_from(Path::new(BACKLIGHT_ROOT))
}

fn read_backlight_value(path: &Path) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn read_backlight_level() -> u8 {
    load_runtime_state()
        .map(|state| state.status.backlight_level)
        .unwrap_or(0)
}

pub fn read_display_brightness() -> u32 {
    primary_backlight_dir()
        .and_then(|dir| read_backlight_value(&dir.join("brightness")))
        .unwrap_or(0)
}

pub fn read_max_brightness() -> u32 {
    primary_backlight_dir()
        .and_then(|dir| read_backlight_value(&dir.join("max_brightness")))
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

    // The keyboard can expose both paired Bluetooth and docked USB HID interfaces at the
    // same time. Prefer USB so docked behavior, including the media remap helper, stays active.
    classify_connection_type(saw_usb, saw_bluetooth)
}

fn classify_connection_type(saw_usb: bool, saw_bluetooth: bool) -> ConnectionType {
    if saw_usb {
        ConnectionType::Usb
    } else if saw_bluetooth {
        ConnectionType::Bluetooth
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_usb_when_both_transport_interfaces_are_visible() {
        assert_eq!(classify_connection_type(true, true), ConnectionType::Usb);
    }

    #[test]
    fn returns_bluetooth_when_only_bluetooth_is_visible() {
        assert_eq!(
            classify_connection_type(false, true),
            ConnectionType::Bluetooth
        );
    }

    #[test]
    fn returns_none_when_no_transport_is_visible() {
        assert_eq!(classify_connection_type(false, false), ConnectionType::None);
    }

    #[test]
    fn discovers_primary_and_secondary_backlight_devices() {
        let root = unique_temp_dir("backlight");
        create_backlight(&root, "card1-eDP-2-backlight");
        create_backlight(&root, "intel_backlight");

        assert_eq!(
            primary_backlight_dir_from(&root)
                .and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some(std::ffi::OsString::from("intel_backlight"))
        );
        assert_eq!(
            secondary_backlight_dir_from(&root)
                .and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some(std::ffi::OsString::from("card1-eDP-2-backlight"))
        );

        fs::remove_dir_all(root).expect("remove temp dir");
    }

    fn create_backlight(root: &Path, name: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).expect("create backlight dir");
        fs::write(dir.join("brightness"), "100").expect("write brightness");
        fs::write(dir.join("max_brightness"), "400").expect("write max brightness");
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "zenbook-duo-sysfs-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
