use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::commands::usb_media_remap;

#[derive(Default)]
pub struct HotkeyState {
    usb_media_remap: Mutex<Option<String>>,
}

impl HotkeyState {
    pub fn set_usb_media_remap_hotkey(
        &self,
        app: &AppHandle,
        enabled: bool,
        accelerator: &str,
    ) -> Result<(), String> {
        // The underlying Linux implementation relies on X11. On Wayland sessions, global
        // shortcut registration either fails or never receives events.
        #[cfg(target_os = "linux")]
        if enabled && std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("wayland") {
            return Err(
                "Global hotkeys are not supported on Wayland. Use GNOME Settings > Keyboard > Custom Shortcuts to run `zenbook-duo-control --toggle-usb-media-remap`.".to_string()
            );
        }

        let mut current = self
            .usb_media_remap
            .lock()
            .map_err(|_| "Failed to lock hotkey state".to_string())?;

        // Always unregister the previous shortcut before applying changes.
        if let Some(prev) = current.take() {
            let _ = app.global_shortcut().unregister(prev.as_str());
        }

        if !enabled {
            return Ok(());
        }

        let accelerator = accelerator.trim();
        if accelerator.is_empty() {
            return Err("Hotkey cannot be empty".to_string());
        }

        let app_handle = app.clone();
        app.global_shortcut()
            .on_shortcut(accelerator, move |_app, _shortcut, _event| {
                let app_handle = app_handle.clone();
                // pkexec and the status polling can block; run off the shortcut callback thread.
                tauri::async_runtime::spawn_blocking(move || {
                    let status = usb_media_remap::get_status();
                    let _ = if status.running {
                        usb_media_remap::stop_remap()
                    } else {
                        usb_media_remap::start_remap()
                    };
                    // Wake the UI to refresh its status on the next poll.
                    let _ = app_handle.emit("duo://status-changed", ());
                });
            })
            .map_err(|e| format!("Failed to register hotkey '{accelerator}': {e}"))?;

        *current = Some(accelerator.to_string());
        Ok(())
    }
}
