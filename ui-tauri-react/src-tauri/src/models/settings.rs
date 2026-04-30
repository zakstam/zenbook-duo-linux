use serde::{Deserialize, Serialize};

use super::DisplayLayout;

pub const DEFAULT_BACKLIGHT_LEVEL: u8 = 0;
pub const DEFAULT_SCALE_FACTOR: f64 = 1.66;
pub const DEFAULT_USB_MEDIA_REMAP_ENABLED: bool = true;
pub const DEFAULT_START_ON_BOOT_MINIMIZED: bool = false;
pub const DEFAULT_INVERT_SENSOR_ROTATION: bool = false;
pub const DEFAULT_SETUP_COMPLETED: bool = false;

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
    #[serde(default = "default_start_on_boot_minimized")]
    pub start_on_boot_minimized: bool,
    #[serde(default = "default_invert_sensor_rotation")]
    pub invert_sensor_rotation: bool,
    #[serde(default)]
    pub setup_completed: bool,
    #[serde(default)]
    pub touchscreen_disabled: Vec<String>,
    #[serde(default)]
    pub saved_display_layout: Option<DisplayLayout>,
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
            start_on_boot_minimized: default_start_on_boot_minimized(),
            invert_sensor_rotation: default_invert_sensor_rotation(),
            setup_completed: DEFAULT_SETUP_COMPLETED,
            touchscreen_disabled: Vec::new(),
            saved_display_layout: None,
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
    DEFAULT_BACKLIGHT_LEVEL
}

fn default_scale() -> f64 {
    DEFAULT_SCALE_FACTOR
}

fn default_usb_media_remap_enabled() -> bool {
    DEFAULT_USB_MEDIA_REMAP_ENABLED
}

fn default_start_on_boot_minimized() -> bool {
    DEFAULT_START_ON_BOOT_MINIMIZED
}

fn default_invert_sensor_rotation() -> bool {
    DEFAULT_INVERT_SENSOR_ROTATION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_installer_and_frontend_contract() {
        let settings = DuoSettings::default();

        assert_eq!(settings.default_backlight, DEFAULT_BACKLIGHT_LEVEL);
        assert_eq!(settings.default_scale, DEFAULT_SCALE_FACTOR);
        assert_eq!(
            settings.usb_media_remap_enabled,
            DEFAULT_USB_MEDIA_REMAP_ENABLED
        );
        assert_eq!(
            settings.start_on_boot_minimized,
            DEFAULT_START_ON_BOOT_MINIMIZED
        );
        assert_eq!(
            settings.invert_sensor_rotation,
            DEFAULT_INVERT_SENSOR_ROTATION
        );
        assert_eq!(settings.setup_completed, DEFAULT_SETUP_COMPLETED);
        assert!(settings.auto_dual_screen);
        assert!(settings.sync_brightness);
        assert_eq!(settings.theme, ThemePreference::System);
    }
}
