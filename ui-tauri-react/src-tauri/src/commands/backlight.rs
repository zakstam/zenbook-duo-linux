use crate::hardware::{hid, sysfs};

#[tauri::command]
pub fn get_backlight() -> u8 {
    sysfs::read_backlight_level()
}

#[tauri::command]
pub fn set_backlight(level: u8) -> Result<(), String> {
    hid::set_backlight(level)
}
