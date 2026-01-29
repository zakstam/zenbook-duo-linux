pub mod dbus_watcher;
pub mod file_watcher;
pub mod usb_watcher;

use crate::commands::events::EventBuffer;
use tauri::AppHandle;

pub fn start_all_watchers(app: &AppHandle, buffer: EventBuffer) {
    file_watcher::start(app.clone(), buffer.clone());
    usb_watcher::start(app.clone(), buffer.clone());
    dbus_watcher::start(app.clone(), buffer);
}
