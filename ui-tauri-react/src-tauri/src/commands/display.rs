use crate::hardware::display_config;
use crate::models::{DisplayLayout, Orientation};

#[tauri::command]
pub fn get_display_layout() -> Result<DisplayLayout, String> {
    display_config::get_display_layout()
}

#[tauri::command]
pub fn apply_display_layout(layout: DisplayLayout) -> Result<(), String> {
    display_config::apply_display_layout(&layout)
}

#[tauri::command]
pub fn set_orientation(orientation: Orientation) -> Result<(), String> {
    display_config::set_orientation(&orientation)
}
