use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DuoStatus {
    pub keyboard_attached: bool,
    pub connection_type: ConnectionType,
    pub monitor_count: u32,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub backlight_level: u8,
    pub display_brightness: u32,
    pub max_brightness: u32,
    pub service_active: bool,
    pub orientation: Orientation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionType {
    Usb,
    Bluetooth,
    #[default]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Normal,
    Left,
    Right,
    Inverted,
}

impl Orientation {
    pub fn as_duo_arg(&self) -> &str {
        match self {
            Orientation::Normal => "normal",
            Orientation::Left => "left-up",
            Orientation::Right => "right-up",
            Orientation::Inverted => "bottom-up",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub connector: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f64,
    pub scale: f64,
    pub x: i32,
    pub y: i32,
    pub transform: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayLayout {
    pub displays: Vec<DisplayInfo>,
}
