use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuoSettings {
    #[serde(default = "default_backlight")]
    pub default_backlight: u8,
    #[serde(default = "default_scale")]
    pub default_scale: f64,
    #[serde(default)]
    pub auto_dual_screen: bool,
    #[serde(default)]
    pub sync_brightness: bool,
    #[serde(default)]
    pub theme: ThemePreference,
    #[serde(default = "default_usb_media_remap_enabled")]
    pub usb_media_remap_enabled: bool,
    #[serde(default)]
    pub setup_completed: bool,
}

impl Default for DuoSettings {
    fn default() -> Self {
        Self {
            default_backlight: default_backlight(),
            default_scale: default_scale(),
            auto_dual_screen: true,
            sync_brightness: true,
            theme: ThemePreference::System,
            usb_media_remap_enabled: default_usb_media_remap_enabled(),
            setup_completed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

fn default_backlight() -> u8 {
    3
}

fn default_scale() -> f64 {
    1.66
}

fn default_usb_media_remap_enabled() -> bool {
    true
}
