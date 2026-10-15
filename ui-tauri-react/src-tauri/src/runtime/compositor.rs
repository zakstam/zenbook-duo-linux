use std::process::{Command, Output};

use serde_json::Value;

use crate::runtime::session;

pub fn command_output<S: AsRef<str>>(program: &str, args: &[S]) -> Result<Output, String> {
    Command::new(program)
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .map_err(|e| format!("Failed to run {program}: {e}"))
}

pub fn command_succeeds<S: AsRef<str>>(program: &str, args: &[S]) -> bool {
    command_output(program, args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn run_command<S: AsRef<str>>(program: &str, args: &[S]) -> Result<(), String> {
    let output = command_output(program, args)?;
    ensure_success(output)
}

pub fn niri_command_output(args: &[&str]) -> Result<Output, String> {
    session::build_niri_command(args)
        .output()
        .map_err(|e| format!("Failed to run niri msg: {e}"))
}

pub fn niri_command_succeeds(args: &[&str]) -> bool {
    niri_command_output(args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn run_niri_command(args: &[&str]) -> Result<(), String> {
    let output = niri_command_output(args)?;
    ensure_success(output)
}

pub fn kscreen_json() -> Result<Value, String> {
    let output = command_output("kscreen-doctor", &["-j"])?;
    if !output.status.success() {
        return Err(stderr_trimmed(&output));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid kscreen JSON: {e}"))
}

pub fn niri_outputs_json() -> Result<Value, String> {
    let output = niri_command_output(&["msg", "--json", "outputs"])?;
    if !output.status.success() {
        return Err(stderr_trimmed(&output));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid niri JSON: {e}"))
}

pub fn hyprland_monitors_json() -> Result<Value, String> {
    let output = command_output("hyprctl", &["monitors", "-j"])?;
    if !output.status.success() {
        return Err(stderr_trimmed(&output));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let start = raw
        .find(['[', '{'])
        .ok_or_else(|| "Invalid hyprctl JSON: missing JSON payload".to_string())?;
    serde_json::from_str(&raw[start..]).map_err(|e| format!("Invalid hyprctl JSON: {e}"))
}

pub fn hyprland_outputs_from_value(value: &Value) -> Result<Vec<Value>, String> {
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "Unexpected Hyprland monitors shape".into())
}

pub fn kde_outputs_from_value(value: &Value) -> Result<Vec<Value>, String> {
    value
        .get("outputs")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or_else(|| "Missing KDE outputs array".to_string())
}

pub fn kde_output_names_from_value(value: &Value) -> Result<Vec<String>, String> {
    Ok(kde_outputs_from_value(value)?
        .iter()
        .filter_map(|output| output.get("name").and_then(|v| v.as_str()))
        .map(ToString::to_string)
        .collect())
}

pub fn niri_outputs_from_value(value: &Value) -> Result<Vec<Value>, String> {
    if let Some(arr) = value.as_array() {
        Ok(arr.clone())
    } else if let Some(obj) = value.as_object() {
        Ok(obj.values().cloned().collect())
    } else {
        Err("Unexpected niri outputs shape".into())
    }
}

pub fn niri_output_names_from_value(value: &Value) -> Result<Vec<String>, String> {
    Ok(niri_outputs_from_value(value)?
        .iter()
        .filter_map(|output| output.get("name").and_then(|v| v.as_str()))
        .map(ToString::to_string)
        .collect())
}

pub fn kde_output_logical_size_from_value(value: &Value, name: &str) -> Result<(i64, i64), String> {
    for output in kde_outputs_from_value(value)? {
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

pub fn kde_enabled_output_count_from_value(value: &Value) -> Result<usize, String> {
    Ok(kde_outputs_from_value(value)?
        .iter()
        .filter(|output| {
            output
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count())
}

/// An output's logical size, rounded up. Niri reports the logical width and
/// height rounded to whole pixels, but places outputs with the fractional size:
/// at scale 1.66 the Duo panel is about 1085.4 tall yet reported as 1085, so a
/// screen placed at y=1085 overlaps it and niri moves it to the right. Rounding
/// the mode size divided by the scale up keeps a neighbour clear of it.
pub fn niri_output_logical_size_from_value(
    value: &Value,
    name: &str,
) -> Result<(i64, i64), String> {
    for output in niri_outputs_from_value(value)? {
        if output.get("name").and_then(|v| v.as_str()) == Some(name) {
            let logical = output
                .get("logical")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "Missing niri logical size".to_string())?;
            let width = logical.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
            let height = logical.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
            return Ok(niri_exact_logical_size(&output).unwrap_or((width, height)));
        }
    }

    Err(format!("Niri output {name} not found"))
}

fn niri_exact_logical_size(output: &Value) -> Option<(i64, i64)> {
    let logical = output.get("logical")?;
    let scale = logical.get("scale").and_then(|v| v.as_f64())?;
    if scale <= 0.0 {
        return None;
    }
    let mode = match output.get("current_mode")? {
        Value::Object(_) => output.get("current_mode")?.clone(),
        index => output
            .get("modes")?
            .as_array()?
            .get(index.as_u64()? as usize)?
            .clone(),
    };
    let width = mode.get("width").and_then(|v| v.as_f64())?;
    let height = mode.get("height").and_then(|v| v.as_f64())?;
    let transform = logical
        .get("transform")
        .or_else(|| output.get("transform"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let rotated = transform.contains("90") || transform.contains("270");
    let (width, height) = if rotated {
        (height, width)
    } else {
        (width, height)
    };
    Some((
        (width / scale).ceil() as i64,
        (height / scale).ceil() as i64,
    ))
}

pub fn niri_enabled_output_count_from_value(value: &Value) -> Result<usize, String> {
    Ok(niri_outputs_from_value(value)?
        .iter()
        .filter(|output| {
            !output
                .get("current_mode")
                .is_some_and(|value| value.is_null())
        })
        .count())
}

fn ensure_success(output: Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(stderr_trimmed(&output))
    }
}

fn stderr_trimmed(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kde_output_names_and_logical_size() {
        let value = serde_json::json!({
            "outputs": [
                { "name": "eDP-1", "enabled": true, "size": { "width": 2880, "height": 1800 }, "scale": 1.5 },
                { "name": "eDP-2", "enabled": false, "size": { "width": 2880, "height": 1800 }, "scale": 2.0 }
            ]
        });

        assert_eq!(
            kde_output_names_from_value(&value).expect("names should parse"),
            vec!["eDP-1".to_string(), "eDP-2".to_string()]
        );
        assert_eq!(
            kde_output_logical_size_from_value(&value, "eDP-1"),
            Ok((1920, 1200))
        );
        assert_eq!(kde_enabled_output_count_from_value(&value), Ok(1));
    }

    #[test]
    fn parses_niri_array_and_object_output_shapes() {
        let array_value = serde_json::json!([
            { "name": "eDP-1", "current_mode": { "width": 2880 }, "logical": { "width": 1440, "height": 900 } },
            { "name": "eDP-2", "current_mode": null, "logical": { "width": 1440, "height": 900 } }
        ]);
        let object_value = serde_json::json!({
            "eDP-1": { "name": "eDP-1", "current_mode": { "width": 2880 }, "logical": { "width": 1440, "height": 900 } },
            "eDP-2": { "name": "eDP-2", "current_mode": null, "logical": { "width": 1440, "height": 900 } }
        });

        assert_eq!(
            niri_output_names_from_value(&object_value).expect("names should parse"),
            vec!["eDP-1".to_string(), "eDP-2".to_string()]
        );
        assert_eq!(
            niri_output_logical_size_from_value(&array_value, "eDP-1"),
            Ok((1440, 900))
        );
        assert_eq!(niri_enabled_output_count_from_value(&array_value), Ok(1));
    }

    // Niri on a UX8406MA at scale 1.66 reported the top panel as 1736x1085 and,
    // after a rotation placed the bottom panel at y=1085, moved it to x=1737.
    #[test]
    fn niri_logical_size_rounds_up_so_a_stacked_screen_does_not_overlap() {
        let value = serde_json::json!([{
            "name": "eDP-1",
            "current_mode": 0,
            "modes": [{ "width": 2880, "height": 1800, "refresh_rate": 120000 }],
            "logical": { "x": 0, "y": 0, "width": 1736, "height": 1085,
                          "scale": 1.6583333333333334, "transform": "Normal" }
        }]);
        assert_eq!(
            niri_output_logical_size_from_value(&value, "eDP-1"),
            Ok((1737, 1086))
        );
    }

    #[test]
    fn niri_logical_size_follows_a_quarter_turn() {
        let value = serde_json::json!([{
            "name": "eDP-1",
            "current_mode": { "width": 2880, "height": 1800 },
            "logical": { "width": 1085, "height": 1736, "scale": 1.6583333333333334, "transform": "90" }
        }]);
        assert_eq!(
            niri_output_logical_size_from_value(&value, "eDP-1"),
            Ok((1086, 1737))
        );
    }
}
