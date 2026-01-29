use crate::hardware::sysfs;

#[tauri::command]
pub fn read_log(lines: usize) -> Vec<String> {
    sysfs::read_log_lines(lines)
}

#[tauri::command]
pub fn clear_log() -> Result<(), String> {
    sysfs::clear_log()
}
