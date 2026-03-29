use crate::hardware::display_config;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::models::{DisplayLayout, Orientation};
use crate::runtime::client;

#[tauri::command]
pub fn get_display_layout() -> Result<DisplayLayout, String> {
    match client::request(DaemonRequest::GetDisplayLayout) {
        Ok(DaemonResponse::DisplayLayout { layout }) => {
            Ok(display_config::normalize_display_layout(layout))
        }
        Ok(DaemonResponse::Error { .. }) => display_config::get_display_layout(),
        Ok(_) => display_config::get_display_layout(),
        Err(_) => display_config::get_display_layout(),
    }
}

#[tauri::command]
pub fn apply_display_layout(layout: DisplayLayout) -> Result<(), String> {
    let normalized = display_config::normalize_display_layout(layout);

    match client::request(DaemonRequest::ApplyDisplayLayout {
        layout: normalized.clone(),
    }) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { .. }) => display_config::apply_display_layout(&normalized),
        Ok(_) => display_config::apply_display_layout(&normalized),
        Err(_) => display_config::apply_display_layout(&normalized),
    }
}

#[tauri::command]
pub fn set_orientation(orientation: Orientation) -> Result<(), String> {
    match client::request(DaemonRequest::SetOrientation {
        orientation: orientation.clone(),
    }) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { .. }) => display_config::set_orientation(&orientation),
        Ok(_) => display_config::set_orientation(&orientation),
        Err(_) => display_config::set_orientation(&orientation),
    }
}
