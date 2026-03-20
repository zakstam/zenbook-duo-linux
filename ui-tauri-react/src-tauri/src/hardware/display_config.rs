use crate::models::{DisplayInfo, DisplayLayout, Orientation};
use std::env;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Get the current display layout by parsing gdctl output.
pub fn get_display_layout() -> Result<DisplayLayout, String> {
    match detect_backend() {
        DisplayBackend::Gnome => get_gnome_display_layout(),
        DisplayBackend::Kde => get_kde_display_layout(),
        DisplayBackend::Hyprland => get_hyprland_display_layout(),
        DisplayBackend::Niri => get_niri_display_layout(),
        DisplayBackend::Unknown => Err("Unsupported session backend for display layout".into()),
    }
}

fn get_gnome_display_layout() -> Result<DisplayLayout, String> {
    let output = Command::new("gdctl")
        .arg("show")
        .output()
        .map_err(|e| format!("Failed to run gdctl: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "gdctl show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gdctl_output(&stdout)
}

fn parse_gdctl_output(output: &str) -> Result<DisplayLayout, String> {
    // gdctl output is a tree with box-drawing characters.
    // We parse two parts:
    //   - "Monitors" section for connectors and current/preferred mode
    //   - "Logical monitors" section for position/scale/transform/primary

    #[derive(Debug, Clone, Default)]
    struct LogicalProps {
        x: i32,
        y: i32,
        scale: f64,
        transform: u32,
        primary: bool,
    }

    fn extract_connector_from_monitor_header(line: &str) -> Option<String> {
        // Examples:
        // "├──Monitor eDP-1 (Built-in display)"
        // "└──Monitor HDMI-1 (… )"
        // "Monitor eDP-1 (… )"
        let idx = line.find("Monitor ")?;
        let rest = &line[idx + "Monitor ".len()..];
        let connector = rest.split_whitespace().next().unwrap_or("").trim();
        if connector.is_empty() {
            None
        } else {
            Some(connector.to_string())
        }
    }

    fn extract_mode_from_line(line: &str) -> Option<(u32, u32, f64)> {
        // Finds the first token like "2880x1800@120.000" (optionally suffixed with "Hz").
        let token = line
            .split_whitespace()
            .find(|t| t.contains('x') && t.contains('@'))?;

        // Tokens in gdctl output often have tree prefixes (e.g. "└──2880x1800@120.000").
        let token = token.trim_start_matches(|c: char| !c.is_ascii_digit());

        let (res, rate) = token.split_once('@')?;
        let (w, h) = res.split_once('x')?;

        let width: u32 = w.trim().parse().ok()?;
        let height: u32 = h.trim().parse().ok()?;

        let rate = rate
            .trim_end_matches("Hz")
            .trim_end_matches(|c: char| !(c.is_ascii_digit() || c == '.'))
            .trim();
        let refresh_rate: f64 = rate.parse().ok()?;
        Some((width, height, refresh_rate))
    }

    fn extract_i32_pair(line: &str) -> Option<(i32, i32)> {
        // Extracts first two integers (supports negative).
        let nums: Vec<i32> = line
            .split(|c: char| !c.is_ascii_digit() && c != '-')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() >= 2 {
            Some((nums[0], nums[1]))
        } else {
            None
        }
    }

    fn parse_transform(s: &str) -> u32 {
        // gdctl prints e.g. "normal" or "90".
        match s.trim() {
            "normal" => 0,
            "90" => 90,
            "180" => 180,
            "270" => 270,
            _ => 0,
        }
    }

    let mut monitor_order: Vec<String> = Vec::new();
    let mut monitor_mode: std::collections::HashMap<String, (u32, u32, f64)> =
        std::collections::HashMap::new();

    let mut in_monitors = false;
    let mut in_logical = false;

    let mut current_monitor: Option<String> = None;
    let mut current_logical = LogicalProps {
        x: 0,
        y: 0,
        scale: 1.0,
        transform: 0,
        primary: false,
    };
    let mut in_logical_block = false;

    let mut connector_to_logical: std::collections::HashMap<String, LogicalProps> =
        std::collections::HashMap::new();

    for raw in output.lines() {
        let line = raw.trim();

        if line.starts_with("Monitors:") {
            in_monitors = true;
            in_logical = false;
            current_monitor = None;
            continue;
        }
        if line.starts_with("Logical monitors:") {
            in_logical = true;
            in_monitors = false;
            current_monitor = None;
            in_logical_block = false;
            continue;
        }

        if in_monitors {
            if let Some(connector) = extract_connector_from_monitor_header(line) {
                current_monitor = Some(connector.clone());
                if !monitor_order.contains(&connector) {
                    monitor_order.push(connector);
                }
                continue;
            }

            if let Some(ref connector) = current_monitor {
                if !monitor_mode.contains_key(connector) {
                    if let Some((w, h, rr)) = extract_mode_from_line(line) {
                        monitor_mode.insert(connector.clone(), (w, h, rr));
                    }
                }
            }
        }

        if in_logical {
            if line.contains("Logical monitor #") {
                // Start a new logical monitor block.
                current_logical = LogicalProps {
                    x: 0,
                    y: 0,
                    scale: 1.0,
                    transform: 0,
                    primary: false,
                };
                in_logical_block = true;
                continue;
            }
            if !in_logical_block {
                continue;
            }

            if line.contains("Position:") {
                if let Some((x, y)) = extract_i32_pair(line) {
                    current_logical.x = x;
                    current_logical.y = y;
                }
                continue;
            }
            if line.contains("Scale:") {
                if let Some(scale) = line.split_whitespace().last().and_then(|s| s.parse().ok()) {
                    current_logical.scale = scale;
                }
                continue;
            }
            if line.contains("Transform:") {
                if let Some(t) = line.split_whitespace().last() {
                    current_logical.transform = parse_transform(t);
                }
                continue;
            }
            if line.contains("Primary:") {
                current_logical.primary = line.contains("yes");
                continue;
            }

            // Monitor list item under this logical monitor.
            // Example: "└──eDP-1 (Built-in display)"
            if line.contains('(') {
                let token = line.split_whitespace().next().unwrap_or("");
                let connector = token
                    .trim()
                    .trim_start_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '-'));
                // Avoid picking up tree labels like "Monitors:".
                if !connector.is_empty() && connector.contains('-') && !connector.ends_with(':') {
                    connector_to_logical.insert(connector.to_string(), current_logical.clone());
                }
            }
        }
    }

    let mut displays: Vec<DisplayInfo> = Vec::new();
    let mut missing_logical: Vec<usize> = Vec::new();

    for connector in monitor_order {
        let had_logical = connector_to_logical.contains_key(&connector);
        let (width, height, refresh_rate) = monitor_mode
            .get(&connector)
            .copied()
            .unwrap_or((0, 0, 60.0));
        let logical = connector_to_logical
            .get(&connector)
            .cloned()
            .unwrap_or_default();

        let idx = displays.len();
        displays.push(DisplayInfo {
            connector,
            width,
            height,
            refresh_rate,
            scale: if logical.scale == 0.0 {
                1.0
            } else {
                logical.scale
            },
            x: logical.x,
            y: logical.y,
            transform: logical.transform,
            primary: logical.primary,
        });
        if !had_logical {
            missing_logical.push(idx);
        }
    }

    // If gdctl doesn't report a logical-monitor position for a physical monitor
    // (e.g. an internal panel that's currently disabled), place it below the primary
    // as a sensible default for UI editing.
    if !displays.is_empty() && !missing_logical.is_empty() {
        let primary_idx = displays.iter().position(|d| d.primary).unwrap_or(0);
        let anchor = displays[primary_idx].clone();
        let mut next_y = anchor.y + anchor.height as i32;

        for &i in &missing_logical {
            if i == primary_idx {
                continue;
            }
            displays[i].x = anchor.x;
            displays[i].y = next_y;
            displays[i].scale = anchor.scale;
            displays[i].transform = anchor.transform;
            next_y += displays[i].height as i32;
        }
    }

    Ok(DisplayLayout { displays })
}

/// Apply a display layout using gdctl commands.
pub fn apply_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    match detect_backend() {
        DisplayBackend::Gnome => apply_gnome_display_layout(layout),
        DisplayBackend::Kde => apply_kde_display_layout(layout),
        DisplayBackend::Hyprland => apply_hyprland_display_layout(layout),
        DisplayBackend::Niri => apply_niri_display_layout(layout),
        DisplayBackend::Unknown => Err("Unsupported session backend for display layout".into()),
    }
}

fn apply_gnome_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    // gdctl rejects negative logical monitor positions.
    // Normalize the layout so the smallest x/y becomes 0.
    let mut min_x: i32 = 0;
    let mut min_y: i32 = 0;
    for d in &layout.displays {
        min_x = min_x.min(d.x);
        min_y = min_y.min(d.y);
    }
    let shift_x = if min_x < 0 { -min_x } else { 0 };
    let shift_y = if min_y < 0 { -min_y } else { 0 };

    fn transform_arg(t: u32) -> Option<&'static str> {
        match t {
            0 => Some("normal"),
            90 => Some("90"),
            180 => Some("180"),
            270 => Some("270"),
            _ => None,
        }
    }

    // Build gdctl set command in logical-monitor mode.
    // Keep the transform semantics aligned with `gdctl set --help`.
    let mut args: Vec<String> = vec!["set".into(), "--layout-mode".into(), "logical".into()];
    let mut primary_used = false;

    for display in &layout.displays {
        args.push("--logical-monitor".into());

        if display.primary && !primary_used {
            args.push("--primary".into());
            primary_used = true;
        }
        args.push("--scale".into());
        args.push(format!("{:.6}", display.scale.max(0.1)));
        args.push("--monitor".into());
        args.push(display.connector.clone());
        args.push("--x".into());
        args.push((display.x + shift_x).to_string());
        args.push("--y".into());
        args.push((display.y + shift_y).to_string());
        if let Some(t) = transform_arg(display.transform) {
            if t != "normal" {
                args.push("--transform".into());
                args.push(t.into());
            }
        }
    }

    // If caller didn't set a primary at all, make the first display primary.
    if !primary_used {
        if let Some(pos) = args.iter().position(|a| a == "--logical-monitor") {
            args.insert(pos + 1, "--primary".into());
        }
    }

    let output = Command::new("gdctl")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run gdctl: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "gdctl set failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn get_kde_display_layout() -> Result<DisplayLayout, String> {
    let output = Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .map_err(|e| format!("Failed to run kscreen-doctor: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid kscreen JSON: {e}"))?;
    let outputs = value
        .get("outputs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing KDE outputs array".to_string())?;

    let mut displays = Vec::new();
    for output in outputs {
        if !output.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
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

        displays.push(DisplayInfo {
            connector: connector.to_string(),
            width: size.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            height: size.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            refresh_rate: output
                .get("currentMode")
                .and_then(|mode| mode.get("refreshRate"))
                .and_then(|v| v.as_f64())
                .map(|rate| rate / 1000.0)
                .unwrap_or(60.0),
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
        });
    }

    Ok(DisplayLayout { displays })
}

fn apply_kde_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    let mut args: Vec<String> = Vec::new();
    for display in &layout.displays {
        args.push(format!("output.{}.enable", display.connector));
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

fn get_niri_display_layout() -> Result<DisplayLayout, String> {
    let output = Command::new("niri")
        .args(["msg", "--json", "outputs"])
        .output()
        .map_err(|e| format!("Failed to run niri msg: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid niri JSON: {e}"))?;
    let outputs: Vec<serde_json::Value> = if let Some(arr) = value.as_array() {
        arr.clone()
    } else if let Some(obj) = value.as_object() {
        obj.values().cloned().collect()
    } else {
        return Err("Unexpected niri outputs shape".into());
    };

    let mut displays = Vec::new();
    for output in outputs {
        if output.get("current_mode").is_some_and(|value| value.is_null()) {
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
            .ok_or_else(|| format!("Missing niri current mode for {connector}"))?;

        displays.push(DisplayInfo {
            connector: connector.to_string(),
            width: current_mode.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            height: current_mode.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            refresh_rate: current_mode
                .get("refresh_rate")
                .and_then(|v| v.as_f64())
                .map(|rate| rate / 1000.0)
                .unwrap_or(60.0),
            scale: logical.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0),
            x: logical.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            y: logical.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            transform: parse_niri_transform(&output),
            primary: connector == "eDP-1",
        });
    }

    Ok(DisplayLayout { displays })
}

fn resolve_niri_current_mode(output: &serde_json::Value) -> Option<serde_json::Value> {
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

fn parse_niri_transform(output: &serde_json::Value) -> u32 {
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

fn apply_niri_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    for display in &layout.displays {
        run_command("niri", &["msg", "output", &display.connector, "on"])?;
        run_command(
            "niri",
            &[
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
            ],
        )?;
        run_command(
            "niri",
            &[
                "msg",
                "output",
                &display.connector,
                "scale",
                &format!("{:.6}", display.scale.max(0.1)),
            ],
        )?;
        run_command(
            "niri",
            &[
                "msg",
                "output",
                &display.connector,
                "position",
                "set",
                &display.x.to_string(),
                &display.y.to_string(),
            ],
        )?;
    }

    Ok(())
}

fn get_hyprland_display_layout() -> Result<DisplayLayout, String> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .map_err(|e| format!("Failed to run hyprctl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_hyprland_monitors(&stdout)
}

fn parse_hyprland_monitors(raw: &str) -> Result<DisplayLayout, String> {
    let value = parse_hyprland_json(raw)?;
    let outputs = hyprland_outputs_from_value(&value)?;
    let mut displays = Vec::new();

    for output in outputs {
        if output.get("disabled").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }

        let connector = output
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing Hyprland output name".to_string())?;
        let width = output.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let height = output.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let refresh_rate = output
            .get("refreshRate")
            .or_else(|| output.get("refresh_rate"))
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0);
        let scale = output.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let x = output.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let y = output.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        displays.push(DisplayInfo {
            connector: connector.to_string(),
            width,
            height,
            refresh_rate,
            scale,
            x,
            y,
            transform: hyprland_transform_degrees(
                output.get("transform").and_then(|v| v.as_i64()).unwrap_or(0),
            ),
            primary: false,
        });
    }

    if let Some(primary_idx) = displays
        .iter()
        .position(|display| display.connector == "eDP-1")
        .or_else(|| (!displays.is_empty()).then_some(0))
    {
        displays[primary_idx].primary = true;
    }

    Ok(DisplayLayout { displays })
}

fn parse_hyprland_json(raw: &str) -> Result<serde_json::Value, String> {
    let json_start = raw
        .find(|c| c == '[' || c == '{')
        .ok_or_else(|| "Invalid hyprctl JSON: missing JSON payload".to_string())?;
    serde_json::from_str(&raw[json_start..]).map_err(|e| format!("Invalid hyprctl JSON: {e}"))
}

fn hyprland_outputs_from_value(value: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    if let Some(arr) = value.as_array() {
        Ok(arr.clone())
    } else if let Some(obj) = value.as_object() {
        Ok(obj.values().cloned().collect())
    } else {
        Err("Unexpected Hyprland monitors shape".into())
    }
}

fn hyprland_monitor_counts_from_json(raw: &str) -> Result<(usize, usize), String> {
    let value = parse_hyprland_json(raw)?;
    let outputs = hyprland_outputs_from_value(&value)?;
    let disabled = outputs
        .iter()
        .filter(|output| output.get("disabled").and_then(|v| v.as_bool()) == Some(true))
        .count();
    Ok((outputs.len().saturating_sub(disabled), disabled))
}

fn hyprland_enabled_monitor_count_from_json(raw: &str) -> Result<usize, String> {
    Ok(hyprland_monitor_counts_from_json(raw)?.0)
}

fn hyprland_monitor_keyword_args(display: &DisplayInfo) -> Vec<String> {
    vec!["keyword".into(), "monitor".into(), hyprland_monitor_rule(display)]
}

fn hyprland_monitor_rule(display: &DisplayInfo) -> String {
    format!(
        "{},{},{},{},transform,{}",
        display.connector,
        hyprland_mode_string(display),
        format!("{}x{}", display.x, display.y),
        hyprland_scale_string(display.scale),
        hyprland_transform_value_from_degrees(display.transform)
    )
}

fn hyprland_mode_string(display: &DisplayInfo) -> String {
    if display.width == 0 || display.height == 0 {
        return "preferred".to_string();
    }

    let mut refresh = format!("{:.3}", display.refresh_rate.max(1.0));
    while refresh.contains('.') && refresh.ends_with('0') {
        refresh.pop();
    }
    if refresh.ends_with('.') {
        refresh.pop();
    }

    format!("{}x{}@{}", display.width, display.height, refresh)
}

fn hyprland_scale_string(scale: f64) -> String {
    let mut value = format!("{:.6}", scale.max(0.1));
    while value.contains('.') && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

fn hyprland_transform_degrees(raw: i64) -> u32 {
    match raw {
        1 | 5 => 90,
        2 | 6 => 180,
        3 | 7 => 270,
        _ => 0,
    }
}

fn hyprland_transform_value(orientation: &Orientation) -> i64 {
    match orientation {
        Orientation::Normal => 0,
        Orientation::Left => 1,
        Orientation::Inverted => 2,
        Orientation::Right => 3,
    }
}

fn hyprland_transform_value_from_degrees(transform: u32) -> i64 {
    match transform {
        90 => 1,
        180 => 2,
        270 => 3,
        _ => 0,
    }
}

fn hyprland_secondary_position(primary_logical: (i64, i64), orientation: &Orientation) -> (i64, i64) {
    let (width, height) = primary_logical;
    match orientation {
        Orientation::Normal => (0, height),
        Orientation::Left => (-width, 0),
        Orientation::Right => (width, 0),
        Orientation::Inverted => (0, -height),
    }
}

fn hyprland_primary_logical_size(display: &DisplayInfo) -> (i64, i64) {
    let width = (display.width as f64 / display.scale.max(0.1)).round() as i64;
    let height = (display.height as f64 / display.scale.max(0.1)).round() as i64;
    match display.transform {
        90 | 270 => (height, width),
        _ => (width, height),
    }
}

fn hyprland_monitor_state(name: &str) -> Result<DisplayInfo, String> {
    let layout = get_hyprland_display_layout()?;
    layout
        .displays
        .into_iter()
        .find(|display| display.connector == name)
        .ok_or_else(|| format!("Hyprland output {name} not found"))
}

fn hyprland_enabled_output_count() -> Result<usize, String> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .map_err(|e| format!("Failed to run hyprctl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    hyprland_enabled_monitor_count_from_json(&stdout)
}

fn apply_hyprland_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    let current_layout = get_hyprland_display_layout()?;

    for display in hyprland_apply_order(layout) {
        let args = hyprland_monitor_keyword_args(display);
        run_command("hyprctl", &args)?;
    }

    for connector in hyprland_connectors_to_disable(&current_layout, layout) {
        run_command("hyprctl", &["keyword", "monitor", &format!("{connector},disable")])?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayBackend {
    Gnome,
    Kde,
    Hyprland,
    Niri,
    Unknown,
}

fn detect_backend() -> DisplayBackend {
    let current = env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| env::var("XDG_SESSION_DESKTOP"))
        .or_else(|_| env::var("DESKTOP_SESSION"))
        .unwrap_or_default();

    detect_backend_from_session(&current)
}

fn detect_backend_from_session(value: &str) -> DisplayBackend {
    let current = value.to_lowercase();

    if current.contains("hyprland") || current == "hypr" {
        DisplayBackend::Hyprland
    } else if current.contains("gnome") {
        DisplayBackend::Gnome
    } else if current.contains("plasma") || current.contains("kde") {
        DisplayBackend::Kde
    } else if current.contains("niri") {
        DisplayBackend::Niri
    } else {
        DisplayBackend::Unknown
    }
}

fn run_command<S: AsRef<str>>(program: &str, args: &[S]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .map_err(|e| format!("Failed to run {program}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn gnome_scale() -> Result<f64, String> {
    let layout = get_display_layout()?;
    layout
        .displays
        .iter()
        .find(|display| display.primary)
        .or_else(|| layout.displays.first())
        .map(|display| display.scale.max(0.1))
        .ok_or_else(|| "No GNOME displays available".to_string())
}

fn gnome_logical_monitor_count() -> Result<usize, String> {
    let output = Command::new("gdctl")
        .arg("show")
        .output()
        .map_err(|e| format!("Failed to run gdctl: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "gdctl show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("Logical monitor #"))
        .count())
}

fn set_gnome_orientation(orientation: &Orientation) -> Result<(), String> {
    let scale = format!("{:.6}", gnome_scale()?);
    let transform = match orientation {
        Orientation::Normal => None,
        Orientation::Left => Some("90"),
        Orientation::Right => Some("270"),
        Orientation::Inverted => Some("180"),
    };

    let logical_count = gnome_logical_monitor_count().unwrap_or(1);
    let mut args = vec![
        "set".to_string(),
        "--logical-monitor".to_string(),
        "--primary".to_string(),
        "--scale".to_string(),
        scale.clone(),
        "--monitor".to_string(),
        "eDP-1".to_string(),
    ];

    if let Some(transform) = transform {
        args.push("--transform".to_string());
        args.push(transform.to_string());
    }

    if logical_count > 1 {
        args.push("--logical-monitor".to_string());
        args.push("--scale".to_string());
        args.push(scale);
        args.push("--monitor".to_string());
        args.push("eDP-2".to_string());

        match orientation {
            Orientation::Left => args.extend(["--left-of", "eDP-1"].map(str::to_string)),
            Orientation::Right => args.extend(["--right-of", "eDP-1"].map(str::to_string)),
            Orientation::Inverted => args.extend(["--above", "eDP-1"].map(str::to_string)),
            Orientation::Normal => args.extend(["--below", "eDP-1"].map(str::to_string)),
        }

        if let Some(transform) = transform {
            args.push("--transform".to_string());
            args.push(transform.to_string());
        }
    }

    run_command("gdctl", &args)
}

fn kde_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    let output = Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .map_err(|e| format!("Failed to run kscreen-doctor: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid kscreen JSON: {e}"))?;
    let outputs = value
        .get("outputs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing KDE outputs array".to_string())?;

    for output in outputs {
        if output.get("name").and_then(|v| v.as_str()) == Some(name) {
            let size = output
                .get("size")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "Missing KDE output size".to_string())?;
            let scale = output.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let width = size.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
            let height = size.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
            return Ok((
                (width as f64 / scale).round() as i64,
                (height as f64 / scale).round() as i64,
            ));
        }
    }

    Err(format!("KDE output {name} not found"))
}

fn kde_enabled_output_count() -> Result<usize, String> {
    let output = Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .map_err(|e| format!("Failed to run kscreen-doctor: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid kscreen JSON: {e}"))?;
    let outputs = value
        .get("outputs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing KDE outputs array".to_string())?;
    Ok(outputs
        .iter()
        .filter(|output| output.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false))
        .count())
}

fn kde_rotation_token(orientation: &Orientation) -> &'static str {
    match orientation {
        Orientation::Left => "right",
        Orientation::Right => "left",
        Orientation::Inverted => "inverted",
        Orientation::Normal => "none",
    }
}

fn set_kde_orientation(orientation: &Orientation) -> Result<(), String> {
    let token = kde_rotation_token(orientation);
    let enabled_count = kde_enabled_output_count().unwrap_or(1);

    if enabled_count <= 1 {
        return run_command(
            "kscreen-doctor",
            &[
                "output.eDP-1.enable",
                "output.eDP-1.position.0,0",
                &format!("output.eDP-1.rotation.{token}"),
            ],
        );
    }

    let (width, height) = kde_output_logical_size("eDP-1").unwrap_or((0, 0));
    let (rot_w, rot_h) = match orientation {
        Orientation::Left | Orientation::Right => (height, width),
        Orientation::Normal | Orientation::Inverted => (width, height),
    };
    let (pos_x, pos_y) = match orientation {
        Orientation::Left => (-rot_w, 0),
        Orientation::Right => (rot_w, 0),
        Orientation::Inverted => (0, -rot_h),
        Orientation::Normal => (0, rot_h),
    };

    run_command(
        "kscreen-doctor",
        &[
            "output.eDP-1.enable",
            "output.eDP-2.enable",
            &format!("output.eDP-1.rotation.{token}"),
            &format!("output.eDP-2.rotation.{token}"),
            "output.eDP-1.position.0,0",
            &format!("output.eDP-2.position.{pos_x},{pos_y}"),
        ],
    )
}

fn niri_output_logical_size(name: &str) -> Result<(i64, i64), String> {
    let output = Command::new("niri")
        .args(["msg", "--json", "outputs"])
        .output()
        .map_err(|e| format!("Failed to run niri msg: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid niri JSON: {e}"))?;
    let outputs = if let Some(arr) = value.as_array() {
        arr.clone()
    } else if let Some(obj) = value.as_object() {
        obj.values().cloned().collect()
    } else {
        return Err("Unexpected niri outputs shape".into());
    };

    for output in outputs {
        if output.get("name").and_then(|v| v.as_str()) == Some(name) {
            let logical = output
                .get("logical")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "Missing niri logical size".to_string())?;
            let width = logical.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
            let height = logical.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
            return Ok((width, height));
        }
    }

    Err(format!("Niri output {name} not found"))
}

fn niri_enabled_output_count() -> Result<usize, String> {
    let output = Command::new("niri")
        .args(["msg", "--json", "outputs"])
        .output()
        .map_err(|e| format!("Failed to run niri msg: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid niri JSON: {e}"))?;
    let outputs: Vec<serde_json::Value> = if let Some(arr) = value.as_array() {
        arr.clone()
    } else if let Some(obj) = value.as_object() {
        obj.values().cloned().collect()
    } else {
        return Err("Unexpected niri outputs shape".into());
    };

    Ok(outputs
        .iter()
        .filter(|output| !output.get("current_mode").is_some_and(|value| value.is_null()))
        .count())
}

fn niri_transform_token(orientation: &Orientation) -> &'static str {
    match orientation {
        Orientation::Left => "90",
        Orientation::Right => "270",
        Orientation::Inverted => "180",
        Orientation::Normal => "normal",
    }
}

fn set_niri_orientation(orientation: &Orientation) -> Result<(), String> {
    let token = niri_transform_token(orientation);
    run_command("niri", &["msg", "output", "eDP-1", "transform", token])?;
    run_command("niri", &["msg", "output", "eDP-1", "position", "set", "0", "0"])?;

    if niri_enabled_output_count().unwrap_or(1) <= 1 {
        return Ok(());
    }

    run_command("niri", &["msg", "output", "eDP-2", "transform", token])?;
    thread::sleep(Duration::from_millis(300));

    let (width, height) = niri_output_logical_size("eDP-1").unwrap_or((0, 0));
    let (pos_x, pos_y) = match orientation {
        Orientation::Left => (-width, 0),
        Orientation::Right => (width, 0),
        Orientation::Inverted => (0, -height),
        Orientation::Normal => (0, height),
    };

    run_command(
        "niri",
        &[
            "msg",
            "output",
            "eDP-2",
            "position",
            "set",
            &pos_x.to_string(),
            &pos_y.to_string(),
        ],
    )
}

fn set_hyprland_orientation(orientation: &Orientation) -> Result<(), String> {
    let mut primary = hyprland_monitor_state("eDP-1")?;
    primary.transform = hyprland_transform_degrees(hyprland_transform_value(orientation));
    primary.x = 0;
    primary.y = 0;
    let primary_args = hyprland_monitor_keyword_args(&primary);
    run_command("hyprctl", &primary_args)?;

    thread::sleep(Duration::from_millis(300));

    let refreshed_primary = hyprland_monitor_state("eDP-1")?;
    let current_layout = get_hyprland_display_layout()?;
    if !hyprland_should_update_secondary(hyprland_enabled_output_count()?, &current_layout) {
        return Ok(());
    }

    let primary_logical = hyprland_primary_logical_size(&refreshed_primary);
    let (pos_x, pos_y) = hyprland_secondary_position(primary_logical, orientation);

    let Ok(mut secondary) = hyprland_monitor_state("eDP-2") else {
        return Ok(());
    };
    secondary.transform = primary.transform;
    secondary.x = pos_x as i32;
    secondary.y = pos_y as i32;
    let secondary_args = hyprland_monitor_keyword_args(&secondary);
    run_command("hyprctl", &secondary_args)
}

fn hyprland_connectors_to_disable(current: &DisplayLayout, target: &DisplayLayout) -> Vec<String> {
    let target_connectors: Vec<&str> = target
        .displays
        .iter()
        .map(|display| display.connector.as_str())
        .collect();
    current
        .displays
        .iter()
        .filter(|display| !target_connectors.contains(&display.connector.as_str()))
        .map(|display| display.connector.clone())
        .collect()
}

fn hyprland_layout_has_connector(layout: &DisplayLayout, connector: &str) -> bool {
    layout
        .displays
        .iter()
        .any(|display| display.connector == connector)
}

fn hyprland_apply_order(layout: &DisplayLayout) -> Vec<&DisplayInfo> {
    let mut displays: Vec<&DisplayInfo> = layout.displays.iter().collect();
    displays.sort_by_key(|display| !display.primary);
    displays
}

fn hyprland_should_update_secondary(enabled_count: usize, layout: &DisplayLayout) -> bool {
    enabled_count > 1 && hyprland_layout_has_connector(layout, "eDP-2")
}

/// Set screen orientation using compositor-native commands.
pub fn set_orientation(orientation: &Orientation) -> Result<(), String> {
    match detect_backend() {
        DisplayBackend::Gnome => set_gnome_orientation(orientation),
        DisplayBackend::Kde => set_kde_orientation(orientation),
        DisplayBackend::Hyprland => set_hyprland_orientation(orientation),
        DisplayBackend::Niri => set_niri_orientation(orientation),
        DisplayBackend::Unknown => {
            Err("Unsupported session backend for orientation control".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HYPRLAND_MONITORS_JSON: &str = r#"
    [
      {
        "name": "eDP-1",
        "width": 2880,
        "height": 1800,
        "refreshRate": 120.0,
        "x": 0,
        "y": 0,
        "scale": 1.5,
        "transform": 0,
        "focused": true,
        "disabled": false
      },
      {
        "name": "DP-1",
        "width": 1920,
        "height": 1080,
        "refreshRate": 60.0,
        "x": 1920,
        "y": 0,
        "scale": 1.0,
        "transform": 1,
        "focused": false,
        "disabled": false
      },
      {
        "name": "HDMI-A-1",
        "width": 2560,
        "height": 1440,
        "refreshRate": 59.95,
        "x": 0,
        "y": 0,
        "scale": 1.0,
        "transform": 0,
        "focused": false,
        "disabled": true
      }
    ]
    "#;

    #[test]
    fn parses_niri_transform_from_logical_object() {
        let value = json!({
            "logical": { "transform": "Normal" }
        });
        assert_eq!(parse_niri_transform(&value), 0);

        let value = json!({
            "logical": { "transform": "90" }
        });
        assert_eq!(parse_niri_transform(&value), 90);
    }

    #[test]
    fn falls_back_to_top_level_niri_transform() {
        let value = json!({
            "transform": "270"
        });
        assert_eq!(parse_niri_transform(&value), 270);
    }

    #[test]
    fn resolves_niri_current_mode_from_index() {
        let value = json!({
            "current_mode": 1,
            "modes": [
                { "width": 1920, "height": 1200, "refresh_rate": 60000 },
                { "width": 2880, "height": 1800, "refresh_rate": 120000 }
            ]
        });

        let mode = resolve_niri_current_mode(&value).expect("mode should resolve");
        assert_eq!(mode.get("width").and_then(|v| v.as_u64()), Some(2880));
        assert_eq!(mode.get("height").and_then(|v| v.as_u64()), Some(1800));
    }

    #[test]
    fn detects_hyprland_backend_from_session_names() {
        assert_eq!(detect_backend_from_session("Hyprland"), DisplayBackend::Hyprland);
        assert_eq!(detect_backend_from_session("hypr"), DisplayBackend::Hyprland);
        assert_eq!(detect_backend_from_session("GNOME"), DisplayBackend::Gnome);
    }

    #[test]
    fn parses_hyprland_monitor_json_into_layout() {
        let layout = parse_hyprland_monitors(HYPRLAND_MONITORS_JSON).expect("layout should parse");

        assert_eq!(layout.displays.len(), 2);

        let primary = &layout.displays[0];
        assert_eq!(primary.connector, "eDP-1");
        assert_eq!(primary.width, 2880);
        assert_eq!(primary.height, 1800);
        assert_eq!(primary.refresh_rate, 120.0);
        assert_eq!(primary.scale, 1.5);
        assert_eq!(primary.x, 0);
        assert_eq!(primary.y, 0);
        assert_eq!(primary.transform, 0);
        assert!(primary.primary);

        let external = &layout.displays[1];
        assert_eq!(external.connector, "DP-1");
        assert_eq!(external.transform, 90);
        assert_eq!(external.x, 1920);
        assert_eq!(external.y, 0);
        assert!(!external.primary);
    }

    #[test]
    fn parses_hyprland_monitor_json_with_prefixed_noise() {
        let raw = format!("warning: noisy prefix\n{HYPRLAND_MONITORS_JSON}");
        let layout = parse_hyprland_monitors(&raw).expect("layout should parse");
        assert_eq!(layout.displays.len(), 2);
    }

    #[test]
    fn hyprland_primary_selection_prefers_edp1_over_focus() {
        let layout = parse_hyprland_monitors(
            r#"
            [
              {
                "name": "eDP-1",
                "width": 2880,
                "height": 1800,
                "refreshRate": 120.0,
                "x": 0,
                "y": 0,
                "scale": 1.5,
                "transform": 0,
                "focused": false,
                "disabled": false
              },
              {
                "name": "DP-1",
                "width": 1920,
                "height": 1080,
                "refreshRate": 60.0,
                "x": 1920,
                "y": 0,
                "scale": 1.0,
                "transform": 0,
                "focused": true,
                "disabled": false
              }
            ]
            "#,
        )
        .expect("layout should parse");

        assert!(layout.displays.iter().any(|display| display.connector == "eDP-1" && display.primary));
        assert!(layout.displays.iter().any(|display| display.connector == "DP-1" && !display.primary));
    }

    #[test]
    fn hyprland_primary_selection_falls_back_to_first_display_without_edp1() {
        let layout = parse_hyprland_monitors(
            r#"
            [
              {
                "name": "DP-1",
                "width": 1920,
                "height": 1080,
                "refreshRate": 60.0,
                "x": 0,
                "y": 0,
                "scale": 1.0,
                "transform": 0,
                "focused": false,
                "disabled": false
              },
              {
                "name": "HDMI-A-1",
                "width": 2560,
                "height": 1440,
                "refreshRate": 59.95,
                "x": 1920,
                "y": 0,
                "scale": 1.0,
                "transform": 0,
                "focused": true,
                "disabled": false
              }
            ]
            "#,
        )
        .expect("layout should parse");

        assert!(layout.displays[0].primary);
        assert_eq!(layout.displays[0].connector, "DP-1");
        assert!(!layout.displays[1].primary);
    }

    #[test]
    fn counts_enabled_and_disabled_hyprland_monitors() {
        let (enabled, disabled) =
            hyprland_monitor_counts_from_json(HYPRLAND_MONITORS_JSON).expect("counts should parse");

        assert_eq!(enabled, 2);
        assert_eq!(disabled, 1);
        assert_eq!(
            hyprland_enabled_monitor_count_from_json(HYPRLAND_MONITORS_JSON).unwrap(),
            2
        );
    }

    #[test]
    fn maps_hyprland_transform_helpers() {
        assert_eq!(hyprland_transform_degrees(0), 0);
        assert_eq!(hyprland_transform_degrees(1), 90);
        assert_eq!(hyprland_transform_degrees(2), 180);
        assert_eq!(hyprland_transform_degrees(3), 270);

        assert_eq!(hyprland_transform_value(&Orientation::Normal), 0);
        assert_eq!(hyprland_transform_value(&Orientation::Left), 1);
        assert_eq!(hyprland_transform_value(&Orientation::Inverted), 2);
        assert_eq!(hyprland_transform_value(&Orientation::Right), 3);
    }

    #[test]
    fn computes_hyprland_secondary_position_from_logical_size() {
        assert_eq!(
            hyprland_secondary_position((1920, 1200), &Orientation::Normal),
            (0, 1200)
        );
        assert_eq!(
            hyprland_secondary_position((1200, 1920), &Orientation::Left),
            (-1200, 0)
        );
        assert_eq!(
            hyprland_secondary_position((1200, 1920), &Orientation::Right),
            (1200, 0)
        );
        assert_eq!(
            hyprland_secondary_position((1920, 1200), &Orientation::Inverted),
            (0, -1200)
        );
    }

    #[test]
    fn hyprland_primary_logical_size_accounts_for_transform() {
        let display = DisplayInfo {
            connector: "eDP-1".into(),
            width: 2880,
            height: 1800,
            refresh_rate: 120.0,
            scale: 1.5,
            x: 0,
            y: 0,
            transform: 90,
            primary: true,
        };

        assert_eq!(hyprland_primary_logical_size(&display), (1200, 1920));
    }

    #[test]
    fn builds_hyprland_monitor_keyword_arguments() {
        let display = DisplayInfo {
            connector: "eDP-1".into(),
            width: 2880,
            height: 1800,
            refresh_rate: 120.0,
            scale: 1.5,
            x: 0,
            y: 0,
            transform: 90,
            primary: true,
        };

        let args = hyprland_monitor_keyword_args(&display);
        assert_eq!(args[0], "keyword");
        assert_eq!(args[1], "monitor");
        assert_eq!(args[2], "eDP-1,2880x1800@120,0x0,1.5,transform,1");
    }

    #[test]
    fn hyprland_connectors_to_disable_match_current_minus_target() {
        let current = DisplayLayout {
            displays: vec![
                DisplayInfo {
                    connector: "eDP-1".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.5,
                    x: 0,
                    y: 0,
                    transform: 0,
                    primary: true,
                },
                DisplayInfo {
                    connector: "DP-1".into(),
                    width: 1920,
                    height: 1080,
                    refresh_rate: 60.0,
                    scale: 1.0,
                    x: 1920,
                    y: 0,
                    transform: 0,
                    primary: false,
                },
            ],
        };
        let target = DisplayLayout {
            displays: vec![DisplayInfo {
                connector: "eDP-1".into(),
                width: 2880,
                height: 1800,
                refresh_rate: 120.0,
                scale: 1.5,
                x: 0,
                y: 0,
                transform: 0,
                primary: true,
            }],
        };

        assert_eq!(hyprland_connectors_to_disable(&current, &target), vec!["DP-1"]);
    }

    #[test]
    fn hyprland_layout_has_connector_detects_missing_internal_secondary() {
        let layout = DisplayLayout {
            displays: vec![
                DisplayInfo {
                    connector: "eDP-1".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.5,
                    x: 0,
                    y: 0,
                    transform: 0,
                    primary: true,
                },
                DisplayInfo {
                    connector: "DP-1".into(),
                    width: 1920,
                    height: 1080,
                    refresh_rate: 60.0,
                    scale: 1.0,
                    x: 1920,
                    y: 0,
                    transform: 0,
                    primary: false,
                },
            ],
        };

        assert!(hyprland_layout_has_connector(&layout, "eDP-1"));
        assert!(!hyprland_layout_has_connector(&layout, "eDP-2"));
    }

    #[test]
    fn hyprland_apply_order_puts_primary_display_first() {
        let layout = DisplayLayout {
            displays: vec![
                DisplayInfo {
                    connector: "DP-1".into(),
                    width: 1920,
                    height: 1080,
                    refresh_rate: 60.0,
                    scale: 1.0,
                    x: 1920,
                    y: 0,
                    transform: 0,
                    primary: false,
                },
                DisplayInfo {
                    connector: "eDP-1".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.5,
                    x: 0,
                    y: 0,
                    transform: 0,
                    primary: true,
                },
            ],
        };

        let ordered = hyprland_apply_order(&layout);
        assert_eq!(ordered[0].connector, "eDP-1");
        assert_eq!(ordered[1].connector, "DP-1");
    }

    #[test]
    fn hyprland_should_update_secondary_requires_edp2_and_multiple_outputs() {
        let external_only = DisplayLayout {
            displays: vec![
                DisplayInfo {
                    connector: "eDP-1".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.5,
                    x: 0,
                    y: 0,
                    transform: 0,
                    primary: true,
                },
                DisplayInfo {
                    connector: "DP-1".into(),
                    width: 1920,
                    height: 1080,
                    refresh_rate: 60.0,
                    scale: 1.0,
                    x: 1920,
                    y: 0,
                    transform: 0,
                    primary: false,
                },
            ],
        };

        assert!(!hyprland_should_update_secondary(2, &external_only));

        let dual_internal = DisplayLayout {
            displays: vec![
                DisplayInfo {
                    connector: "eDP-1".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.5,
                    x: 0,
                    y: 0,
                    transform: 0,
                    primary: true,
                },
                DisplayInfo {
                    connector: "eDP-2".into(),
                    width: 2880,
                    height: 1800,
                    refresh_rate: 120.0,
                    scale: 1.5,
                    x: 0,
                    y: 1200,
                    transform: 0,
                    primary: false,
                },
            ],
        };

        assert!(hyprland_should_update_secondary(2, &dual_internal));
        assert!(!hyprland_should_update_secondary(1, &dual_internal));
    }
}
