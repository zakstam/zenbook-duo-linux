use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::hardware::{hid, sysfs};
use crate::runtime::client;

#[tauri::command]
pub fn get_backlight() -> u8 {
    sysfs::read_backlight_level()
}

#[tauri::command]
pub fn set_backlight(level: u8) -> Result<(), String> {
    set_backlight_daemon_first(level)
}

pub fn set_backlight_daemon_first(level: u8) -> Result<(), String> {
    match client::request(DaemonRequest::SetBacklight { level }) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { .. }) => hid::set_backlight(level),
        Ok(_) => hid::set_backlight(level),
        Err(_) => hid::set_backlight(level),
    }
}
