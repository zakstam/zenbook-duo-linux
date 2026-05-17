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
    pub usb_media_remap_reconcile: UsbMediaRemapReconcileState,
    #[serde(default)]
    pub last_runtime_notification: Option<RuntimeNotificationState>,
    #[serde(default)]
    pub lid_closed: bool,
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
            usb_media_remap_reconcile: UsbMediaRemapReconcileState::default(),
            last_runtime_notification: None,
            lid_closed: false,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsbMediaRemapReconcileState {
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_start_log_at: Option<DateTime<Utc>>,
    pub last_backoff_log_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNotificationState {
    pub key: String,
    pub emitted_at: DateTime<Utc>,
}

pub const MAX_RECENT_EVENTS: usize = 500;

impl RuntimeState {
    pub fn touch(&mut self) {
        self.last_updated = Utc::now();
    }

    pub fn push_recent_event(&mut self, event: HardwareEvent) {
        self.recent_events.push(event);
        self.trim_recent_events();
    }

    pub fn trim_recent_events(&mut self) {
        if self.recent_events.len() > MAX_RECENT_EVENTS {
            let overflow = self.recent_events.len() - MAX_RECENT_EVENTS;
            self.recent_events.drain(0..overflow);
        }
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
