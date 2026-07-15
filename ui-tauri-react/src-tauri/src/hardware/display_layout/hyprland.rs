use super::*;

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

fn migrate_workspaces_from(connectors: &[String], target: &str) -> Result<(), String> {
    let output = compositor::command_output("hyprctl", &["workspaces", "-j"])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid Hyprland workspace JSON: {e}"))?;
    for workspace in value.as_array().into_iter().flatten() {
        let monitor = workspace
            .get("monitor")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if connectors.iter().any(|name| name == monitor) {
            if let Some(id) = workspace.get("id").and_then(|v| v.as_i64()) {
                run_command(
                    "hyprctl",
                    &[
                        "dispatch",
                        "moveworkspacetomonitor",
                        &format!("{id} {target}"),
                    ],
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn apply_hyprland_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("Refusing to disable every display".into());
    }
    let available = hyprland_output_names()?;
    let omitted = omitted_output_names(layout, &available);
    let target = layout
        .displays
        .iter()
        .find(|d| d.primary)
        .unwrap_or(&layout.displays[0]);
    for display in &layout.displays {
        run_command("hyprctl", &["keyword", "monitor", &monitor_rule(display)])?;
    }
    migrate_workspaces_from(&omitted, &target.connector)?;
    for connector in omitted {
        run_command(
            "hyprctl",
            &["keyword", "monitor", &format!("{connector},disable")],
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
}
