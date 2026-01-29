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
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(settings: DuoSettings) -> Result<(), String> {
    let path = settings_path();
    let json =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("Serialize error: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {e}"))
}
