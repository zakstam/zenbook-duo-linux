use crate::hardware::duo::{
    is_internal_connector, is_primary_internal_connector, PRIMARY_INTERNAL_CONNECTOR,
    SECONDARY_INTERNAL_CONNECTOR,
};
mod adapters;
mod gnome;
mod kde;
mod niri;

use crate::ipc::protocol::SessionBackend;
use crate::models::{DisplayInfo, DisplayLayout, DisplayMode, Orientation, RefreshPolicy};
use crate::runtime::{compositor, session};
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

fn format_refresh_rate(refresh_rate: f64) -> String {
    let rounded = (refresh_rate * 1000.0).round() / 1000.0;
    let mut value = format!("{rounded:.3}");
    while value.contains('.') && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

fn mode_id(width: u32, height: u32, refresh_rate: f64) -> String {
    format!("{width}x{height}@{}", format_refresh_rate(refresh_rate))
}

fn make_display_mode(width: u32, height: u32, refresh_rate: f64) -> DisplayMode {
    make_display_mode_with_backend_id(width, height, refresh_rate, None)
}

fn make_display_mode_with_backend_id(
    width: u32,
    height: u32,
    refresh_rate: f64,
    backend_mode_id: Option<String>,
) -> DisplayMode {
    DisplayMode {
        mode_id: mode_id(width, height, refresh_rate),
        backend_mode_id,
        width,
        height,
        refresh_rate,
    }
}

fn dedupe_modes(modes: Vec<DisplayMode>) -> Vec<DisplayMode> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for mode in modes {
        if seen.insert(mode.mode_id.clone()) {
            deduped.push(mode);
        }
    }
    deduped
}

fn omitted_output_names(layout: &DisplayLayout, available_outputs: &[String]) -> Vec<String> {
    let desired = layout
        .displays
        .iter()
        .map(|display| display.connector.as_str())
        .collect::<HashSet<_>>();

    available_outputs
        .iter()
        .filter(|name| !desired.contains(name.as_str()))
        .cloned()
        .collect()
}

fn kde_output_names() -> Result<Vec<String>, String> {
    compositor::kde_output_names_from_value(&compositor::kscreen_json()?)
}

fn niri_output_names() -> Result<Vec<String>, String> {
    compositor::niri_output_names_from_value(&compositor::niri_outputs_json()?)
}

fn stacked_logical_height(display: &DisplayInfo) -> i32 {
    let rotated = display.transform == 90 || display.transform == 270;
    let physical_height = if rotated {
        display.width
    } else {
        display.height
    };
    let scale = display.scale.max(0.1);
    (physical_height as f64 / scale).ceil() as i32
}

pub fn normalize_display_layout(layout: DisplayLayout) -> DisplayLayout {
    let Some(top_display) = layout
        .displays
        .iter()
        .find(|display| is_primary_internal_connector(&display.connector))
        .cloned()
    else {
        return layout;
    };

    let top_logical_height = stacked_logical_height(&top_display);
    let shift_x = top_display.x;
    let shift_y = top_display.y;

    let displays = layout
        .displays
        .into_iter()
        .map(|display| {
            if is_primary_internal_connector(&display.connector) {
                return DisplayInfo {
                    x: 0,
                    y: 0,
                    ..display
                };
            }

            if display.connector == SECONDARY_INTERNAL_CONNECTOR {
                return DisplayInfo {
                    x: 0,
                    y: top_logical_height,
                    ..display
                };
            }

            DisplayInfo {
                x: display.x - shift_x,
                y: display.y - shift_y,
                ..display
            }
        })
        .collect();

    DisplayLayout { displays }
}


/// Get the current display layout through the selected compositor Adapter.
pub fn get_display_layout() -> Result<DisplayLayout, String> {
    let layout = adapters::with_display_adapter(detect_backend(), |adapter| adapter.layout())?;

    Ok(normalize_display_layout(layout))
}

/// Apply a display layout through the selected compositor Adapter.
pub fn apply_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    let normalized = normalize_display_layout(layout.clone());
    adapters::with_display_adapter(detect_backend(), |adapter| adapter.apply_layout(&normalized))
}

fn detect_backend() -> SessionBackend {
    session::detect_backend_from_env()
}

fn run_command<S: AsRef<str>>(program: &str, args: &[S]) -> Result<(), String> {
    compositor::run_command(program, args)
}

fn run_niri_command(args: &[&str]) -> Result<(), String> {
    compositor::run_niri_command(args)
}

fn run_niri_position_command(connector: &str, x: i32, y: i32) -> Result<(), String> {
    let x_arg = x.to_string();
    let y_arg = y.to_string();
    run_niri_command(&[
        "msg", "output", connector, "position", "set", "--", &x_arg, &y_arg,
    ])
}

/// Set screen orientation through the selected compositor Adapter.
pub fn set_orientation(orientation: &Orientation) -> Result<(), String> {
    adapters::with_display_adapter(detect_backend(), |adapter| adapter.set_orientation(orientation))
        .map_err(|message| {
            if message == "Unsupported session backend for display layout" {
                "Unsupported session backend for orientation control".to_string()
            } else {
                message
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::adapters::CompositorDisplayAdapter;

    fn test_display(connector: &str) -> DisplayInfo {
        let mode = make_display_mode(2880, 1800, 120.0);
        DisplayInfo {
            connector: connector.to_string(),
            width: mode.width,
            height: mode.height,
            refresh_rate: mode.refresh_rate,
            scale: 1.66,
            x: 0,
            y: 0,
            transform: 0,
            primary: is_primary_internal_connector(connector),
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        }
    }

    struct FakeAdapter;

    impl adapters::CompositorDisplayAdapter for FakeAdapter {
        fn layout(&self) -> Result<DisplayLayout, String> {
            Ok(DisplayLayout {
                displays: vec![test_display(PRIMARY_INTERNAL_CONNECTOR)],
            })
        }

        fn apply_layout(&self, layout: &DisplayLayout) -> Result<(), String> {
            if layout.displays.is_empty() {
                Err("empty layout".into())
            } else {
                Ok(())
            }
        }

        fn set_orientation(&self, _orientation: &Orientation) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn display_adapter_interface_covers_layout_apply_and_orientation() {
        let adapter = FakeAdapter;
        assert_eq!(adapter.layout().expect("layout").displays.len(), 1);
        assert!(adapter
            .apply_layout(&DisplayLayout {
                displays: vec![test_display(PRIMARY_INTERNAL_CONNECTOR)],
            })
            .is_ok());
        assert!(adapter.set_orientation(&Orientation::Left).is_ok());
    }

    #[test]
    fn finds_all_outputs_omitted_from_requested_layout() {
        let layout = DisplayLayout {
            displays: vec![test_display(PRIMARY_INTERNAL_CONNECTOR)],
        };
        let available = vec![
            PRIMARY_INTERNAL_CONNECTOR.to_string(),
            SECONDARY_INTERNAL_CONNECTOR.to_string(),
            "HDMI-A-1".to_string(),
        ];

        assert_eq!(
            omitted_output_names(&layout, &available),
            vec![SECONDARY_INTERNAL_CONNECTOR, "HDMI-A-1"]
        );
    }

    #[test]
    fn external_only_layout_omits_internal_outputs_for_clamshell() {
        let layout = DisplayLayout {
            displays: vec![test_display("HDMI-A-1")],
        };
        let available = vec![
            PRIMARY_INTERNAL_CONNECTOR.to_string(),
            SECONDARY_INTERNAL_CONNECTOR.to_string(),
            "HDMI-A-1".to_string(),
        ];

        assert_eq!(
            omitted_output_names(&layout, &available),
            vec![PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR]
        );
    }

    #[test]
    fn parses_kde_output_names_for_disable_replay() {
        let value = serde_json::json!({
            "outputs": [
                { "name": "eDP-1", "enabled": true },
                { "name": "eDP-2", "enabled": true }
            ]
        });

        assert_eq!(
            compositor::kde_output_names_from_value(&value).expect("names should parse"),
            vec!["eDP-1".to_string(), "eDP-2".to_string()]
        );
    }

    #[test]
    fn parses_niri_output_names_for_disable_replay() {
        let value = serde_json::json!({
            "eDP-1": { "name": "eDP-1" },
            "eDP-2": { "name": "eDP-2" }
        });

        assert_eq!(
            compositor::niri_output_names_from_value(&value).expect("names should parse"),
            vec!["eDP-1".to_string(), "eDP-2".to_string()]
        );
    }

    #[test]
    fn formats_mode_ids_without_trailing_zeroes() {
        assert_eq!(mode_id(2880, 1800, 120.0), "2880x1800@120");
        assert_eq!(mode_id(2880, 1800, 60.001), "2880x1800@60.001");
    }

    #[test]
    fn parses_gdctl_modes_into_current_and_available_modes() {
        let output = r#"Monitors:
├──Monitor eDP-1 (Built-in display)
│  ├──Current mode
│  │   └──2880x1800@120.000
│  └──Preferences
│      ├──2880x1800@120.000
│      └──2880x1800@60.000
Logical monitors:
└──Logical monitor #1
    ├──Position: 0, 0
    ├──Scale: 1.750000
    ├──Transform: normal
    ├──Primary: yes
    └──eDP-1 (Built-in display)
"#;

        let layout = gnome::parse_gdctl_output(output).expect("gdctl output should parse");
        let display = layout.displays.first().expect("display should exist");

        assert_eq!(display.current_mode.mode_id, "2880x1800@120");
        assert_eq!(
            display.current_mode.backend_mode_id.as_deref(),
            Some("2880x1800@120.000")
        );
        assert_eq!(display.available_modes.len(), 2);
        assert_eq!(display.available_modes[1].mode_id, "2880x1800@60");
        assert_eq!(
            display.available_modes[1].backend_mode_id.as_deref(),
            Some("2880x1800@60.000")
        );
        assert_eq!(display.refresh_policy, RefreshPolicy::Fixed);
    }

    #[test]
    fn gnome_parser_skips_non_logical_external_monitors_but_keeps_internal_panels() {
        let output = r#"Monitors:
├──Monitor eDP-1 (Built-in display)
│  └──Current mode
│      └──2880x1800@120.000
├──Monitor eDP-2 (Built-in display)
│  └──Preferences
│      └──2880x1800@120.000
└──Monitor HDMI-A-1 (External display)
   └──Preferences
       └──1920x1080@60.000
Logical monitors:
└──Logical monitor #1
    ├──Position: 0, 0
    ├──Scale: 1.000000
    ├──Transform: normal
    ├──Primary: yes
    └──eDP-1 (Built-in display)
"#;

        let layout = gnome::parse_gdctl_output(output).expect("gdctl output should parse");
        let connectors = layout
            .displays
            .iter()
            .map(|display| display.connector.as_str())
            .collect::<Vec<_>>();

        assert_eq!(connectors, vec!["eDP-1", "eDP-2"]);
        assert_eq!(layout.displays[1].y, 1800);
    }

    #[test]
    fn gnome_parser_keeps_external_monitors_that_have_logical_entries() {
        let output = r#"Monitors:
├──Monitor eDP-1 (Built-in display)
│  └──Current mode
│      └──2880x1800@120.000
└──Monitor HDMI-A-1 (External display)
   └──Current mode
       └──1920x1080@60.000
Logical monitors:
├──Logical monitor #1
│   ├──Position: 0, 0
│   ├──Scale: 1.000000
│   ├──Transform: normal
│   ├──Primary: yes
│   └──eDP-1 (Built-in display)
└──Logical monitor #2
    ├──Position: 2880, 0
    ├──Scale: 1.000000
    ├──Transform: normal
    ├──Primary: no
    └──HDMI-A-1 (External display)
"#;

        let layout = gnome::parse_gdctl_output(output).expect("gdctl output should parse");
        let external = layout
            .displays
            .iter()
            .find(|display| display.connector == "HDMI-A-1")
            .expect("active external should be included");

        assert_eq!(external.x, 2880);
        assert!(!external.primary);
    }

    #[test]
    fn gnome_mode_arg_prefers_backend_token_and_falls_back_to_current_modes() {
        let mut display = test_display(PRIMARY_INTERNAL_CONNECTOR);
        display.current_mode.backend_mode_id = Some("2880x1800@120.000".into());
        assert_eq!(gnome::gnome_mode_arg(&display, None), "2880x1800@120.000");

        display.current_mode.backend_mode_id = None;
        display.available_modes[0].backend_mode_id = None;
        let current_layout = DisplayLayout {
            displays: vec![DisplayInfo {
                current_mode: make_display_mode_with_backend_id(
                    2880,
                    1800,
                    120.0,
                    Some("2880x1800@120.000".into()),
                ),
                available_modes: vec![make_display_mode_with_backend_id(
                    2880,
                    1800,
                    120.0,
                    Some("2880x1800@120.000".into()),
                )],
                ..test_display(PRIMARY_INTERNAL_CONNECTOR)
            }],
        };

        assert_eq!(
            gnome::gnome_mode_arg(&display, Some(&current_layout)),
            "2880x1800@120.000"
        );
    }

    #[test]
    fn parses_niri_transform_from_logical_object() {
        let value = serde_json::json!({
            "logical": { "transform": "Normal" }
        });
        assert_eq!(niri::parse_niri_transform(&value), 0);

        let value = serde_json::json!({
            "logical": { "transform": "90" }
        });
        assert_eq!(niri::parse_niri_transform(&value), 90);
    }

    #[test]
    fn falls_back_to_top_level_niri_transform() {
        let value = serde_json::json!({
            "transform": "270"
        });
        assert_eq!(niri::parse_niri_transform(&value), 270);
    }

    #[test]
    fn resolves_niri_current_mode_from_index() {
        let value = serde_json::json!({
            "current_mode": 1,
            "modes": [
                { "width": 1920, "height": 1200, "refresh_rate": 60000 },
                { "width": 2880, "height": 1800, "refresh_rate": 120000 }
            ]
        });

        let mode = niri::resolve_niri_current_mode(&value).expect("mode should resolve");
        assert_eq!(mode.get("width").and_then(|v| v.as_u64()), Some(2880));
        assert_eq!(mode.get("height").and_then(|v| v.as_u64()), Some(1800));
    }
}
