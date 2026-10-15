//! Spots the Duo lying opened flat on a table with both screens facing up
//! (ASUS's Sharing posture), where the top screen turns around for the person
//! opposite while the bottom one stays as it is.
//!
//! monitor-sensor reports nothing when the device goes flat (it keeps the last
//! edge orientation), so this reads the top panel's raw accelerometer axes from
//! sysfs. Measured on a UX8406MA: lying flat reads z at about -0.99 g; laptop,
//! kickstand and on-its-side postures all read z between -0.32 g and 0.

use std::fs;
use std::path::{Path, PathBuf};

use crate::hardware::duo::{PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR};
use crate::models::DisplayLayout;

/// Share of gravity through the top screen's face at or above which it counts
/// as lying flat (within about 16 degrees of level)...
const ENTER_FACE_UP: f64 = 0.96;
/// ...and below which it has been picked up again (about 26 degrees); the gap
/// stops the layout flickering at the edge.
const LEAVE_FACE_UP: f64 = 0.90;
/// Face-up readings in a row before the layout changes, so tilting the device
/// through flat on the way to another posture changes nothing.
const ENTER_READINGS: u8 = 2;

const TRANSFORM_NORMAL: u32 = 0;
const TRANSFORM_INVERTED: u32 = 180;

/// The IIO accelerometer in the top panel, if the sensor hub exposes one.
pub(super) fn find_accelerometer() -> Option<PathBuf> {
    fs::read_dir("/sys/bus/iio/devices")
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            fs::read_to_string(path.join("name"))
                .map(|name| name.trim() == "accel_3d")
                .unwrap_or(false)
        })
}

/// One raw (x, y, z) reading; units do not matter, only their ratio.
pub(super) fn read_accelerometer(device: &Path) -> Option<(f64, f64, f64)> {
    let axis = |name: &str| -> Option<f64> {
        fs::read_to_string(device.join(format!("in_accel_{name}_raw")))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    Some((axis("x")?, axis("y")?, axis("z")?))
}

/// How much of gravity points out through the top screen's face, from -1
/// (face down, e.g. lid closed) to 1 (face up). The panel's z axis points out
/// of its back, so a face-up screen reads negative z. None for an all-zero
/// reading, which a sensor that is not answering returns.
pub(super) fn face_up_share((x, y, z): (f64, f64, f64)) -> Option<f64> {
    let magnitude = (x * x + y * y + z * z).sqrt();
    (magnitude > 0.0).then(|| -z / magnitude)
}

#[derive(Debug, Default)]
pub(super) struct LyingFlatDetector {
    lying_flat: bool,
    face_up_readings: u8,
}

impl LyingFlatDetector {
    pub(super) fn lying_flat(&self) -> bool {
        self.lying_flat
    }

    /// Feeds one accelerometer reading; returns the new posture when it changes.
    pub(super) fn update(&mut self, reading: (f64, f64, f64)) -> Option<bool> {
        let share = face_up_share(reading)?;
        if self.lying_flat {
            if share < LEAVE_FACE_UP {
                self.lying_flat = false;
                self.face_up_readings = 0;
                return Some(false);
            }
            return None;
        }
        if share < ENTER_FACE_UP {
            self.face_up_readings = 0;
            return None;
        }
        self.face_up_readings = self.face_up_readings.saturating_add(1);
        if self.face_up_readings >= ENTER_READINGS {
            self.lying_flat = true;
            return Some(true);
        }
        None
    }
}

/// The current layout with only the top screen turned around: top inverted,
/// bottom upright (when it is on), each keeping its scale and mode. This is the
/// Presentation profile's arrangement; normalize_display_layout stacks the
/// bottom screen under the top one. External monitors are left as they are.
pub(super) fn lying_flat_layout(current: &DisplayLayout) -> DisplayLayout {
    let mut layout = current.clone();
    for display in &mut layout.displays {
        let transform = match display.connector.as_str() {
            PRIMARY_INTERNAL_CONNECTOR => TRANSFORM_INVERTED,
            SECONDARY_INTERNAL_CONNECTOR => TRANSFORM_NORMAL,
            _ => continue,
        };
        display.transform = transform;
        display.x = 0;
        display.y = 0;
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DisplayInfo, DisplayMode, RefreshPolicy};

    // Raw readings from a UX8406MA, keyboard off.
    const LAPTOP: (f64, f64, f64) = (17928.0, -1034231.0, -182864.0);
    const LAPTOP_LID_BACK: (f64, f64, f64) = (19881.0, -997124.0, -315668.0);
    const KICKSTAND: (f64, f64, f64) = (19881.0, -1053761.0, 8530.0);
    const ON_ITS_SIDE: (f64, f64, f64) = (1019817.0, -215924.0, 718.0);
    const LYING_FLAT: (f64, f64, f64) = (19881.0, -81167.0, -987500.0);
    const FACE_DOWN: (f64, f64, f64) = (0.0, -50000.0, 990000.0);
    // About 22 degrees from level: between the enter and leave thresholds.
    const NEARLY_FLAT: (f64, f64, f64) = (0.0, -375000.0, -930000.0);

    fn detector_lying_flat() -> LyingFlatDetector {
        let mut detector = LyingFlatDetector::default();
        detector.update(LYING_FLAT);
        assert_eq!(detector.update(LYING_FLAT), Some(true));
        detector
    }

    #[test]
    fn lying_flat_is_recognised_after_two_face_up_readings() {
        let mut detector = LyingFlatDetector::default();
        assert_eq!(detector.update(LYING_FLAT), None);
        assert!(!detector.lying_flat());
        assert_eq!(detector.update(LYING_FLAT), Some(true));
        assert!(detector.lying_flat());
        assert_eq!(detector.update(LYING_FLAT), None);
    }

    #[test]
    fn other_postures_never_count_as_lying_flat() {
        let mut detector = LyingFlatDetector::default();
        for reading in [LAPTOP, LAPTOP_LID_BACK, KICKSTAND, ON_ITS_SIDE, FACE_DOWN] {
            for _ in 0..5 {
                assert_eq!(detector.update(reading), None, "{reading:?}");
            }
        }
        assert!(!detector.lying_flat());
    }

    #[test]
    fn tilting_through_flat_for_one_reading_changes_nothing() {
        let mut detector = LyingFlatDetector::default();
        assert_eq!(detector.update(LYING_FLAT), None);
        assert_eq!(detector.update(LAPTOP), None);
        assert_eq!(detector.update(LYING_FLAT), None);
        assert!(!detector.lying_flat());
    }

    #[test]
    fn picking_it_up_ends_lying_flat_with_a_margin() {
        let mut detector = detector_lying_flat();
        assert_eq!(detector.update(NEARLY_FLAT), None);
        assert!(detector.lying_flat());
        assert_eq!(detector.update(KICKSTAND), Some(false));
        assert!(!detector.lying_flat());

        let mut fresh = LyingFlatDetector::default();
        assert_eq!(fresh.update(NEARLY_FLAT), None);
        assert_eq!(fresh.update(NEARLY_FLAT), None);
    }

    #[test]
    fn a_sensor_reading_all_zeros_is_ignored() {
        let mut detector = detector_lying_flat();
        assert_eq!(face_up_share((0.0, 0.0, 0.0)), None);
        assert_eq!(detector.update((0.0, 0.0, 0.0)), None);
        assert!(detector.lying_flat());
    }

    fn display(connector: &str, transform: u32, x: i32, y: i32) -> DisplayInfo {
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
            scale: 1.66,
            x,
            y,
            transform,
            primary: connector == PRIMARY_INTERNAL_CONNECTOR,
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        }
    }

    fn transform_of(layout: &DisplayLayout, connector: &str) -> Option<u32> {
        layout
            .displays
            .iter()
            .find(|display| display.connector == connector)
            .map(|display| display.transform)
    }

    #[test]
    fn lying_flat_turns_only_the_top_screen() {
        let current = DisplayLayout {
            displays: vec![
                display(PRIMARY_INTERNAL_CONNECTOR, 90, 0, 0),
                display(SECONDARY_INTERNAL_CONNECTOR, 90, 1084, 0),
                display("HDMI-A-1", 90, 2168, 0),
            ],
        };
        let layout = lying_flat_layout(&current);
        assert_eq!(transform_of(&layout, PRIMARY_INTERNAL_CONNECTOR), Some(180));
        assert_eq!(transform_of(&layout, SECONDARY_INTERNAL_CONNECTOR), Some(0));
        assert_eq!(transform_of(&layout, "HDMI-A-1"), Some(90));
        let bottom = &layout.displays[1];
        assert_eq!((bottom.x, bottom.y, bottom.scale), (0, 0, 1.66));
    }

    #[test]
    fn lying_flat_with_the_bottom_screen_off_keeps_it_off() {
        let current = DisplayLayout {
            displays: vec![display(PRIMARY_INTERNAL_CONNECTOR, 0, 0, 0)],
        };
        let layout = lying_flat_layout(&current);
        assert_eq!(layout.displays.len(), 1);
        assert_eq!(transform_of(&layout, PRIMARY_INTERNAL_CONNECTOR), Some(180));
    }
}
