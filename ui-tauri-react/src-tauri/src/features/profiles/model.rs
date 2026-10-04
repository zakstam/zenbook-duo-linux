use serde::{Deserialize, Serialize};

use crate::models::Orientation;

/// A named hardware configuration for the Duo's two built-in screens.
///
/// `scale` and `orientation` belong to the top screen. The bottom screen has its
/// own scale and rotation; a profile saved before those existed has neither, and
/// the bottom screen then follows the top one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub backlight_level: u8,
    pub scale: f64,
    pub orientation: Orientation,
    pub dual_screen_enabled: bool,
    #[serde(default)]
    pub bottom_scale: Option<f64>,
    #[serde(default)]
    pub bottom_orientation: Option<Orientation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
}

impl Profile {
    pub fn effective_bottom_scale(&self) -> f64 {
        self.bottom_scale.unwrap_or(self.scale)
    }

    pub fn effective_bottom_orientation(&self) -> Orientation {
        self.bottom_orientation
            .clone()
            .unwrap_or_else(|| self.orientation.clone())
    }

    /// The built-in profiles, at the scale chosen during setup.
    pub fn default_profiles(scale: f64) -> Vec<Profile> {
        vec![
            Profile {
                id: "docked".into(),
                name: "Docked".into(),
                backlight_level: 3,
                scale,
                orientation: Orientation::Normal,
                dual_screen_enabled: false,
                bottom_scale: Some(scale),
                bottom_orientation: Some(Orientation::Normal),
            },
            Profile {
                id: "tablet".into(),
                name: "Tablet".into(),
                backlight_level: 0,
                scale,
                orientation: Orientation::Normal,
                dual_screen_enabled: true,
                bottom_scale: Some(scale),
                bottom_orientation: Some(Orientation::Normal),
            },
            // Laid flat: the top screen is flipped for the person across the table.
            Profile {
                id: "presentation".into(),
                name: "Presentation".into(),
                backlight_level: 3,
                scale,
                orientation: Orientation::Inverted,
                dual_screen_enabled: true,
                bottom_scale: Some(scale),
                bottom_orientation: Some(Orientation::Normal),
            },
        ]
    }
}

pub fn orientation_transform(orientation: &Orientation) -> u32 {
    match orientation {
        Orientation::Normal => 0,
        Orientation::Left => 90,
        Orientation::Inverted => 180,
        Orientation::Right => 270,
    }
}

pub fn transform_orientation(transform: u32) -> Orientation {
    match transform {
        90 => Orientation::Left,
        180 => Orientation::Inverted,
        270 => Orientation::Right,
        _ => Orientation::Normal,
    }
}
