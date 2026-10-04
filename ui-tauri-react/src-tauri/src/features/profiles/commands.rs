use std::{fs, path::PathBuf};

use super::layout::{profile_from_layout, profile_layout, shared_rotation};
use super::model::{Profile, ProfileList};
use crate::commands::display;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;

fn profiles_path() -> PathBuf {
    let config_dir = crate::commands::settings::config_base_dir().join("zenbook-duo");
    let _ = fs::create_dir_all(&config_dir);
    config_dir.join("profiles.json")
}

fn load_profile_list() -> ProfileList {
    let path = profiles_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| ProfileList {
            profiles: Profile::default_profiles(
                crate::commands::settings::load_settings_local().default_scale,
            ),
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

/// Saves the screens as they are now (each screen's scale and rotation, and
/// whether the bottom screen is on) as a new profile.
#[tauri::command]
pub fn save_current_profile(name: String, backlight_level: u8) -> Result<Profile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Profile name is empty".into());
    }
    let current = display::get_display_layout()
        .map_err(|message| format!("Read display layout failed: {message}"))?;
    let profile = profile_from_layout(new_profile_id(&name), name, backlight_level, &current)?;
    save_profile(profile.clone())?;
    Ok(profile)
}

fn new_profile_id(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    format!("{slug}-{millis}")
}

#[tauri::command]
pub fn delete_profile(id: String) -> Result<(), String> {
    let mut list = load_profile_list();
    list.profiles.retain(|p| p.id != id);
    save_profile_list(&list)
}

fn daemon_response_result(
    response: Result<DaemonResponse, String>,
    action: &str,
    fallback: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match response {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(format!("{action} failed: {message}")),
        Ok(other) => Err(format!(
            "{action} failed: unexpected daemon response {other:?}"
        )),
        Err(_) => fallback().map_err(|message| format!("{action} failed: {message}")),
    }
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

    daemon_response_result(
        client::request(DaemonRequest::SetBacklight {
            level: profile.backlight_level,
        }),
        "Set profile backlight",
        || crate::commands::backlight::set_backlight_daemon_first(profile.backlight_level),
    )?;

    let current = display::get_display_layout()
        .map_err(|message| format!("Read display layout failed: {message}"))?;
    let layout = profile_layout(&profile, &current)?;
    display::apply_display_layout(layout)
        .map_err(|message| format!("Apply profile display layout failed: {message}"))?;

    if let Some(orientation) = shared_rotation(&profile) {
        display::set_orientation(orientation)
            .map_err(|message| format!("Set profile orientation failed: {message}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Orientation;
    use std::env;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestHome {
        path: PathBuf,
        previous_home: Option<String>,
    }

    impl TestHome {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "zenbook-duo-profiles-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test home");
            let previous_home = env::var("ZENBOOK_DUO_HOME").ok();
            env::set_var("ZENBOOK_DUO_HOME", &path);
            Self {
                path,
                previous_home,
            }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            if let Some(previous_home) = &self.previous_home {
                env::set_var("ZENBOOK_DUO_HOME", previous_home);
            } else {
                env::remove_var("ZENBOOK_DUO_HOME");
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_profile(id: &str) -> Profile {
        Profile {
            id: id.into(),
            name: "Test".into(),
            backlight_level: 2,
            scale: 1.5,
            orientation: Orientation::Normal,
            dual_screen_enabled: true,
            bottom_scale: Some(1.25),
            bottom_orientation: Some(Orientation::Inverted),
        }
    }

    #[test]
    fn profiles_saved_before_per_screen_fields_still_load() {
        let raw = r#"{"profiles":[{"id":"old","name":"Old","backlightLevel":2,
            "scale":1.25,"orientation":"left","dualScreenEnabled":true,"displayLayout":null}]}"#;
        let list: ProfileList = serde_json::from_str(raw).expect("old profiles parse");
        let old = &list.profiles[0];
        assert_eq!(old.effective_bottom_scale(), 1.25);
        assert_eq!(old.effective_bottom_orientation(), Orientation::Left);
    }

    #[test]
    fn default_profiles_use_the_setup_scale_and_persist_edits() {
        let _guard = crate::commands::settings::test_env_lock()
            .lock()
            .expect("profiles env lock");
        let _home = TestHome::new();
        let mut settings = crate::models::DuoSettings::default();
        settings.default_scale = 1.25;
        crate::commands::settings::save_settings_local(settings).expect("save settings");

        let docked = list_profiles()
            .into_iter()
            .find(|profile| profile.id == "docked")
            .expect("docked default");
        assert_eq!(docked.scale, 1.25);
        assert!(!docked.dual_screen_enabled);

        save_profile(Profile {
            scale: 2.0,
            ..docked
        })
        .expect("edit default");

        let profiles = list_profiles();
        assert_eq!(profiles.len(), 3);
        let edited = profiles.iter().find(|p| p.id == "docked").expect("docked");
        assert_eq!(edited.scale, 2.0);
    }

    #[test]
    fn profile_storage_roundtrip_uses_zenbook_duo_home() {
        let _guard = crate::commands::settings::test_env_lock()
            .lock()
            .expect("profiles env lock");
        let home = TestHome::new();
        let profile = test_profile("custom");

        save_profile(profile).expect("save profile");

        let profile_path = home.path.join(".config/zenbook-duo/profiles.json");
        assert!(profile_path.is_file());
        assert!(list_profiles().iter().any(|profile| profile.id == "custom"));

        delete_profile("custom".into()).expect("delete profile");
        assert!(!list_profiles().iter().any(|profile| profile.id == "custom"));
    }

    #[test]
    fn profile_storage_loads_defaults_when_no_file_exists() {
        let _guard = crate::commands::settings::test_env_lock()
            .lock()
            .expect("profiles env lock");
        let _home = TestHome::new();

        let profiles = list_profiles();

        assert!(profiles.iter().any(|profile| profile.id == "docked"));
        assert!(profiles.iter().any(|profile| profile.id == "tablet"));
        assert!(profiles.iter().any(|profile| profile.id == "presentation"));
    }

    #[test]
    fn profile_activation_surfaces_daemon_errors_and_unexpected_responses() {
        let daemon_error = daemon_response_result(
            Ok(DaemonResponse::Error {
                message: "no session".into(),
            }),
            "Set profile orientation",
            || Ok(()),
        );
        assert_eq!(
            daemon_error.expect_err("daemon error should fail"),
            "Set profile orientation failed: no session"
        );

        let unexpected =
            daemon_response_result(Ok(DaemonResponse::Pong), "Set profile backlight", || Ok(()));
        assert!(unexpected
            .expect_err("unexpected response should fail")
            .contains("unexpected daemon response"));
    }

    #[test]
    fn profile_activation_uses_fallback_only_for_daemon_transport_failure() {
        let fallback_error = daemon_response_result(
            Err("daemon unavailable".into()),
            "Apply profile display layout",
            || Err("local apply failed".into()),
        );
        assert_eq!(
            fallback_error.expect_err("fallback error should fail"),
            "Apply profile display layout failed: local apply failed"
        );

        let fallback_success = daemon_response_result(
            Err("daemon unavailable".into()),
            "Set profile backlight",
            || Ok(()),
        );
        assert!(fallback_success.is_ok());
    }
}
