use tauri::{AppHandle, State};

use crate::hotkeys::HotkeyState;

#[tauri::command]
pub fn apply_usb_media_remap_hotkey(
    app: AppHandle,
    state: State<'_, HotkeyState>,
    enabled: bool,
    accelerator: String,
) -> Result<(), String> {
    state.set_usb_media_remap_hotkey(&app, enabled, &accelerator)
}

