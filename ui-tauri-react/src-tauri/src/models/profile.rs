use serde::{Deserialize, Serialize};

use super::{DisplayLayout, Orientation};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub backlight_level: u8,
    pub scale: f64,
    pub orientation: Orientation,
    pub dual_screen_enabled: bool,
    pub display_layout: Option<DisplayLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
}

impl Profile {
    pub fn default_profiles() -> Vec<Profile> {
        vec![
            Profile {
                id: "docked".into(),
                name: "Docked".into(),
                backlight_level: 3,
                scale: 1.66,
                orientation: Orientation::Normal,
                dual_screen_enabled: false,
                display_layout: None,
            },
            Profile {
                id: "tablet".into(),
                name: "Tablet".into(),
                backlight_level: 0,
                scale: 1.66,
                orientation: Orientation::Normal,
                dual_screen_enabled: true,
                display_layout: None,
            },
            Profile {
                id: "presentation".into(),
                name: "Presentation".into(),
                backlight_level: 3,
                scale: 1.66,
                orientation: Orientation::Normal,
                dual_screen_enabled: true,
                display_layout: None,
            },
        ]
    }
}
