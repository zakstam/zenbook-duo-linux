use std::fs;
use std::path::PathBuf;

use crate::models::DuoSettings;

fn settings_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("zenbook-duo");
    let _ = fs::create_dir_all(&config_dir);
    config_dir.join("settings.json")
}

#[tauri::command]
pub fn load_settings() -> DuoSettings {
    let path = settings_path();
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return DuoSettings::default(), // no settings file => show setup
    };

    // Merge defaults + file contents.
    // Important behavior: when upgrading an existing install, we don't want to force the setup
    // screen to appear just because `setupCompleted` is a new field.
    let mut settings: DuoSettings = serde_json::from_str(&raw).unwrap_or_default();

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
        if v.get("setupCompleted").is_none() {
            settings.setup_completed = true;
        }
    }

    settings
}

#[tauri::command]
pub fn save_settings(settings: DuoSettings) -> Result<(), String> {
    let path = settings_path();
    let json =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("Serialize error: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {e}"))
}
