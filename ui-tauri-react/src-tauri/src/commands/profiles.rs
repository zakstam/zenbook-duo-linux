use std::fs;
use std::path::PathBuf;

use crate::hardware::display_config;
use crate::models::{Profile, ProfileList};

fn profiles_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("zenbook-duo");
    let _ = fs::create_dir_all(&config_dir);
    config_dir.join("profiles.json")
}

fn load_profile_list() -> ProfileList {
    let path = profiles_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| ProfileList {
            profiles: Profile::default_profiles(),
        })
}

fn save_profile_list(list: &ProfileList) -> Result<(), String> {
    let path = profiles_path();
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("Serialize error: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {e}"))
}

#[tauri::command]
pub fn list_profiles() -> Vec<Profile> {
    load_profile_list().profiles
}

#[tauri::command]
pub fn save_profile(profile: Profile) -> Result<(), String> {
    let mut list = load_profile_list();
    if let Some(existing) = list.profiles.iter_mut().find(|p| p.id == profile.id) {
        *existing = profile;
    } else {
        list.profiles.push(profile);
    }
    save_profile_list(&list)
}

#[tauri::command]
pub fn delete_profile(id: String) -> Result<(), String> {
    let mut list = load_profile_list();
    list.profiles.retain(|p| p.id != id);
    save_profile_list(&list)
}

#[tauri::command]
pub fn activate_profile(id: String) -> Result<(), String> {
    let list = load_profile_list();
    let profile = list
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("Profile '{id}' not found"))?
        .clone();

    // Apply backlight
    let _ = crate::hardware::hid::set_backlight(profile.backlight_level);

    // Apply orientation
    let _ = display_config::set_orientation(&profile.orientation);

    // Apply display layout if present
    if let Some(ref layout) = profile.display_layout {
        let _ = display_config::apply_display_layout(layout);
    }

    Ok(())
}
