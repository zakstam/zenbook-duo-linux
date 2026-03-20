use serde::{Deserialize, Serialize};

use crate::commands::usb_media_remap::UsbMediaRemapStatus;
use crate::models::{DisplayLayout, DuoSettings, DuoStatus, HardwareEvent, Orientation};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<T> {
    pub protocol_version: u32,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    Ping,
    HandleLifecycle { phase: LifecyclePhase },
    GetStatus,
    GetDisplayLayout,
    GetSettings,
    SaveSettings { settings: DuoSettings },
    SetBacklight { level: u8 },
    SetOrientation { orientation: Orientation },
    ApplyDisplayLayout { layout: DisplayLayout },
    UsbMediaRemapStatus,
    UsbMediaRemapStart,
    UsbMediaRemapStop,
    UsbMediaRemapTogglePause,
    RestartService,
    RegisterSessionAgent {
        session_id: String,
        backend: SessionBackend,
        socket_path: String,
    },
    TailLogs { lines: usize },
    GetRecentEvents { limit: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Pong,
    Ack,
    Status { status: DuoStatus },
    DisplayLayout { layout: DisplayLayout },
    Settings { settings: DuoSettings },
    UsbMediaRemapStatus { status: UsbMediaRemapStatus },
    Logs { lines: Vec<String> },
    Events { events: Vec<HardwareEvent> },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    Pre,
    Post,
    Hibernate,
    Thaw,
    Boot,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    StatusChanged { status: DuoStatus },
    HardwareEvent { event: HardwareEvent },
    SessionAgentChanged { connected: bool, backend: Option<SessionBackend> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionBackend {
    Gnome,
    Kde,
    Hyprland,
    Niri,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::SessionBackend;

    #[test]
    fn session_backend_serializes_hyprland_in_snake_case() {
        let json = serde_json::to_string(&SessionBackend::Hyprland).unwrap();
        assert_eq!(json, "\"hyprland\"");
    }

    #[test]
    fn session_backend_deserializes_hyprland_from_snake_case() {
        let backend: SessionBackend = serde_json::from_str("\"hyprland\"").unwrap();
        assert_eq!(backend, SessionBackend::Hyprland);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionCommand {
    GetDisplayLayout,
    SetDockMode { attached: bool, scale: f64 },
    ApplyDisplayLayout { layout: DisplayLayout },
    SetOrientation { orientation: Orientation },
    OpenEmojiPicker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionResponse {
    Ack,
    DisplayLayout { layout: DisplayLayout },
    Error { message: String },
}
