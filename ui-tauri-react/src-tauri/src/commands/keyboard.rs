use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;

#[tauri::command]
pub fn set_fn_lock(locked: bool) -> Result<(), String> {
    // Only the root daemon can write the shared Fn lock state.
    match client::request(DaemonRequest::SetFnLock { locked })? {
        DaemonResponse::Ack => Ok(()),
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("Unexpected daemon response while setting Fn lock: {other:?}")),
    }
}
