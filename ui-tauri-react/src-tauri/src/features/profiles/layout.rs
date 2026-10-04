//! Converts between a profile and the display layout of the Duo's two built-in screens.

use crate::hardware::duo::{
    is_internal_connector, PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR,
};
use crate::models::{DisplayInfo, DisplayLayout, Orientation};

use super::model::{orientation_transform, Profile};

/// The layout a profile asks for, built on the screens as they are now.
///
/// The top screen takes the profile's scale and rotation; the bottom screen is
/// included only when the profile turns it on (an output the layout leaves out is
/// switched off by every compositor adapter). A bottom screen that is off right now
/// borrows the top screen's mode, since both panels are identical. External
/// monitors are kept as they are.
pub fn profile_layout(profile: &Profile, current: &DisplayLayout) -> Result<DisplayLayout, String> {
    let find = |connector: &str| {
        current
            .displays
            .iter()
            .find(|display| display.connector == connector)
            .cloned()
    };
    let as_connector = |display: &DisplayInfo, connector: &str| DisplayInfo {
        connector: connector.to_string(),
        ..display.clone()
    };

    let mut top = find(PRIMARY_INTERNAL_CONNECTOR)
        .or_else(|| {
            find(SECONDARY_INTERNAL_CONNECTOR)
                .map(|bottom| as_connector(&bottom, PRIMARY_INTERNAL_CONNECTOR))
        })
        .ok_or_else(|| "No built-in screen is on, so the profile cannot be applied".to_string())?;
    top.scale = profile.scale.max(0.1);
    top.transform = orientation_transform(&profile.orientation);
    top.x = 0;
    top.y = 0;

    let mut displays = vec![top.clone()];

    if profile.dual_screen_enabled {
        let mut bottom = find(SECONDARY_INTERNAL_CONNECTOR)
            .unwrap_or_else(|| as_connector(&top, SECONDARY_INTERNAL_CONNECTOR));
        bottom.scale = profile.effective_bottom_scale().max(0.1);
        bottom.transform = orientation_transform(&profile.effective_bottom_orientation());
        bottom.primary = false;
        // normalize_display_layout stacks the bottom screen under the top one.
        bottom.x = 0;
        bottom.y = 0;
        displays.push(bottom);
    }

    displays.extend(
        current
            .displays
            .iter()
            .filter(|display| !is_internal_connector(&display.connector))
            .cloned(),
    );

    if !displays.iter().any(|display| display.primary) {
        displays[0].primary = true;
    }

    Ok(DisplayLayout { displays })
}

/// When both screens share a rotation other than normal they sit side by side,
/// which a stacked layout cannot express; the orientation command arranges them.
pub fn shared_rotation(profile: &Profile) -> Option<Orientation> {
    let rotated = profile.orientation != Orientation::Normal;
    let shared = !profile.dual_screen_enabled
        || profile.effective_bottom_orientation() == profile.orientation;
    (rotated && shared).then(|| profile.orientation.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DisplayMode, RefreshPolicy};

    fn display(connector: &str, scale: f64, transform: u32) -> DisplayInfo {
        let mode = DisplayMode {
            mode_id: "2880x1800@120".into(),
            backend_mode_id: None,
            width: 2880,
            height: 1800,
            refresh_rate: 120.0,
        };
        DisplayInfo {
            connector: connector.into(),
            width: 2880,
            height: 1800,
            refresh_rate: 120.0,
            scale,
            x: 0,
            y: 0,
            transform,
            primary: connector == PRIMARY_INTERNAL_CONNECTOR,
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        }
    }

    fn profile(dual: bool) -> Profile {
        Profile {
            id: "p".into(),
            name: "P".into(),
            backlight_level: 1,
            scale: 1.5,
            orientation: Orientation::Normal,
            dual_screen_enabled: dual,
            bottom_scale: None,
            bottom_orientation: None,
        }
    }

    fn connectors(layout: &DisplayLayout) -> Vec<&str> {
        layout.displays.iter().map(|d| d.connector.as_str()).collect()
    }

    #[test]
    fn single_screen_profile_drops_the_bottom_screen_at_the_profile_scale() {
        let current = DisplayLayout {
            displays: vec![
                display(PRIMARY_INTERNAL_CONNECTOR, 1.0, 0),
                display(SECONDARY_INTERNAL_CONNECTOR, 1.0, 0),
            ],
        };

        let layout = profile_layout(&profile(false), &current).expect("layout");

        assert_eq!(connectors(&layout), vec![PRIMARY_INTERNAL_CONNECTOR]);
        assert_eq!(layout.displays[0].scale, 1.5);
        assert!(layout.displays[0].primary);
    }

    #[test]
    fn dual_screen_profile_turns_an_off_bottom_screen_on_with_the_top_mode() {
        let current = DisplayLayout {
            displays: vec![display(PRIMARY_INTERNAL_CONNECTOR, 1.0, 0)],
        };

        let layout = profile_layout(&profile(true), &current).expect("layout");

        assert_eq!(
            connectors(&layout),
            vec![PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR]
        );
        let bottom = &layout.displays[1];
        assert_eq!(bottom.scale, 1.5);
        assert_eq!(bottom.width, 2880);
        assert!(!bottom.primary);
    }

    #[test]
    fn each_screen_takes_its_own_scale_and_rotation() {
        let current = DisplayLayout {
            displays: vec![
                display(PRIMARY_INTERNAL_CONNECTOR, 1.0, 0),
                display(SECONDARY_INTERNAL_CONNECTOR, 1.0, 0),
            ],
        };
        let presentation = Profile {
            orientation: Orientation::Inverted,
            bottom_scale: Some(1.25),
            bottom_orientation: Some(Orientation::Normal),
            ..profile(true)
        };

        let layout = profile_layout(&presentation, &current).expect("layout");

        assert_eq!(layout.displays[0].transform, 180);
        assert_eq!(layout.displays[0].scale, 1.5);
        assert_eq!(layout.displays[1].transform, 0);
        assert_eq!(layout.displays[1].scale, 1.25);
        assert_eq!(shared_rotation(&presentation), None);
    }

    #[test]
    fn external_monitors_are_kept_and_keep_primary() {
        let mut external = display("HDMI-A-1", 1.0, 0);
        external.primary = true;
        external.x = 1920;
        let mut top = display(PRIMARY_INTERNAL_CONNECTOR, 1.0, 0);
        top.primary = false;
        let current = DisplayLayout {
            displays: vec![top, external],
        };

        let layout = profile_layout(&profile(false), &current).expect("layout");

        assert_eq!(connectors(&layout), vec![PRIMARY_INTERNAL_CONNECTOR, "HDMI-A-1"]);
        assert!(!layout.displays[0].primary);
        assert!(layout.displays[1].primary);
        assert_eq!(layout.displays[1].x, 1920);
    }

    #[test]
    fn no_built_in_screen_is_an_error() {
        let current = DisplayLayout {
            displays: vec![display("HDMI-A-1", 1.0, 0)],
        };
        assert!(profile_layout(&profile(true), &current).is_err());
    }

    #[test]
    fn a_rotation_shared_by_both_screens_is_handed_to_the_orientation_command() {
        let rotated = Profile {
            orientation: Orientation::Left,
            ..profile(true)
        };
        assert_eq!(shared_rotation(&rotated), Some(Orientation::Left));
        assert_eq!(shared_rotation(&profile(true)), None);
    }
}
