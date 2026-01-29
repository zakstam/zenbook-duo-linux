use crate::hardware::sysfs;
use crate::models::DuoStatus;

#[tauri::command]
pub fn get_status() -> Result<DuoStatus, String> {
    Ok(sysfs::get_full_status())
}
