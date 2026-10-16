//! The Duo panels' brightness. The level is read from each panel's backlight
//! in sysfs and set through systemd-logind (`Session.SetBrightness`) on that
//! backlight, as the user, without root, on any desktop. On GNOME with the
//! Zenbook Duo Snap extension, the top panel goes through GNOME's own slider
//! logic instead, so GNOME's slider shows the level.
//!
//! Not through GNOME's Mutter: Mutter 50 drives the top panel's backlight for
//! eDP-2 as well, so setting the bottom panel there changes the top one, and
//! writing its values made GNOME's own slider move both panels. GNOME's
//! slider changes the top panel only; the brightness sync copies it to the
//! bottom panel when "same brightness" is on.

use serde::Serialize;

use crate::hardware::sysfs;

/// One Duo panel's brightness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelBrightness {
    /// "top" or "bottom".
    pub panel: &'static str,
    pub percent: u8,
}

/// The backlight device (sysfs name) of "top" or "bottom".
fn device(panel: &str) -> Option<std::path::PathBuf> {
    match panel {
        "top" => sysfs::primary_backlight_dir(),
        "bottom" => sysfs::secondary_backlight_dir(),
        _ => None,
    }
}

/// The level for a percentage: at least 1, so the panel never goes dark.
pub(crate) fn level_for_percent(max: u32, percent: u8) -> u32 {
    (u64::from(max) * u64::from(percent.min(100)) / 100).max(1) as u32
}

pub(crate) fn percent_of(level: u32, max: u32) -> u8 {
    ((u64::from(level) * 100 + u64::from(max) / 2) / u64::from(max.max(1))).min(100) as u8
}

fn read_value(dir: &std::path::Path, file: &str) -> Option<u32> {
    std::fs::read_to_string(dir.join(file)).ok()?.trim().parse().ok()
}

/// The Duo panels that have a backlight, with their brightness.
pub fn read_panels() -> Vec<PanelBrightness> {
    ["top", "bottom"]
        .into_iter()
        .filter_map(|panel| {
            let dir = device(panel)?;
            let max = read_value(&dir, "max_brightness")?;
            let level = read_value(&dir, "brightness")?;
            Some(PanelBrightness { panel, percent: percent_of(level, max) })
        })
        .collect()
}

/// The user's graphical session in logind, looked up by user ID: the
/// Control Panel and the session agent may run outside the login session,
/// where "session/auto" and "user/self" are unknown.
async fn session_path(connection: &zbus::Connection) -> String {
    const AUTO: &str = "/org/freedesktop/login1/session/auto";
    let uid = nix::unistd::Uid::current().as_raw();
    let user_path: Result<zbus::zvariant::OwnedObjectPath, _> = connection
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "GetUser",
            &(uid,),
        )
        .await
        .and_then(|reply| reply.body().deserialize());
    let Ok(user_path) = user_path else {
        return AUTO.to_string();
    };
    let Ok(user) = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        user_path,
        "org.freedesktop.login1.User",
    )
    .await
    else {
        return AUTO.to_string();
    };
    match user
        .get_property::<(String, zbus::zvariant::OwnedObjectPath)>("Display")
        .await
    {
        Ok((id, path)) if !id.is_empty() => path.to_string(),
        _ => AUTO.to_string(),
    }
}

/// Sets the top panel through GNOME's own brightness slider logic, via the
/// Zenbook Duo Snap extension (inside GNOME Shell), so GNOME's slider shows
/// the level. False without the extension or without GNOME's slider.
async fn set_top_through_gnome(percent: u8) -> bool {
    let Ok(connection) = zbus::Connection::session().await else {
        return false;
    };
    connection
        .call_method(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell/Extensions/ZenbookDuoSnap",
            Some("org.gnome.Shell.Extensions.ZenbookDuoSnap"),
            "SetTopBrightness",
            &(f64::from(percent),),
        )
        .await
        .ok()
        .and_then(|reply| reply.body().deserialize::<(bool,)>().ok())
        .is_some_and(|(done,)| done)
}

/// Sets a panel's backlight level through logind, on its own backlight.
pub async fn set_panel_level(dir: &std::path::Path, level: u32) -> Result<(), String> {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    set_level(&name, level).await
}

/// Sets a backlight device's level through logind, as the desktop user.
pub async fn set_level(device_name: &str, level: u32) -> Result<(), String> {
    let connection = zbus::Connection::system()
        .await
        .map_err(|e| format!("system bus: {e}"))?;
    let session = session_path(&connection).await;
    connection
        .call_method(
            Some("org.freedesktop.login1"),
            session.as_str(),
            Some("org.freedesktop.login1.Session"),
            "SetBrightness",
            &("backlight", device_name, level),
        )
        .await
        .map(|_| ())
        .map_err(|e| format!("setting the brightness of {device_name}: {e}"))
}

/// Sets "top", "bottom" or "both" Duo panels to `percent`.
pub async fn set_panels(panels: &str, percent: u8) -> Result<(), String> {
    let wanted: &[&str] = match panels {
        "top" => &["top"],
        "bottom" => &["bottom"],
        "both" => &["top", "bottom"],
        other => return Err(format!("unknown panel {other}")),
    };
    for panel in wanted {
        let Some(dir) = device(panel) else {
            continue;
        };
        if *panel == "top" && set_top_through_gnome(percent).await {
            continue;
        }
        let max = read_value(&dir, "max_brightness").unwrap_or(1);
        set_panel_level(&dir, level_for_percent(max, percent)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_and_level() {
        assert_eq!(level_for_percent(400, 50), 200);
        assert_eq!(level_for_percent(400, 0), 1, "never dark");
        assert_eq!(level_for_percent(400, 250), 400);
        assert_eq!(percent_of(221, 400), 55);
        assert_eq!(percent_of(400, 400), 100);
        assert_eq!(percent_of(5, 0), 100, "no division by zero");
    }
}

