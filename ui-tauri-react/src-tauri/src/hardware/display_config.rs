use crate::models::{DisplayInfo, DisplayLayout, Orientation};
use std::process::Command;

/// Get the current display layout by parsing gdctl output.
pub fn get_display_layout() -> Result<DisplayLayout, String> {
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
    // This mirrors what `duo.sh` does and matches `gdctl set --help`.
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

/// Set screen orientation using the duo command.
pub fn set_orientation(orientation: &Orientation) -> Result<(), String> {
    let arg = orientation.as_duo_arg();
    let output = Command::new("/usr/local/bin/duo")
        .arg(arg)
        .output()
        .map_err(|e| format!("Failed to run duo: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "duo {} failed: {}",
            arg,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}
