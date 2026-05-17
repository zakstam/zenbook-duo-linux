use std::{fs, path::PathBuf};

use crate::hardware::display_layout;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::models::{Profile, ProfileList};
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

fn daemon_response_result(
    response: Result<DaemonResponse, String>,
    action: &str,
    fallback: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match response {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(format!("{action} failed: {message}")),
        Ok(other) => Err(format!("{action} failed: unexpected daemon response {other:?}")),
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

    daemon_response_result(
        client::request(DaemonRequest::SetOrientation {
            orientation: profile.orientation.clone(),
        }),
        "Set profile orientation",
        || display_layout::set_orientation(&profile.orientation),
    )?;

    if let Some(ref layout) = profile.display_layout {
        daemon_response_result(
            client::request(DaemonRequest::ApplyDisplayLayout {
                layout: layout.clone(),
            }),
            "Apply profile display layout",
            || display_layout::apply_display_layout(layout),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Orientation;
    use std::env;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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
            display_layout: None,
        }
    }

    #[test]
    fn profile_storage_roundtrip_uses_zenbook_duo_home() {
        let _guard = env_lock().lock().expect("profiles env lock");
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
        let _guard = env_lock().lock().expect("profiles env lock");
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

        let unexpected = daemon_response_result(
            Ok(DaemonResponse::Pong),
            "Set profile backlight",
            || Ok(()),
        );
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
