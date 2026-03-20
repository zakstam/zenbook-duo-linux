pub mod file_watcher;

use crate::commands::events::EventBuffer;
use tauri::AppHandle;

pub fn start_all_watchers(app: &AppHandle, buffer: EventBuffer) {
    file_watcher::start(app.clone(), buffer.clone());
    let _ = buffer;
}
