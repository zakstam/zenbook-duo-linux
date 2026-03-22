use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::ipc::protocol::SessionBackend;
use crate::models::{DuoSettings, DuoStatus, HardwareEvent};
use crate::runtime::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    pub status: DuoStatus,
    pub settings: DuoSettings,
    pub session_agent: SessionAgentState,
    #[serde(default)]
    pub last_runtime_notification: Option<RuntimeNotificationState>,
    pub remembered_wifi_enabled: Option<bool>,
    pub remembered_bluetooth_enabled: Option<bool>,
    pub last_updated: DateTime<Utc>,
    pub recent_events: Vec<HardwareEvent>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            status: DuoStatus::default(),
            settings: DuoSettings::default(),
            session_agent: SessionAgentState::default(),
            last_runtime_notification: None,
            remembered_wifi_enabled: None,
            remembered_bluetooth_enabled: None,
            last_updated: Utc::now(),
            recent_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionAgentState {
    pub connected: bool,
    pub session_id: Option<String>,
    pub backend: Option<SessionBackend>,
    pub socket_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNotificationState {
    pub key: String,
    pub emitted_at: DateTime<Utc>,
}

impl RuntimeState {
    pub fn touch(&mut self) {
        self.last_updated = Utc::now();
    }

    pub fn load() -> Self {
        fs::read_to_string(paths::state_file_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = paths::state_file_path().parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create runtime state dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize runtime state: {e}"))?;
        fs::write(paths::state_file_path(), json)
            .map_err(|e| format!("Failed to write runtime state: {e}"))
    }
}
