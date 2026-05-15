use std::process::Command;

use crate::models::ThemePreference;

#[tauri::command]
pub fn get_system_theme() -> Option<ThemePreference> {
    system_theme_from_portal().or_else(system_theme_from_gsettings)
}

fn system_theme_from_portal() -> Option<ThemePreference> {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_portal_color_scheme(&stdout)
}

fn system_theme_from_gsettings() -> Option<ThemePreference> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gsettings_color_scheme(&stdout)
}

fn parse_portal_color_scheme(value: &str) -> Option<ThemePreference> {
    if value.contains("uint32 1") {
        Some(ThemePreference::Dark)
    } else if value.contains("uint32 2") || value.contains("uint32 0") {
        Some(ThemePreference::Light)
    } else {
        None
    }
}

fn parse_gsettings_color_scheme(value: &str) -> Option<ThemePreference> {
    let normalized = value.trim().trim_matches('\'').trim_matches('"');

    if normalized.contains("prefer-dark") {
        Some(ThemePreference::Dark)
    } else if normalized.contains("prefer-light") || normalized.contains("default") {
        Some(ThemePreference::Light)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_portal_color_scheme() {
        assert_eq!(
            parse_portal_color_scheme("(<<uint32 1>>,)"),
            Some(ThemePreference::Dark)
        );
        assert_eq!(
            parse_portal_color_scheme("(<<uint32 0>>,)"),
            Some(ThemePreference::Light)
        );
        assert_eq!(
            parse_portal_color_scheme("(<<uint32 2>>,)"),
            Some(ThemePreference::Light)
        );
        assert_eq!(parse_portal_color_scheme(""), None);
    }

    #[test]
    fn parses_gsettings_color_scheme() {
        assert_eq!(
            parse_gsettings_color_scheme("'prefer-dark'"),
            Some(ThemePreference::Dark)
        );
        assert_eq!(
            parse_gsettings_color_scheme("'prefer-light'"),
            Some(ThemePreference::Light)
        );
        assert_eq!(
            parse_gsettings_color_scheme("'default'"),
            Some(ThemePreference::Light)
        );
        assert_eq!(parse_gsettings_color_scheme("''"), None);
    }
}
