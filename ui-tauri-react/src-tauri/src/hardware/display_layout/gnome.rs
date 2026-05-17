use super::*;

pub(super) fn get_gnome_display_layout() -> Result<DisplayLayout, String> {
    let output = compositor::command_output("gdctl", &["show"])?;

    if !output.status.success() {
        return Err(format!(
            "gdctl show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gdctl_output(&stdout)
}

pub(super) fn parse_gdctl_output(output: &str) -> Result<DisplayLayout, String> {
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

    fn extract_mode_from_line(line: &str) -> Option<DisplayMode> {
        // Finds the first token like "2880x1800@120.000" (optionally suffixed with "Hz").
        let token = line
            .split_whitespace()
            .find(|t| t.contains('x') && t.contains('@'))?;

        // Tokens in gdctl output often have tree prefixes (e.g. "└──2880x1800@120.000").
        let token = token.trim_start_matches(|c: char| !c.is_ascii_digit());
        let backend_mode_id = token
            .trim_end_matches("Hz")
            .trim_end_matches(|c: char| !(c.is_ascii_digit() || c == '.'))
            .trim()
            .to_string();

        let (res, rate) = backend_mode_id.split_once('@')?;
        let (w, h) = res.split_once('x')?;

        let width: u32 = w.trim().parse().ok()?;
        let height: u32 = h.trim().parse().ok()?;
        let refresh_rate: f64 = rate.trim().parse().ok()?;

        Some(make_display_mode_with_backend_id(
            width,
            height,
            refresh_rate,
            Some(backend_mode_id),
        ))
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
    let mut monitor_modes: std::collections::HashMap<String, Vec<DisplayMode>> =
        std::collections::HashMap::new();
    let mut monitor_current_mode: std::collections::HashMap<String, DisplayMode> =
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
                if let Some(mode) = extract_mode_from_line(line) {
                    monitor_modes
                        .entry(connector.clone())
                        .or_default()
                        .push(mode.clone());

                    if line.contains("Current mode")
                        || line.contains("current mode")
                        || !monitor_current_mode.contains_key(connector)
                    {
                        monitor_current_mode.insert(connector.clone(), mode);
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
        let current_mode = monitor_current_mode
            .get(&connector)
            .cloned()
            .or_else(|| {
                monitor_modes
                    .get(&connector)
                    .and_then(|modes| modes.first())
                    .cloned()
            })
            .unwrap_or_else(|| make_display_mode(0, 0, 60.0));
        let available_modes = dedupe_modes(
            monitor_modes
                .remove(&connector)
                .filter(|modes| !modes.is_empty())
                .unwrap_or_else(|| vec![current_mode.clone()]),
        );
        let logical = connector_to_logical
            .get(&connector)
            .cloned()
            .unwrap_or_default();

        let idx = displays.len();
        displays.push(DisplayInfo {
            connector,
            width: current_mode.width,
            height: current_mode.height,
            refresh_rate: current_mode.refresh_rate,
            scale: if logical.scale == 0.0 {
                1.0
            } else {
                logical.scale
            },
            x: logical.x,
            y: logical.y,
            transform: logical.transform,
            primary: logical.primary,
            current_mode,
            available_modes,
            refresh_policy: RefreshPolicy::Fixed,
            supports_dynamic_refresh: false,
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

pub(super) fn gnome_mode_arg(display: &DisplayInfo, current_layout: Option<&DisplayLayout>) -> String {
    if let Some(mode) = display.current_mode.backend_mode_id.as_ref() {
        return mode.clone();
    }

    if let Some(mode) = display
        .available_modes
        .iter()
        .find(|mode| mode.mode_id == display.current_mode.mode_id)
        .and_then(|mode| mode.backend_mode_id.as_ref())
    {
        return mode.clone();
    }

    if let Some(mode) = current_layout
        .and_then(|layout| {
            layout
                .displays
                .iter()
                .find(|d| d.connector == display.connector)
        })
        .and_then(|current| {
            current.available_modes.iter().find(|mode| {
                mode.mode_id == display.current_mode.mode_id
                    || (mode.width == display.current_mode.width
                        && mode.height == display.current_mode.height
                        && (mode.refresh_rate - display.current_mode.refresh_rate).abs() < 0.001)
            })
        })
        .and_then(|mode| mode.backend_mode_id.as_ref())
    {
        return mode.clone();
    }

    display.current_mode.mode_id.clone()
}

pub(super) fn apply_gnome_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    let current_layout = get_gnome_display_layout().ok();

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
        if display.refresh_policy == RefreshPolicy::Dynamic {
            return Err(format!(
                "Dynamic refresh is not supported for {} on GNOME",
                display.connector
            ));
        }

        args.push("--logical-monitor".into());

        if display.primary && !primary_used {
            args.push("--primary".into());
            primary_used = true;
        }
        args.push("--scale".into());
        args.push(format!("{:.6}", display.scale.max(0.1)));
        args.push("--monitor".into());
        args.push(display.connector.clone());
        args.push("--mode".into());
        args.push(gnome_mode_arg(display, current_layout.as_ref()));
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

    let output = compositor::command_output("gdctl", &args)?;

    if !output.status.success() {
        return Err(format!(
            "gdctl set failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
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
    let output = compositor::command_output("gdctl", &["show"])?;

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

pub(super) fn set_gnome_orientation(orientation: &Orientation) -> Result<(), String> {
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
        PRIMARY_INTERNAL_CONNECTOR.to_string(),
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
        args.push(SECONDARY_INTERNAL_CONNECTOR.to_string());

        match orientation {
            Orientation::Left => {
                args.extend(["--left-of", PRIMARY_INTERNAL_CONNECTOR].map(str::to_string))
            }
            Orientation::Right => {
                args.extend(["--right-of", PRIMARY_INTERNAL_CONNECTOR].map(str::to_string))
            }
            Orientation::Inverted => {
                args.extend(["--above", PRIMARY_INTERNAL_CONNECTOR].map(str::to_string))
            }
            Orientation::Normal => {
                args.extend(["--below", PRIMARY_INTERNAL_CONNECTOR].map(str::to_string))
            }
        }

        if let Some(transform) = transform {
            args.push("--transform".to_string());
            args.push(transform.to_string());
        }
    }

    run_command("gdctl", &args)
}

