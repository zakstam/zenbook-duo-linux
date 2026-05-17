use std::env;
use std::fs;
use std::path::PathBuf;

use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::models::DuoSettings;
use crate::runtime::client;

const AUTOSTART_FILE_NAME: &str = "zenbook-duo-control.desktop";

pub(crate) fn config_base_dir() -> PathBuf {
    if let Ok(home_override) = env::var("ZENBOOK_DUO_HOME") {
        PathBuf::from(home_override).join(".config")
    } else {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"))
    }
}

fn settings_path() -> PathBuf {
    let config_dir = config_base_dir().join("zenbook-duo");
    let _ = fs::create_dir_all(&config_dir);
    config_dir.join("settings.json")
}

fn autostart_path() -> PathBuf {
    config_base_dir()
        .join("autostart")
        .join(AUTOSTART_FILE_NAME)
}

fn autostart_enabled() -> bool {
    autostart_path().is_file()
}

fn desktop_exec_path() -> String {
    let executable = env::current_exe().unwrap_or_else(|_| PathBuf::from("zenbook-duo-control"));
    let executable = executable.to_string_lossy();
    let escaped = executable.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\" --start-minimized")
}

fn write_autostart_entry() -> Result<(), String> {
    let path = autostart_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create autostart dir error: {e}"))?;
    }

    let entry = format!(
        "[Desktop Entry]\n\
Type=Application\n\
Name=Zenbook Duo Control\n\
Comment=Start Zenbook Duo Control hidden in the system tray\n\
Exec={}\n\
Icon=zenbook-duo-control\n\
Terminal=false\n\
Categories=Utility;Settings;\n\
NoDisplay=true\n\
X-GNOME-Autostart-enabled=true\n",
        desktop_exec_path()
    );

    fs::write(&path, entry).map_err(|e| format!("Write autostart entry error: {e}"))
}

fn remove_autostart_entry() -> Result<(), String> {
    match fs::remove_file(autostart_path()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Remove autostart entry error: {e}")),
    }
}

fn sync_autostart_entry(settings: &DuoSettings) -> Result<(), String> {
    if settings.start_on_boot_minimized {
        write_autostart_entry()
    } else {
        remove_autostart_entry()
    }
}

pub fn load_settings_local() -> DuoSettings {
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

pub fn save_settings_local(settings: DuoSettings) -> Result<(), String> {
    let path = settings_path();
    let json =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("Serialize error: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {e}"))
}

#[tauri::command]
pub fn load_settings() -> DuoSettings {
    let mut settings = load_settings_local();
    settings.start_on_boot_minimized = autostart_enabled();
    settings
}

#[tauri::command]
pub fn save_settings(settings: DuoSettings) -> Result<(), String> {
    sync_autostart_entry(&settings)?;
    save_settings_local(settings.clone())?;

    match client::request(DaemonRequest::SaveSettings { settings }) {
        Ok(DaemonResponse::Ack) | Ok(DaemonResponse::Error { .. }) | Ok(_) | Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                "zenbook-duo-settings-test-{}-{unique}",
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

    #[test]
    fn local_settings_roundtrip_preserves_issue_17_switch_fields() {
        let _guard = env_lock().lock().expect("settings env lock");
        let _home = TestHome::new();
        let mut settings = DuoSettings::default();
        settings.start_on_boot_minimized = true;
        settings.invert_sensor_rotation = true;
        settings.setup_completed = true;

        save_settings_local(settings).expect("save settings");
        let loaded = load_settings_local();

        assert!(loaded.start_on_boot_minimized);
        assert!(loaded.invert_sensor_rotation);
        assert!(loaded.setup_completed);
    }

    #[test]
    fn load_settings_uses_autostart_file_as_start_on_boot_source_of_truth() {
        let _guard = env_lock().lock().expect("settings env lock");
        let _home = TestHome::new();
        let mut settings = DuoSettings::default();
        settings.start_on_boot_minimized = true;
        settings.invert_sensor_rotation = true;

        save_settings_local(settings).expect("save settings");
        assert!(!load_settings().start_on_boot_minimized);

        let path = autostart_path();
        fs::create_dir_all(path.parent().expect("autostart parent")).expect("create autostart dir");
        fs::write(&path, "[Desktop Entry]\nType=Application\n").expect("write autostart file");

        let loaded = load_settings();
        assert!(loaded.start_on_boot_minimized);
        assert!(loaded.invert_sensor_rotation);
    }
}
