use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchscreenDevice {
    pub name: String,
    pub i2c_id: String,
    pub connector: String,
    pub enabled: bool,
}

/// Maps ELAN model number to display connector.
fn elan_to_connector(name: &str) -> Option<&'static str> {
    if name.contains("ELAN9008") {
        Some("eDP-1")
    } else if name.contains("ELAN9009") {
        Some("eDP-2")
    } else {
        None
    }
}

/// Reads the device name from sysfs for an i2c device.
fn read_i2c_device_name(i2c_id: &str) -> Option<String> {
    let path = format!("/sys/bus/i2c/devices/{}/name", i2c_id);
    fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

/// Checks if the i2c device is currently bound to its driver.
fn is_bound(i2c_id: &str) -> bool {
    Path::new(&format!(
        "/sys/bus/i2c/drivers/i2c_hid_acpi/{}",
        i2c_id
    ))
    .exists()
}

pub fn list_touchscreens() -> Vec<TouchscreenDevice> {
    let mut devices = Vec::new();
    let i2c_devices = match fs::read_dir("/sys/bus/i2c/devices") {
        Ok(entries) => entries,
        Err(_) => return devices,
    };
    for entry in i2c_devices.flatten() {
        let i2c_id = entry.file_name().to_string_lossy().to_string();
        if !i2c_id.starts_with("i2c-ELAN") {
            continue;
        }
        let name = match read_i2c_device_name(&i2c_id) {
            Some(n) => n,
            None => continue,
        };
        let connector = match elan_to_connector(&name) {
            Some(c) => c.to_string(),
            None => continue,
        };
        devices.push(TouchscreenDevice {
            name,
            i2c_id: i2c_id.clone(),
            connector,
            enabled: is_bound(&i2c_id),
        });
    }
    devices
}

pub fn set_touchscreen_enabled(i2c_id: &str, enabled: bool) -> Result<(), String> {
    let path = if enabled {
        "/sys/bus/i2c/drivers/i2c_hid_acpi/bind"
    } else {
        "/sys/bus/i2c/drivers/i2c_hid_acpi/unbind"
    };
    fs::write(path, i2c_id)
        .map_err(|e| format!("Failed to {} touchscreen {}: {}",
            if enabled { "bind" } else { "unbind" }, i2c_id, e))
}
