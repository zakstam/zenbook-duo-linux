//! The lower-screen key (right of F12): cycles which internal screens are on.
//!
//! Keyboard detached: both -> top only -> bottom only -> both off -> both.
//! Keyboard attached (bottom screen covered): top on <-> top off.
//! GNOME only: which panels are on is read from Mutter's logical monitors, and
//! "off" uses Mutter's display power saving. With external screens on, power
//! saving would blank them too, so "off" turns the panels off in the layout
//! instead; the external screens are never changed.

use std::process::Command;
use std::time::Duration;

use crate::hardware::duo::{PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR};
use crate::ipc::protocol::SessionBackend;
use crate::models::DisplayLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenTarget {
    TopOnly,
    BottomOnly,
    Both,
    /// Neither panel: only external screens.
    NoPanels,
}

/// What one key press does: an optional layout, then optionally turning the
/// panels on (before the layout) or off (after it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenCycleAction {
    pub layout: Option<ScreenTarget>,
    pub power_on: bool,
    pub power_off: bool,
}

const NOTHING: ScreenCycleAction = ScreenCycleAction {
    layout: None,
    power_on: false,
    power_off: false,
};

pub(crate) fn next_screen_cycle_action(
    attached: bool,
    top_on: bool,
    bottom_on: bool,
    powered_off: bool,
    can_power_off: bool,
    externals: bool,
) -> ScreenCycleAction {
    if externals {
        return next_action_with_externals(attached, top_on, bottom_on, powered_off);
    }
    if attached {
        // The bottom screen is under the keyboard: never leave it on.
        let layout = bottom_on.then_some(ScreenTarget::TopOnly);
        return if powered_off {
            ScreenCycleAction { layout, power_on: true, ..NOTHING }
        } else {
            ScreenCycleAction { layout, power_off: can_power_off, ..NOTHING }
        };
    }
    if powered_off {
        // The layout was reset to both before going dark.
        return ScreenCycleAction { power_on: true, ..NOTHING };
    }
    match (top_on, bottom_on) {
        (true, true) => ScreenCycleAction { layout: Some(ScreenTarget::TopOnly), ..NOTHING },
        (true, false) => ScreenCycleAction { layout: Some(ScreenTarget::BottomOnly), ..NOTHING },
        // Reset to both before going dark, so any wake-up starts the cycle over.
        _ => ScreenCycleAction {
            layout: Some(ScreenTarget::Both),
            power_off: can_power_off,
            ..NOTHING
        },
    }
}

/// The cycle with external screens on: the panels go off through the layout.
fn next_action_with_externals(
    attached: bool,
    top_on: bool,
    bottom_on: bool,
    powered_off: bool,
) -> ScreenCycleAction {
    let layout = if attached {
        // The bottom screen is under the keyboard: never turn it on.
        if top_on && !bottom_on && !powered_off {
            ScreenTarget::NoPanels
        } else {
            ScreenTarget::TopOnly
        }
    } else if powered_off {
        ScreenTarget::Both
    } else {
        match (top_on, bottom_on) {
            (true, true) => ScreenTarget::TopOnly,
            (true, false) => ScreenTarget::BottomOnly,
            (false, true) => ScreenTarget::NoPanels,
            (false, false) => ScreenTarget::Both,
        }
    };
    ScreenCycleAction {
        layout: Some(layout),
        power_on: powered_off,
        power_off: false,
    }
}

pub(crate) fn screen_layout(
    base: &DisplayLayout,
    target: ScreenTarget,
    scale: f64,
) -> Option<DisplayLayout> {
    match target {
        ScreenTarget::TopOnly => super::dock_layout_from_base(base, true, scale),
        ScreenTarget::Both => super::dock_layout_from_base(base, false, scale),
        ScreenTarget::NoPanels => Some(DisplayLayout { displays: vec![] }),
        ScreenTarget::BottomOnly => {
            let both = super::dock_layout_from_base(base, false, scale)?;
            let mut bottom = both
                .displays
                .into_iter()
                .find(|display| display.connector == SECONDARY_INTERNAL_CONNECTOR)?;
            bottom.x = 0;
            bottom.y = 0;
            bottom.primary = true;
            Some(DisplayLayout {
                displays: vec![bottom],
            })
        }
    }
}

pub(crate) fn cycle_screens(
    attached: bool,
    scale: f64,
    backend: SessionBackend,
) -> Result<(), String> {
    if backend != SessionBackend::Gnome {
        return Err("The lower-screen key cycle is only supported on GNOME".into());
    }
    // The layout reader also lists disabled internal panels (so layouts can be
    // built for them); whether a panel is on comes from the logical monitors.
    let layout = crate::hardware::display_layout::get_display_layout()?;
    let active = gnome_active_connectors()?;
    let is_on = |connector: &str| active.iter().any(|c| c == connector);
    let powered_off = gnome_panels_powered_off().unwrap_or(false);
    let externals = !super::external_screens::active_externals(&layout, &active).is_empty();
    let action = next_screen_cycle_action(
        attached,
        is_on(PRIMARY_INTERNAL_CONNECTOR),
        is_on(SECONDARY_INTERNAL_CONNECTOR),
        powered_off,
        true,
        externals,
    );

    if action.power_on {
        set_gnome_panels_powered(true)?;
    }
    if let Some(target) = action.layout {
        let panels = screen_layout(&layout, target, scale)
            .ok_or_else(|| format!("No internal display layout for {target:?}"))?;
        let next = super::external_screens::with_externals(panels, &layout, &active);
        crate::hardware::display_layout::apply_display_layout(&next)?;
        super::apply_touch_mapping_best_effort("screen cycle", backend);
    }
    if action.power_off {
        // Let the compositor finish the layout change before blanking.
        std::thread::sleep(Duration::from_millis(500));
        set_gnome_panels_powered(false)?;
    } else if let Some(osd) = cycle_osd(action, attached) {
        crate::runtime::osd::show_in_session(&osd);
    }
    Ok(())
}

/// The pop-up naming the screens that are on after a key press (none when the
/// panels go dark).
pub(crate) fn cycle_osd(action: ScreenCycleAction, attached: bool) -> Option<crate::runtime::osd::Osd> {
    use crate::runtime::osd::Osd;
    let target = match action.layout {
        Some(target) => target,
        // Woken from power saving: attached the top screen, else both.
        None if action.power_on && attached => ScreenTarget::TopOnly,
        None if action.power_on => ScreenTarget::Both,
        None => return None,
    };
    Some(match target {
        ScreenTarget::Both => Osd::new("video-joined-displays-symbolic", "Both screens"),
        ScreenTarget::TopOnly => Osd::new("video-display-symbolic", "Top screen only"),
        ScreenTarget::BottomOnly => Osd::new("video-display-symbolic", "Bottom screen only"),
        ScreenTarget::NoPanels => Osd::new("video-display-symbolic", "External screens only"),
    })
}

/// Connectors that are part of a logical monitor, i.e. turned on.
pub(crate) fn gnome_active_connectors() -> Result<Vec<String>, String> {
    let output = Command::new("gdctl")
        .arg("show")
        .output()
        .map_err(|e| format!("Failed to run gdctl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(parse_active_connectors(&String::from_utf8_lossy(&output.stdout)))
}

/// The connectors listed under "Logical monitors:" in `gdctl show`.
fn parse_active_connectors(output: &str) -> Vec<String> {
    output
        .lines()
        .skip_while(|line| !line.starts_with("Logical monitors"))
        .filter_map(|line| {
            let item = line.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
            let connector = item.split_whitespace().next()?;
            let looks_like_connector = connector.contains('-')
                && connector.chars().last().is_some_and(|c| c.is_ascii_digit());
            (looks_like_connector && item.contains('(')).then(|| connector.to_string())
        })
        .collect()
}

const DISPLAY_CONFIG: [&str; 6] = [
    "--session",
    "--dest",
    "org.gnome.Mutter.DisplayConfig",
    "--object-path",
    "/org/gnome/Mutter/DisplayConfig",
    "--method",
];

fn gnome_panels_powered_off() -> Result<bool, String> {
    let output = Command::new("gdbus")
        .arg("call")
        .args(DISPLAY_CONFIG)
        .args([
            "org.freedesktop.DBus.Properties.Get",
            "org.gnome.Mutter.DisplayConfig",
            "PowerSaveMode",
        ])
        .output()
        .map_err(|e| format!("Failed to run gdbus: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(parse_power_save_mode(&String::from_utf8_lossy(&output.stdout)) != Some(0))
}

/// `gdbus` prints the property as `(<0>,)`.
fn parse_power_save_mode(output: &str) -> Option<i32> {
    let digits: String = output.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Whether the screen key turned the screens off (Mutter power saving), as
/// opposed to GNOME after the screen-blank delay. The keyboard backlight
/// follows only the latter (see backlight_idle::watch_screen_power).
pub(crate) static PANELS_OFF_BY_KEY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Turns the panels on (Mutter power save mode 0) or off (mode 3).
fn set_gnome_panels_powered(on: bool) -> Result<(), String> {
    PANELS_OFF_BY_KEY.store(!on, std::sync::atomic::Ordering::SeqCst);
    let status = Command::new("gdbus")
        .arg("call")
        .args(DISPLAY_CONFIG)
        .args([
            "org.freedesktop.DBus.Properties.Set",
            "org.gnome.Mutter.DisplayConfig",
            "PowerSaveMode",
            if on { "<int32 0>" } else { "<int32 3>" },
        ])
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("Failed to run gdbus: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Setting display power save mode failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DisplayInfo, DisplayMode, RefreshPolicy};

    fn action(layout: Option<ScreenTarget>, power_on: bool, power_off: bool) -> ScreenCycleAction {
        ScreenCycleAction { layout, power_on, power_off }
    }

    #[test]
    fn detached_cycle_goes_both_top_bottom_off_and_back() {
        let next = |top, bottom, off| next_screen_cycle_action(false, top, bottom, off, true, false);
        assert_eq!(next(true, true, false), action(Some(ScreenTarget::TopOnly), false, false));
        assert_eq!(next(true, false, false), action(Some(ScreenTarget::BottomOnly), false, false));
        assert_eq!(next(false, true, false), action(Some(ScreenTarget::Both), false, true));
        assert_eq!(next(true, true, true), action(None, true, false));
    }

    #[test]
    fn detached_cycle_skips_off_without_power_saving() {
        assert_eq!(
            next_screen_cycle_action(false, false, true, false, false, false),
            action(Some(ScreenTarget::Both), false, false)
        );
    }

    #[test]
    fn attached_key_toggles_the_top_screen_and_keeps_the_bottom_off() {
        let next = |top, bottom, off| next_screen_cycle_action(true, top, bottom, off, true, false);
        assert_eq!(next(true, false, false), action(None, false, true));
        assert_eq!(next(true, false, true), action(None, true, false));
        // Attached while the layout still had both screens (e.g. attached while dark).
        assert_eq!(next(true, true, true), action(Some(ScreenTarget::TopOnly), true, false));
        assert_eq!(next(true, true, false), action(Some(ScreenTarget::TopOnly), false, true));
    }

    #[test]
    fn with_external_screens_the_panels_go_off_in_the_layout_not_by_power_saving() {
        let next = |top, bottom, off| next_screen_cycle_action(false, top, bottom, off, true, true);
        assert_eq!(next(true, true, false), action(Some(ScreenTarget::TopOnly), false, false));
        assert_eq!(next(true, false, false), action(Some(ScreenTarget::BottomOnly), false, false));
        assert_eq!(next(false, true, false), action(Some(ScreenTarget::NoPanels), false, false));
        assert_eq!(next(false, false, false), action(Some(ScreenTarget::Both), false, false));
        // Still dark from before the external screen was plugged in.
        assert_eq!(next(true, true, true), action(Some(ScreenTarget::Both), true, false));
    }

    #[test]
    fn attached_with_external_screens_toggles_the_top_panel_in_the_layout() {
        let next = |top, bottom, off| next_screen_cycle_action(true, top, bottom, off, true, true);
        assert_eq!(next(true, false, false), action(Some(ScreenTarget::NoPanels), false, false));
        assert_eq!(next(false, false, false), action(Some(ScreenTarget::TopOnly), false, false));
        // Never turns the covered bottom screen on.
        assert_eq!(next(true, true, false), action(Some(ScreenTarget::TopOnly), false, false));
    }

    #[test]
    fn the_pop_up_names_the_screens_that_are_on() {
        let label = |action, attached| cycle_osd(action, attached).map(|osd| osd.label);
        assert_eq!(label(action(Some(ScreenTarget::TopOnly), false, false), false).as_deref(), Some("Top screen only"));
        assert_eq!(label(action(None, true, false), true).as_deref(), Some("Top screen only"));
        assert_eq!(label(action(None, true, false), false).as_deref(), Some("Both screens"));
        assert_eq!(label(action(Some(ScreenTarget::NoPanels), false, false), false).as_deref(), Some("External screens only"));
        assert_eq!(label(action(None, false, true), true), None);
    }

    #[test]
    fn active_connectors_come_from_the_logical_monitors_only() {
        let output = "Monitors:\n├──Monitor eDP-1 (Built-in display)\n│  └──Vendor: SDC\n└──Monitor eDP-2 (Built-in display)\n\nLogical monitors:\n└──Logical monitor #1\n   ├──Position: (0, 0)\n   ├──Scale: 1.6666666269302368\n   ├──Primary: yes\n   └──Monitors: (1)\n       └──eDP-1 (Built-in display)\n";
        assert_eq!(parse_active_connectors(output), vec!["eDP-1".to_string()]);
        let both = format!("{output}└──Logical monitor #2\n   └──Monitors: (1)\n       └──eDP-2 (Built-in display)\n");
        assert_eq!(parse_active_connectors(&both), vec!["eDP-1".to_string(), "eDP-2".to_string()]);
    }

    #[test]
    fn parses_gdbus_power_save_mode() {
        assert_eq!(parse_power_save_mode("(<0>,)\n"), Some(0));
        assert_eq!(parse_power_save_mode("(<3>,)\n"), Some(3));
        assert_eq!(parse_power_save_mode(""), None);
    }

    fn panel(connector: &str, primary: bool, y: i32) -> DisplayInfo {
        let mode = DisplayMode {
            width: 2880,
            height: 1800,
            refresh_rate: 120.0,
            ..Default::default()
        };
        DisplayInfo {
            connector: connector.into(),
            width: 2880,
            height: 1800,
            refresh_rate: 120.0,
            scale: 1.666667,
            x: 0,
            y,
            transform: 0,
            primary,
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::default(),
            supports_dynamic_refresh: false,
        }
    }

    fn connectors(layout: &DisplayLayout) -> Vec<(String, bool, i32)> {
        layout
            .displays
            .iter()
            .map(|d| (d.connector.clone(), d.primary, d.y))
            .collect()
    }

    #[test]
    fn bottom_only_layout_puts_the_bottom_panel_at_the_origin() {
        let base = DisplayLayout {
            displays: vec![panel("eDP-1", true, 0), panel("eDP-2", false, 1080)],
        };
        let bottom = screen_layout(&base, ScreenTarget::BottomOnly, 1.666667).unwrap();
        assert_eq!(connectors(&bottom), vec![("eDP-2".to_string(), true, 0)]);
    }

    #[test]
    fn top_only_and_both_layouts_restore_the_top_panel_from_bottom_only() {
        let base = DisplayLayout {
            displays: vec![panel("eDP-2", true, 0)],
        };
        let top = screen_layout(&base, ScreenTarget::TopOnly, 1.666667).unwrap();
        assert_eq!(connectors(&top), vec![("eDP-1".to_string(), true, 0)]);
        let both = screen_layout(&base, ScreenTarget::Both, 1.666667).unwrap();
        assert_eq!(both.displays.len(), 2);
        assert_eq!(both.displays[0].connector, "eDP-1");
        assert_eq!(both.displays[1].connector, "eDP-2");
        assert!(both.displays[1].y > 0);
    }
}
