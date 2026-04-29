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
            return Ok((width, height));
        }
    }

    Err(format!("Niri output {name} not found"))
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
}
