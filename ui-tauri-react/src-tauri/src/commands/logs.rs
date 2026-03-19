use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::hardware::sysfs;
use crate::runtime::logger;
use crate::runtime::client;

#[tauri::command]
pub fn read_log(lines: usize) -> Vec<String> {
    match client::request(DaemonRequest::TailLogs { lines }) {
        Ok(DaemonResponse::Logs { lines }) => lines,
        _ => sysfs::read_log_lines(lines),
    }
}

#[tauri::command]
pub fn clear_log() -> Result<(), String> {
    logger::clear().or_else(|_| sysfs::clear_log())
}
