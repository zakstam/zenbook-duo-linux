use super::*;

pub(crate) struct BrightnessSync;

impl BrightnessSync {
    pub(crate) async fn watch() -> Result<(), String> {
        watch_brightness_sync().await
    }
}

async fn watch_brightness_sync() -> Result<(), String> {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut last_seen: Option<u32> = None;

    loop {
        interval.tick().await;

        if !brightness_sync_enabled() || keyboard_attached_from_runtime() {
            continue;
        }

        let level = crate::hardware::sysfs::read_display_brightness();
        if last_seen == Some(level) {
            continue;
        }

        sync_secondary_brightness(level).await?;
        last_seen = Some(level);
    }
}

fn brightness_sync_enabled() -> bool {
    crate::commands::settings::load_settings_local().sync_brightness
}

pub(super) fn keyboard_attached_from_runtime() -> bool {
    let Ok(raw) = fs::read_to_string(paths::state_file_path()) else {
        return false;
    };
    let Ok(state) = serde_json::from_str::<RuntimeState>(&raw) else {
        return false;
    };
    state.status.keyboard_attached
}

/// Gives the bottom panel the top panel's brightness (as a percentage of each
/// one's range), through logind like GNOME's own brightness slider.
async fn sync_secondary_brightness(level: u32) -> Result<(), String> {
    let (Some(primary), Some(secondary)) = (
        crate::hardware::sysfs::primary_backlight_dir(),
        crate::hardware::sysfs::secondary_backlight_dir(),
    ) else {
        return Ok(());
    };
    let max = |dir: &std::path::Path| {
        fs::read_to_string(dir.join("max_brightness"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(1)
    };
    use crate::hardware::panel_brightness::{level_for_percent, percent_of, set_panel_level};
    let target = level_for_percent(max(&secondary), percent_of(level, max(&primary)));
    // Already there (GNOME's own slider sets both panels): nothing to do.
    let current = fs::read_to_string(secondary.join("brightness"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    if current == Some(target) {
        return Ok(());
    }
    set_panel_level(&secondary, target).await
}
