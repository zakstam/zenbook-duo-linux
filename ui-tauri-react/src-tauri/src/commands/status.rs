use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::hardware::sysfs;
use crate::models::DuoStatus;
use crate::runtime::client;

#[tauri::command]
pub fn get_status() -> Result<DuoStatus, String> {
    match client::request(DaemonRequest::GetStatus) {
        Ok(DaemonResponse::Status { status }) => Ok(status),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Err("Unexpected daemon response while reading status".into()),
        Err(_) => Ok(sysfs::get_full_status()),
    }
}
