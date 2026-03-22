use crate::hardware::touchscreen::{self, TouchscreenDevice};
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;

#[tauri::command]
pub fn list_touchscreens() -> Result<Vec<TouchscreenDevice>, String> {
    match client::request(DaemonRequest::ListTouchscreens) {
        Ok(DaemonResponse::Touchscreens { devices }) => Ok(devices),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Ok(touchscreen::list_touchscreens()),
        Err(_) => Ok(touchscreen::list_touchscreens()),
    }
}

#[tauri::command]
pub fn set_touchscreen_enabled(connector: String, enabled: bool) -> Result<(), String> {
    let fallback = || {
        let devices = touchscreen::list_touchscreens();
        match devices.iter().find(|d| d.connector == connector) {
            Some(dev) => touchscreen::set_touchscreen_enabled(&dev.i2c_id, enabled),
            None => Err(format!("No touchscreen found for {}", connector)),
        }
    };
    match client::request(DaemonRequest::SetTouchscreenEnabled {
        connector: connector.clone(),
        enabled,
    }) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => fallback(),
        Err(_) => fallback(),
    }
}
