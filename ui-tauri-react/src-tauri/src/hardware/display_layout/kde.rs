use super::*;

pub(super) fn get_kde_display_layout() -> Result<DisplayLayout, String> {
    let value = compositor::kscreen_json()?;
    let outputs = compositor::kde_outputs_from_value(&value)?;

    fn parse_kde_mode(value: &serde_json::Value) -> Option<DisplayMode> {
        let width = value
            .get("size")
            .and_then(|size| size.get("width"))
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("width").and_then(|v| v.as_u64()))? as u32;
        let height = value
            .get("size")
            .and_then(|size| size.get("height"))
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("height").and_then(|v| v.as_u64()))? as u32;
        let refresh_rate = value
            .get("refreshRate")
            .and_then(|v| v.as_f64())
            .map(|rate| rate / 1000.0)?;
        Some(make_display_mode(width, height, refresh_rate))
    }

    let mut displays = Vec::new();
    for output in outputs {
        if !output
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let connector = output
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing KDE output name".to_string())?;
        let size = output
            .get("size")
            .and_then(|v| v.as_object())
            .ok_or_else(|| format!("Missing KDE size for {connector}"))?;
        let pos = output
            .get("pos")
            .and_then(|v| v.as_object())
            .ok_or_else(|| format!("Missing KDE position for {connector}"))?;
        let rotation = output
            .get("rotation")
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        let scale = output.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let current_mode = output
            .get("currentMode")
            .and_then(parse_kde_mode)
            .unwrap_or_else(|| {
                make_display_mode(
                    size.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    size.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    output
                        .get("currentMode")
                        .and_then(|mode| mode.get("refreshRate"))
                        .and_then(|v| v.as_f64())
                        .map(|rate| rate / 1000.0)
                        .unwrap_or(60.0),
                )
            });
        let available_modes = dedupe_modes(
            output
                .get("modes")
                .and_then(|v| v.as_array())
                .map(|modes| modes.iter().filter_map(parse_kde_mode).collect())
                .unwrap_or_else(|| vec![current_mode.clone()]),
        );

        displays.push(DisplayInfo {
            connector: connector.to_string(),
            width: current_mode.width,
            height: current_mode.height,
            refresh_rate: current_mode.refresh_rate,
            scale,
            x: pos.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            y: pos.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            transform: match rotation {
                "right" => 90,
                "left" => 270,
                "inverted" => 180,
                _ => 0,
            },
            primary: output.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) == 1,
            current_mode,
            available_modes,
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
        });
    }

    Ok(DisplayLayout { displays })
}

pub(super) fn apply_kde_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    let mut args: Vec<String> = Vec::new();
    let available_outputs = kde_output_names()?;
    for connector in omitted_output_names(layout, &available_outputs) {
        args.push(format!("output.{connector}.disable"));
    }

    for display in &layout.displays {
        if display.refresh_policy == RefreshPolicy::Dynamic {
            return Err(format!(
                "Dynamic refresh is not supported for {} on KDE",
                display.connector
            ));
        }

        args.push(format!("output.{}.enable", display.connector));
        args.push(format!(
            "output.{}.mode.{}",
            display.connector, display.current_mode.mode_id
        ));
        args.push(format!(
            "output.{}.position.{},{}",
            display.connector, display.x, display.y
        ));
        args.push(format!(
            "output.{}.rotation.{}",
            display.connector,
            match display.transform {
                90 => "right",
                180 => "inverted",
                270 => "left",
                _ => "none",
            }
        ));
        args.push(format!(
            "output.{}.scale.{:.6}",
            display.connector,
            display.scale.max(0.1)
        ));
        if display.primary {
            args.push(format!("output.{}.priority.1", display.connector));
        }
    }

    run_command("kscreen-doctor", &args)
}

fn kde_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    compositor::kde_output_logical_size_from_value(&compositor::kscreen_json()?, name)
}

fn kde_enabled_output_count() -> Result<usize, String> {
    compositor::kde_enabled_output_count_from_value(&compositor::kscreen_json()?)
}

fn kde_rotation_token(orientation: &Orientation) -> &'static str {
    match orientation {
        Orientation::Left => "right",
        Orientation::Right => "left",
        Orientation::Inverted => "inverted",
        Orientation::Normal => "none",
    }
}

/// The UX8407AA's primary panel (eDP-1) is physically mounted 180° rotated and
/// KWin does not compensate for the DRM panel-orientation property, so its
/// rotation must be offset by 180° relative to the secondary panel.
fn primary_panel_mounted_inverted() -> bool {
    static FLIPPED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLIPPED.get_or_init(|| {
        std::fs::read_to_string("/sys/class/dmi/id/board_name")
            .map(|name| name.trim() == "UX8407AA")
            .unwrap_or(false)
    })
}

fn kde_primary_rotation_token(orientation: &Orientation, panel_inverted: bool) -> &'static str {
    if !panel_inverted {
        return kde_rotation_token(orientation);
    }
    match orientation {
        Orientation::Left => "left",
        Orientation::Right => "right",
        Orientation::Inverted => "none",
        Orientation::Normal => "inverted",
    }
}

/// Positions for (primary, secondary) in dual-screen mode. KWin does not
/// reliably apply layouts with negative coordinates, so the pair is always
/// anchored with the leftmost/topmost output at 0,0.
fn dual_screen_positions(
    orientation: &Orientation,
    primary_size: Result<(i64, i64), String>,
) -> Result<((i64, i64), (i64, i64)), String> {
    let (width, height) = primary_size?;
    let (rot_w, rot_h) = match orientation {
        Orientation::Left | Orientation::Right => (height, width),
        Orientation::Normal | Orientation::Inverted => (width, height),
    };
    Ok(match orientation {
        Orientation::Left => ((rot_w, 0), (0, 0)),
        Orientation::Right => ((0, 0), (rot_w, 0)),
        Orientation::Inverted => ((0, rot_h), (0, 0)),
        Orientation::Normal => ((0, 0), (0, rot_h)),
    })
}

pub(super) fn set_kde_orientation(orientation: &Orientation) -> Result<(), String> {
    let token = kde_rotation_token(orientation);
    let primary_token = kde_primary_rotation_token(orientation, primary_panel_mounted_inverted());
    let enabled_count = kde_enabled_output_count().unwrap_or(1);

    if enabled_count <= 1 {
        let args = vec![
            format!("output.{PRIMARY_INTERNAL_CONNECTOR}.enable"),
            format!("output.{PRIMARY_INTERNAL_CONNECTOR}.position.0,0"),
            format!("output.{PRIMARY_INTERNAL_CONNECTOR}.rotation.{primary_token}"),
        ];
        return run_command("kscreen-doctor", &args);
    }

    let ((primary_x, primary_y), (secondary_x, secondary_y)) = dual_screen_positions(
        orientation,
        kde_output_logical_size(PRIMARY_INTERNAL_CONNECTOR),
    )?;

    let args = vec![
        format!("output.{PRIMARY_INTERNAL_CONNECTOR}.enable"),
        format!("output.{SECONDARY_INTERNAL_CONNECTOR}.enable"),
        format!("output.{PRIMARY_INTERNAL_CONNECTOR}.rotation.{primary_token}"),
        format!("output.{SECONDARY_INTERNAL_CONNECTOR}.rotation.{token}"),
        format!("output.{PRIMARY_INTERNAL_CONNECTOR}.position.{primary_x},{primary_y}"),
        format!("output.{SECONDARY_INTERNAL_CONNECTOR}.position.{secondary_x},{secondary_y}"),
    ];

    run_command("kscreen-doctor", &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_primary_geometry_is_an_error_for_secondary_positioning() {
        let result = dual_screen_positions(&Orientation::Normal, Err("missing geometry".into()));

        assert_eq!(
            result.expect_err("missing geometry should fail"),
            "missing geometry"
        );
    }

    #[test]
    fn dual_screen_positions_use_rotated_geometry_without_negative_coordinates() {
        assert_eq!(
            dual_screen_positions(&Orientation::Left, Ok((1200, 800))).expect("positions"),
            ((800, 0), (0, 0))
        );
        assert_eq!(
            dual_screen_positions(&Orientation::Right, Ok((1200, 800))).expect("positions"),
            ((0, 0), (800, 0))
        );
        assert_eq!(
            dual_screen_positions(&Orientation::Inverted, Ok((1200, 800))).expect("positions"),
            ((0, 800), (0, 0))
        );
        assert_eq!(
            dual_screen_positions(&Orientation::Normal, Ok((1200, 800))).expect("positions"),
            ((0, 0), (0, 800))
        );
    }

    #[test]
    fn primary_rotation_token_matches_secondary_when_panel_is_not_inverted() {
        for orientation in [
            Orientation::Normal,
            Orientation::Inverted,
            Orientation::Left,
            Orientation::Right,
        ] {
            assert_eq!(
                kde_primary_rotation_token(&orientation, false),
                kde_rotation_token(&orientation)
            );
        }
    }

    #[test]
    fn primary_rotation_token_is_offset_180_when_panel_is_inverted() {
        assert_eq!(
            kde_primary_rotation_token(&Orientation::Normal, true),
            "inverted"
        );
        assert_eq!(
            kde_primary_rotation_token(&Orientation::Inverted, true),
            "none"
        );
        assert_eq!(kde_primary_rotation_token(&Orientation::Left, true), "left");
        assert_eq!(
            kde_primary_rotation_token(&Orientation::Right, true),
            "right"
        );
    }
}
