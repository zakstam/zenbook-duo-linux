use super::*;
use std::path::Path;

const UPPER_CONNECTOR_ENV: &str = "ZENBOOK_DUO_UPPER_CONNECTOR";
const LOWER_CONNECTOR_ENV: &str = "ZENBOOK_DUO_LOWER_CONNECTOR";

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedConnectors {
    upper: String,
    lower: String,
}

impl OwnedConnectors {
    fn contains(&self, connector: &str) -> bool {
        connector == self.upper || connector == self.lower
    }
}

fn connected_edp_connectors(root: &Path) -> Vec<String> {
    let mut connectors = std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let path = entry.path();
            let status = std::fs::read_to_string(path.join("status")).ok()?;
            if status.trim() != "connected" {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let connector = name
                .split_once("-eDP-")
                .map(|(_, suffix)| format!("eDP-{suffix}"))?;
            Some(connector)
        })
        .collect::<Vec<_>>();
    connectors.sort();
    connectors.dedup();
    connectors
}

fn owned_connectors_from(
    upper_override: Option<String>,
    lower_override: Option<String>,
    connected_edp: &[String],
) -> Result<OwnedConnectors, String> {
    let upper = upper_override.unwrap_or_else(|| {
        if connected_edp
            .iter()
            .any(|name| name == PRIMARY_INTERNAL_CONNECTOR)
        {
            PRIMARY_INTERNAL_CONNECTOR.to_string()
        } else {
            connected_edp
                .first()
                .cloned()
                .unwrap_or_else(|| PRIMARY_INTERNAL_CONNECTOR.to_string())
        }
    });
    let lower = lower_override.unwrap_or_else(|| {
        if connected_edp
            .iter()
            .any(|name| name == SECONDARY_INTERNAL_CONNECTOR)
        {
            SECONDARY_INTERNAL_CONNECTOR.to_string()
        } else {
            connected_edp
                .iter()
                .find(|name| **name != upper)
                .cloned()
                .unwrap_or_else(|| SECONDARY_INTERNAL_CONNECTOR.to_string())
        }
    });
    if upper.is_empty() || lower.is_empty() || upper == lower {
        return Err("Hyprland internal connector overrides must name two distinct outputs".into());
    }
    Ok(OwnedConnectors { upper, lower })
}

fn owned_connectors() -> Result<OwnedConnectors, String> {
    owned_connectors_from(
        std::env::var(UPPER_CONNECTOR_ENV).ok(),
        std::env::var(LOWER_CONNECTOR_ENV).ok(),
        &connected_edp_connectors(Path::new("/sys/class/drm")),
    )
}

fn transform_degrees(value: i64) -> u32 {
    match value {
        1 => 90,
        2 => 180,
        3 => 270,
        _ => 0,
    }
}

fn transform_value(value: u32) -> i64 {
    match value {
        90 => 1,
        180 => 2,
        270 => 3,
        _ => 0,
    }
}

fn parse_mode(value: &serde_json::Value) -> Option<DisplayMode> {
    let width = value.get("width")?.as_u64()? as u32;
    let height = value.get("height")?.as_u64()? as u32;
    let refresh = value
        .get("refreshRate")
        .or_else(|| value.get("refresh_rate"))?
        .as_f64()?;
    Some(make_display_mode(width, height, refresh))
}

pub(super) fn layout_from_value(value: &serde_json::Value) -> Result<DisplayLayout, String> {
    let outputs = compositor::hyprland_outputs_from_value(value)?;
    let mut displays = Vec::new();
    for output in outputs {
        let connector = output
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing Hyprland output name".to_string())?;
        let current_mode =
            parse_mode(&output).ok_or_else(|| format!("Missing Hyprland mode for {connector}"))?;
        let available_modes = dedupe_modes(
            output
                .get("availableModes")
                .and_then(|v| v.as_array())
                .map(|modes| {
                    modes
                        .iter()
                        .filter_map(|mode| {
                            let raw = mode.as_str()?;
                            let (size, rate) = raw.split_once('@')?;
                            let (width, height) = size.split_once('x')?;
                            Some(make_display_mode(
                                width.parse().ok()?,
                                height.parse().ok()?,
                                rate.trim_end_matches("Hz").parse().ok()?,
                            ))
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![current_mode.clone()]),
        );
        displays.push(DisplayInfo {
            connector: connector.to_string(),
            width: current_mode.width,
            height: current_mode.height,
            refresh_rate: current_mode.refresh_rate,
            scale: output.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0),
            x: output.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            y: output.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            transform: transform_degrees(
                output
                    .get("transform")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            ),
            primary: output
                .get("focused")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            current_mode,
            available_modes,
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        });
    }
    if displays.is_empty() {
        return Err("Hyprland reported no enabled monitors".into());
    }
    if !displays.iter().any(|display| display.primary) {
        displays[0].primary = true;
    }
    Ok(DisplayLayout { displays })
}

pub(super) fn get_hyprland_display_layout() -> Result<DisplayLayout, String> {
    layout_from_value(&compositor::hyprland_monitors_json()?)
}

fn monitor_rule(display: &DisplayInfo) -> String {
    format!(
        "{},{}x{}@{},{}x{},{},transform,{}",
        display.connector,
        display.current_mode.width,
        display.current_mode.height,
        format_refresh_rate(display.current_mode.refresh_rate),
        display.x,
        display.y,
        format_refresh_rate(display.scale.max(0.1)),
        transform_value(display.transform)
    )
}

fn workspace_moves_from_value(
    value: &serde_json::Value,
    lower: &str,
    target: &str,
) -> Result<Vec<String>, String> {
    let workspaces = value
        .as_array()
        .ok_or_else(|| "Unexpected Hyprland workspaces shape".to_string())?;
    Ok(workspaces
        .iter()
        .filter_map(|workspace| {
            let monitor = workspace.get("monitor").and_then(|v| v.as_str())?;
            let id = workspace.get("id").and_then(|v| v.as_i64())?;
            (monitor == lower).then(|| format!("{id} {target}"))
        })
        .collect())
}

fn migrate_lower_workspaces(lower: &str, target: &str) -> Result<(), String> {
    let output = compositor::command_output("hyprctl", &["workspaces", "-j"])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid Hyprland workspace JSON: {e}"))?;
    for move_arg in workspace_moves_from_value(&value, lower, target)? {
        run_command(
            "hyprctl",
            &["dispatch", "moveworkspacetomonitor", &move_arg],
        )?;
    }
    Ok(())
}

#[derive(Debug)]
struct MutationPlan<'a> {
    configure: Vec<&'a DisplayInfo>,
    disable_lower: bool,
}

fn mutation_plan<'a>(
    layout: &'a DisplayLayout,
    owned: &OwnedConnectors,
) -> Result<MutationPlan<'a>, String> {
    let configure = layout
        .displays
        .iter()
        .filter(|display| owned.contains(&display.connector))
        .collect::<Vec<_>>();
    if configure.is_empty() {
        return Err("Refusing Hyprland layout with no owned internal display enabled".into());
    }
    let upper_enabled = configure
        .iter()
        .any(|display| display.connector == owned.upper);
    if !upper_enabled {
        return Err("Refusing to disable the upper internal display".into());
    }
    let lower_enabled = configure
        .iter()
        .any(|display| display.connector == owned.lower);
    Ok(MutationPlan {
        configure,
        disable_lower: !lower_enabled,
    })
}

pub(super) fn apply_hyprland_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("Refusing to disable every display".into());
    }
    let owned = owned_connectors()?;
    let plan = mutation_plan(layout, &owned)?;
    let target = plan
        .configure
        .iter()
        .find(|display| display.connector == owned.upper)
        .copied()
        .unwrap_or(plan.configure[0]);
    for display in plan.configure {
        run_command("hyprctl", &["keyword", "monitor", &monitor_rule(display)])?;
    }
    if plan.disable_lower {
        migrate_lower_workspaces(&owned.lower, &target.connector)?;
        run_command(
            "hyprctl",
            &["keyword", "monitor", &format!("{},disable", owned.lower)],
        )?;
    }
    Ok(())
}

pub(super) fn set_hyprland_orientation(orientation: &Orientation) -> Result<(), String> {
    let mut layout = get_hyprland_display_layout()?;
    let transform = match orientation {
        Orientation::Left => 90,
        Orientation::Right => 270,
        Orientation::Inverted => 180,
        Orientation::Normal => 0,
    };
    for display in &mut layout.displays {
        display.transform = transform;
    }
    apply_hyprland_display_layout(&layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(connector: &str, primary: bool) -> DisplayInfo {
        let mode = make_display_mode(2880, 1800, 120.0);
        DisplayInfo {
            connector: connector.into(),
            width: mode.width,
            height: mode.height,
            refresh_rate: mode.refresh_rate,
            scale: 1.67,
            x: 0,
            y: 0,
            transform: 0,
            primary,
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        }
    }

    fn owned() -> OwnedConnectors {
        OwnedConnectors {
            upper: "eDP-1".into(),
            lower: "eDP-2".into(),
        }
    }

    fn mutation_connectors(layout: &DisplayLayout) -> Vec<String> {
        let plan = mutation_plan(layout, &owned()).expect("owned plan");
        let mut connectors = plan
            .configure
            .iter()
            .map(|display| display.connector.clone())
            .collect::<Vec<_>>();
        if plan.disable_lower {
            connectors.push(owned().lower);
        }
        connectors
    }

    #[test]
    fn parses_unusual_connector_names_and_two_monitors() {
        let value = serde_json::json!([
            {"name":"eDP-3","width":2880,"height":1800,"refreshRate":120.0,"scale":2.0,"x":0,"y":0,"transform":0,"focused":true,"availableModes":["2880x1800@120.00Hz"]},
            {"name":"DP-USB-C-7","width":1920,"height":1080,"refreshRate":60.0,"scale":1.0,"x":1440,"y":0,"transform":1,"focused":false}
        ]);
        let layout = layout_from_value(&value).expect("layout");
        assert_eq!(layout.displays.len(), 2);
        assert_eq!(layout.displays[1].connector, "DP-USB-C-7");
        assert_eq!(layout.displays[1].transform, 90);
    }

    #[test]
    fn rejects_zero_monitor_state() {
        assert!(layout_from_value(&serde_json::json!([])).is_err());
    }

    #[test]
    fn internal_only_attached_and_detached_plans_preserve_duo_behavior() {
        let attached = DisplayLayout {
            displays: vec![display("eDP-1", true)],
        };
        let detached = DisplayLayout {
            displays: vec![display("eDP-1", true), display("eDP-2", false)],
        };

        assert_eq!(mutation_connectors(&attached), vec!["eDP-1", "eDP-2"]);
        assert_eq!(mutation_connectors(&detached), vec!["eDP-1", "eDP-2"]);
        assert!(mutation_plan(&attached, &owned()).unwrap().disable_lower);
        assert!(!mutation_plan(&detached, &owned()).unwrap().disable_lower);
    }

    #[test]
    fn physical_hdmi_topology_mutates_only_owned_internal_panels() {
        let layout = DisplayLayout {
            displays: vec![display("eDP-1", true), display("HDMI-A-1", false)],
        };

        assert_eq!(mutation_connectors(&layout), vec!["eDP-1", "eDP-2"]);
        assert!(!monitor_rule(&display("eDP-1", true)).contains("HDMI-A-1"));
    }

    #[test]
    fn all_external_topologies_are_discovery_only() {
        for external in [
            "HDMI-A-1",
            "DP-1",
            "DP-USB-C-7",
            "Thunderbolt Dock 42",
            "DisplayLink-9",
            "WL-virtual:odd/name",
        ] {
            for detached in [false, true] {
                let mut displays = vec![display("eDP-1", true), display(external, false)];
                if detached {
                    displays.push(display("eDP-2", false));
                }
                let mutations = mutation_connectors(&DisplayLayout { displays });
                assert!(mutations
                    .iter()
                    .all(|name| name == "eDP-1" || name == "eDP-2"));
                assert!(!mutations.iter().any(|name| name == external));
            }
        }
    }

    #[test]
    fn multiple_active_or_user_disabled_externals_never_enter_mutation_plan() {
        let active = DisplayLayout {
            displays: vec![
                display("eDP-1", true),
                display("HDMI-A-1", false),
                display("DP-4", false),
            ],
        };
        let user_disabled_external_omitted = DisplayLayout {
            displays: vec![display("eDP-1", true)],
        };

        assert_eq!(mutation_connectors(&active), vec!["eDP-1", "eDP-2"]);
        assert_eq!(
            mutation_connectors(&user_disabled_external_omitted),
            vec!["eDP-1", "eDP-2"]
        );
    }

    #[test]
    fn startup_hotplug_unplug_lock_and_lifecycle_replays_share_owned_scope() {
        let before_startup = DisplayLayout {
            displays: vec![display("eDP-1", true), display("HDMI-A-1", false)],
        };
        let after_hotplug = DisplayLayout {
            displays: vec![display("eDP-1", true), display("DP-2", false)],
        };
        let after_unplug = DisplayLayout {
            displays: vec![display("eDP-1", true)],
        };

        for replay in [&before_startup, &after_hotplug, &after_unplug] {
            assert_eq!(mutation_connectors(replay), vec!["eDP-1", "eDP-2"]);
        }
    }

    #[test]
    fn only_lower_internal_workspaces_migrate() {
        let workspaces = serde_json::json!([
            {"id": 1, "monitor": "eDP-1"},
            {"id": 2, "monitor": "eDP-2"},
            {"id": 3, "monitor": "HDMI-A-1"},
            {"id": 4, "monitor": "DP-1"}
        ]);

        assert_eq!(
            workspace_moves_from_value(&workspaces, "eDP-2", "eDP-1").unwrap(),
            vec!["2 eDP-1"]
        );
    }

    #[test]
    fn explicit_internal_connector_overrides_take_precedence() {
        let resolved = owned_connectors_from(
            Some("eDP-upper-custom".into()),
            Some("eDP-lower-custom".into()),
            &["eDP-1".into(), "eDP-2".into()],
        )
        .unwrap();
        assert_eq!(resolved.upper, "eDP-upper-custom");
        assert_eq!(resolved.lower, "eDP-lower-custom");
    }

    #[test]
    fn automatic_detection_uses_only_connected_drm_edp_candidates() {
        let resolved =
            owned_connectors_from(None, None, &["eDP-3".into(), "eDP-4".into()]).unwrap();
        assert_eq!(resolved.upper, "eDP-3");
        assert_eq!(resolved.lower, "eDP-4");
    }

    #[test]
    fn all_disabled_guard_rejects_external_only_or_empty_target() {
        let external_only = DisplayLayout {
            displays: vec![display("HDMI-A-1", true)],
        };
        assert!(mutation_plan(&external_only, &owned()).is_err());
        assert!(mutation_plan(&DisplayLayout { displays: vec![] }, &owned()).is_err());
    }
}
