//! The display scales the app offers, and the migration of saved scales onto them.
//!
//! Every preset is a multiple of 1/120 (the Wayland fractional-scale step) and
//! divides the Duo's 2880x1800 panels into whole logical pixels. Mutter 51 offers
//! exactly these below 2.25 at 2880x1800, and niri, Hyprland and KWin keep them
//! as given. Off-grid values such as 1.66 or 1.75 are snapped by each desktop to a
//! value of its own, so positions computed from the requested scale drift.
//! Keep in step with `src/shared/display-scales.ts`.

/// The scales the dropdowns and the installer offer, ascending.
pub const DISPLAY_SCALE_PRESETS: [f64; 6] = [1.0, 1.25, 4.0 / 3.0, 1.5, 5.0 / 3.0, 2.0];

use crate::features::profiles::model::Profile;
use crate::hardware::duo::{PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR};
use crate::models::{DisplayLayout, DuoSettings, DEFAULT_SCALE_FACTOR};

/// The default scale for the Duo panels: 1728x1080 logical pixels.
pub const DEFAULT_DISPLAY_SCALE: f64 = DEFAULT_SCALE_FACTOR;

/// A reported scale this close to a preset is that preset: desktops report 5/3
/// as 1.6666666 (f32), niri applied the old 1.66 as 1.6583, the installer writes 1.67.
const PRESET_TOLERANCE: f64 = 0.01;

/// Scales earlier versions offered that no longer exist, and the preset each
/// becomes. 1.75 has no clean neighbour GNOME accepts other than 5/3.
const RETIRED_SCALES: [(f64, f64); 1] = [(1.75, 5.0 / 3.0)];

/// The preset a saved or reported scale stands for, or the scale unchanged when
/// it is none (a value typed into the installer, or a desktop's own choice).
pub fn canonical_display_scale(scale: f64) -> f64 {
    if !scale.is_finite() {
        return scale;
    }
    for (retired, replacement) in RETIRED_SCALES {
        if (scale - retired).abs() < PRESET_TOLERANCE {
            return replacement;
        }
    }
    DISPLAY_SCALE_PRESETS
        .into_iter()
        .find(|preset| (scale - preset).abs() < PRESET_TOLERANCE)
        .unwrap_or(scale)
}

/// Moves the built-in screens' saved scales onto the presets, so someone who
/// saved 1.66 or 1.75 sees 1.67x and gets a scale every desktop keeps. External
/// monitors keep whatever they were given.
pub fn migrate_layout_scales(layout: &mut DisplayLayout) {
    for display in &mut layout.displays {
        if display.connector == PRIMARY_INTERNAL_CONNECTOR
            || display.connector == SECONDARY_INTERNAL_CONNECTOR
        {
            display.scale = canonical_display_scale(display.scale);
        }
    }
}

/// See [`migrate_layout_scales`]; the default scale is the built-in screens'.
pub fn migrate_settings_scales(settings: &mut DuoSettings) {
    settings.default_scale = canonical_display_scale(settings.default_scale);
    if let Some(layout) = settings.saved_display_layout.as_mut() {
        migrate_layout_scales(layout);
    }
}

/// See [`migrate_layout_scales`]; a profile only holds the built-in screens.
pub fn migrate_profile_scales(profile: &mut Profile) {
    profile.scale = canonical_display_scale(profile.scale);
    profile.bottom_scale = profile.bottom_scale.map(canonical_display_scale);
}

/// Logical pixels covered by `physical` pixels at `scale`, rounded up so a
/// neighbour placed after it never overlaps (1800 / 1.6583 = 1085.43 -> 1086).
/// Float noise is not a fraction: at 5/3 reported as f32, 1800 / 1.6666666 is
/// 1080.00002, which is 1080, not 1081.
pub fn ceil_logical_pixels(physical: f64, scale: f64) -> i64 {
    let exact = physical / scale.max(0.1);
    (exact - exact.abs() * 1e-6).ceil() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUO_PANEL: (f64, f64) = (2880.0, 1800.0);

    fn is_whole(value: f64) -> bool {
        (value - value.round()).abs() < 1e-9
    }

    #[test]
    fn every_preset_divides_the_duo_panel_into_whole_pixels_on_the_120th_grid() {
        for scale in DISPLAY_SCALE_PRESETS {
            assert!(
                is_whole(scale * 120.0),
                "{scale} is not a multiple of 1/120"
            );
            assert!(
                is_whole(DUO_PANEL.0 / scale) && is_whole(DUO_PANEL.1 / scale),
                "{scale} gives {}x{}",
                DUO_PANEL.0 / scale,
                DUO_PANEL.1 / scale
            );
        }
        assert!(DISPLAY_SCALE_PRESETS.windows(2).all(|w| w[0] < w[1]));
        assert!(DISPLAY_SCALE_PRESETS.contains(&DEFAULT_DISPLAY_SCALE));
    }

    #[test]
    fn retired_and_reported_scales_become_their_preset() {
        let five_thirds = 5.0 / 3.0;
        for saved in [
            1.66,
            1.67,
            1.75,
            1.6583333333333334,
            1.6666666269302368,
            1.666667,
        ] {
            assert_eq!(canonical_display_scale(saved), five_thirds, "{saved}");
        }
        assert_eq!(canonical_display_scale(1.3333333), 4.0 / 3.0);
        assert_eq!(canonical_display_scale(1.25), 1.25);
        assert_eq!(canonical_display_scale(1.1), 1.1);
    }

    #[test]
    fn logical_pixels_ignore_float_noise_but_round_real_fractions_up() {
        assert_eq!(ceil_logical_pixels(1800.0, 1.6666666269302368), 1080);
        assert_eq!(ceil_logical_pixels(1800.0, 1.6666666), 1080);
        assert_eq!(ceil_logical_pixels(1800.0, 5.0 / 3.0), 1080);
        assert_eq!(ceil_logical_pixels(1800.0, 1.6583333333333334), 1086);
        assert_eq!(ceil_logical_pixels(1800.0, 1.66), 1085);
        assert_eq!(ceil_logical_pixels(1800.0, 1.5), 1200);
    }
}
