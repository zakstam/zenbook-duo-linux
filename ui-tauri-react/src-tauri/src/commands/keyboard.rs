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

/// Opens the desktop's keyboard shortcut settings, where XF86Launch1-4 (the
/// ASUS keys) can be bound.
#[tauri::command]
pub fn open_keyboard_shortcuts() -> Result<(), String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let (program, args) = shortcut_settings_command(&desktop).ok_or_else(|| {
        "Open your desktop's keyboard shortcut settings to bind the keys".to_string()
    })?;
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open keyboard shortcuts: {e}"))
}

fn shortcut_settings_command(desktop: &str) -> Option<(&'static str, &'static [&'static str])> {
    let desktop = desktop.to_ascii_uppercase();
    if desktop.contains("GNOME") {
        Some(("gnome-control-center", &["keyboard"]))
    } else if desktop.contains("KDE") {
        Some(("systemsettings", &["kcm_keys"]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_shortcut_settings_for_the_desktop() {
        assert_eq!(
            shortcut_settings_command("ubuntu:GNOME"),
            Some(("gnome-control-center", &["keyboard"][..]))
        );
        assert_eq!(
            shortcut_settings_command("KDE"),
            Some(("systemsettings", &["kcm_keys"][..]))
        );
        assert_eq!(shortcut_settings_command("Hyprland"), None);
    }
}
