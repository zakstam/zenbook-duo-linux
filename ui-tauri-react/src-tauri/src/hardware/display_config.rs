use crate::models::{DisplayInfo, DisplayLayout, Orientation};
use std::process::Command;

/// Get the current display layout using `hyprctl`
pub fn get_display_layout() -> Result<DisplayLayout, String> {
    let output = Command::new("hyprctl")
        .args(["-j", "monitors"])
        .output()
        .map_err(|e| format!("Failed to run hyprctl: {e}"))?;

    if !output.status.success() {
        return Err("hyprctl command failed".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Fallback à gdctl si la sortie semble vide ou échoue
    // (pour conserver la compatibilité GNOME au cas où)
    if stdout.trim().is_empty() || stdout.trim() == "[]" {
        return Err("No monitors found in hyprctl".into());
    }

    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse hyprctl JSON: {e}"))?;

    let mut displays = Vec::new();

    if let Some(monitors) = parsed.as_array() {
        for monitor in monitors {
            let connector = monitor["name"].as_str().unwrap_or("").to_string();
            let width = monitor["width"].as_u64().unwrap_or(0) as u32;
            let height = monitor["height"].as_u64().unwrap_or(0) as u32;
            let refresh_rate = monitor["refreshRate"].as_f64().unwrap_or(60.0);
            let scale = monitor["scale"].as_f64().unwrap_or(1.0);
            let x = monitor["x"].as_i64().unwrap_or(0) as i32;
            let y = monitor["y"].as_i64().unwrap_or(0) as i32;

            // Hyprland transforms: 0=normal, 1=90, 2=180, 3=270
            let t = monitor["transform"].as_u64().unwrap_or(0);
            let transform = match t {
                1 => 90,
                2 => 180,
                3 => 270,
                _ => 0,
            };

            // Not always strictly defined in Hyprland like in GNOME, fallback true if ID=0
            let primary = monitor["focused"].as_bool().unwrap_or(monitor["id"].as_u64().unwrap_or(1) == 0);

            displays.push(DisplayInfo {
                connector,
                width,
                height,
                refresh_rate,
                scale,
                x,
                y,
                transform,
                primary,
            });
        }
    }

    Ok(DisplayLayout { displays })
}

/// Apply a display layout using hyprctl commands.
pub fn apply_display_layout(layout: &DisplayLayout) -> Result<(), String> {
    if layout.displays.is_empty() {
        return Err("No displays in layout".into());
    }

    for display in &layout.displays {
        let transform_id = match display.transform {
            90 => 1,
            180 => 2,
            270 => 3,
            _ => 0,
        };

        // hyprctl keyword monitor name,resolution,position,scale,transform,X
        let cmd_str = format!(
            "{},{}x{}@{},{}x{},{:.2},transform,{}",
            display.connector,
            display.width,
            display.height,
            display.refresh_rate,
            display.x,
            display.y,
            display.scale,
            transform_id
        );

        let output = Command::new("hyprctl")
            .args(["keyword", "monitor", &cmd_str])
            .output()
            .map_err(|e| format!("Failed to apply hyprctl config: {e}"))?;

        if !output.status.success() {
            return Err(format!("Failed to set monitor {}", display.connector));
        }
    }

    Ok(())
}

/// Set screen orientation using the duo bash command.
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
