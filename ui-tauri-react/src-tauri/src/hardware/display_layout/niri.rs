use super::*;

pub(super) fn get_niri_display_layout() -> Result<DisplayLayout, String> {
    let value = compositor::niri_outputs_json()?;
    let outputs = compositor::niri_outputs_from_value(&value)?;

    fn parse_niri_mode(value: &serde_json::Value) -> Option<DisplayMode> {
        Some(make_display_mode(
            value.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            value.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            value
                .get("refresh_rate")
                .and_then(|v| v.as_f64())
                .map(|rate| rate / 1000.0)?,
        ))
    }

    let mut displays = Vec::new();
    for output in outputs {
        if output
            .get("current_mode")
            .is_some_and(|value| value.is_null())
        {
            continue;
        }

        let connector = output
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing niri output name".to_string())?;
        let logical = output
            .get("logical")
            .and_then(|v| v.as_object())
            .ok_or_else(|| format!("Missing niri logical geometry for {connector}"))?;
        let current_mode = resolve_niri_current_mode(&output)
            .and_then(|value| parse_niri_mode(&value))
            .ok_or_else(|| format!("Missing niri current mode for {connector}"))?;
        let available_modes = dedupe_modes(
            output
                .get("modes")
                .and_then(|v| v.as_array())
                .map(|modes| modes.iter().filter_map(parse_niri_mode).collect())
                .unwrap_or_else(|| vec![current_mode.clone()]),
        );
        // Niri reports VRR support on the Duo panels, but enabling it can hard-freeze the
        // machine on this hardware. Keep the control disabled until there is a known-safe path.
        let supports_dynamic_refresh = false;
        let refresh_policy = RefreshPolicy::Fixed;

        displays.push(DisplayInfo {
            connector: connector.to_string(),
            width: current_mode.width,
            height: current_mode.height,
            refresh_rate: current_mode.refresh_rate,
            scale: logical.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0),
            x: logical.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            y: logical.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            transform: parse_niri_transform(&output),
            primary: is_primary_internal_connector(connector),
            current_mode,
            available_modes,
            refresh_policy,
            supports_dynamic_refresh,
        });
    }

    Ok(DisplayLayout { displays })
}

pub(super) fn resolve_niri_current_mode(output: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(mode) = output.get("current_mode").and_then(|v| v.as_object()) {
        return Some(serde_json::Value::Object(mode.clone()));
    }

    let index = output.get("current_mode").and_then(|v| v.as_u64())? as usize;
    output
        .get("modes")
        .and_then(|v| v.as_array())
        .and_then(|modes| modes.get(index))
        .cloned()
}

pub(super) fn parse_niri_transform(output: &serde_json::Value) -> u32 {
    output
        .get("logical")
        .and_then(|value| value.get("transform"))
        .or_else(|| output.get("transform"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase())
        .map(|value| match value.as_str() {
            "90" => 90,
            "180" => 180,
            "270" => 270,
            "flipped" | "inverted" => 180,
            _ => 0,
        })
        .unwrap_or(0)
}

pub(super) fn apply_niri_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    let available_outputs = niri_output_names()?;
    for connector in omitted_output_names(layout, &available_outputs) {
        run_niri_command(&["msg", "output", &connector, "off"])?;
    }

    for display in &layout.displays {
        run_niri_command(&["msg", "output", &display.connector, "on"])?;
        match display.refresh_policy {
            RefreshPolicy::Dynamic => {
                return Err(format!(
                    "Dynamic refresh is disabled on Niri because it is unstable on this hardware ({})",
                    display.connector
                ));
            }
            RefreshPolicy::Fixed => {
                run_niri_command(&["msg", "output", &display.connector, "vrr", "off"])?;
            }
        }
        run_niri_command(&[
            "msg",
            "output",
            &display.connector,
            "mode",
            &display.current_mode.mode_id,
        ])?;
        run_niri_command(&[
            "msg",
            "output",
            &display.connector,
            "transform",
            match display.transform {
                90 => "90",
                180 => "180",
                270 => "270",
                _ => "normal",
            },
        ])?;
        run_niri_command(&[
            "msg",
            "output",
            &display.connector,
            "scale",
            &format!("{:.6}", display.scale.max(0.1)),
        ])?;
        run_niri_position_command(&display.connector, display.x, display.y)?;
    }

    Ok(())
}

fn niri_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    compositor::niri_output_logical_size_from_value(&compositor::niri_outputs_json()?, name)
}

fn niri_enabled_output_count() -> Result<usize, String> {
    compositor::niri_enabled_output_count_from_value(&compositor::niri_outputs_json()?)
}

fn niri_transform_token(orientation: &Orientation) -> &'static str {
    match orientation {
        Orientation::Left => "90",
        Orientation::Right => "270",
        Orientation::Inverted => "180",
        Orientation::Normal => "normal",
    }
}

fn secondary_position(
    orientation: &Orientation,
    primary_size: Result<(i64, i64), String>,
) -> Result<(i64, i64), String> {
    let (width, height) = primary_size?;
    Ok(match orientation {
        Orientation::Left => (-width, 0),
        Orientation::Right => (width, 0),
        Orientation::Inverted => (0, -height),
        Orientation::Normal => (0, height),
    })
}

pub(super) fn set_niri_orientation(orientation: &Orientation) -> Result<(), String> {
    let token = niri_transform_token(orientation);
    run_niri_command(&[
        "msg",
        "output",
        PRIMARY_INTERNAL_CONNECTOR,
        "transform",
        token,
    ])?;
    run_niri_position_command(PRIMARY_INTERNAL_CONNECTOR, 0, 0)?;

    if niri_enabled_output_count().unwrap_or(1) <= 1 {
        return Ok(());
    }

    run_niri_command(&[
        "msg",
        "output",
        SECONDARY_INTERNAL_CONNECTOR,
        "transform",
        token,
    ])?;
    thread::sleep(Duration::from_millis(300));

    let (pos_x, pos_y) = secondary_position(
        orientation,
        niri_output_logical_size(PRIMARY_INTERNAL_CONNECTOR),
    )?;

    run_niri_position_command(SECONDARY_INTERNAL_CONNECTOR, pos_x as i32, pos_y as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_primary_geometry_is_an_error_for_secondary_positioning() {
        let result = secondary_position(&Orientation::Normal, Err("missing geometry".into()));

        assert_eq!(result.expect_err("missing geometry should fail"), "missing geometry");
    }

    #[test]
    fn secondary_position_uses_primary_geometry() {
        assert_eq!(
            secondary_position(&Orientation::Right, Ok((1200, 800))).expect("position"),
            (1200, 0)
        );
        assert_eq!(
            secondary_position(&Orientation::Normal, Ok((1200, 800))).expect("position"),
            (0, 800)
        );
    }
}

