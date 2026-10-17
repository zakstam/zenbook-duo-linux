//! Keyboard backlight sleep: asks GNOME's idle monitor (Mutter) to report when
//! there has been no input (keys, touchpad, mouse, touch) for the configured
//! time, and when input comes again, and tells the daemon, which turns the
//! keyboard backlight off and back on. Independently of that setting, the
//! backlight also goes off while GNOME has blanked the screens (power saving)
//! and comes back with them.

use std::time::Duration;

use futures_util::StreamExt;

use crate::ipc::protocol::DaemonRequest;

/// How often a changed sleep time is picked up while waiting.
const SETTINGS_CHECK: Duration = Duration::from_secs(10);

fn idle_secs() -> u32 {
    crate::commands::settings::load_settings_local().keyboard_backlight_idle_secs
}

pub(crate) async fn watch() -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("session bus: {e}"))?;
    let monitor = zbus::Proxy::new(
        &connection,
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core",
        "org.gnome.Mutter.IdleMonitor",
    )
    .await
    .map_err(|e| format!("Mutter IdleMonitor: {e}"))?;
    let mut fired = monitor
        .receive_signal("WatchFired")
        .await
        .map_err(|e| format!("WatchFired: {e}"))?;

    loop {
        let secs = idle_secs();
        if secs == 0 {
            tokio::time::sleep(SETTINGS_CHECK).await;
            continue;
        }
        let idle_watch: u32 = monitor
            .call("AddIdleWatch", &(u64::from(secs) * 1000))
            .await
            .map_err(|e| format!("AddIdleWatch: {e}"))?;
        let became_idle = wait_for_watch(&mut fired, idle_watch, Some(secs)).await;
        let _: Result<(), _> = monitor.call("RemoveWatch", &idle_watch).await;
        if !became_idle {
            continue; // The sleep time changed: watch again with the new one.
        }

        // A user-active watch fires once, on the next input.
        let active_watch: u32 = monitor
            .call("AddUserActiveWatch", &())
            .await
            .map_err(|e| format!("AddUserActiveWatch: {e}"))?;
        report(true).await;
        wait_for_watch(&mut fired, active_watch, None).await;
        report(false).await;
    }
}

/// Waits until the watch fires (true). With `secs`, gives up (false) when the
/// sleep time setting no longer matches.
async fn wait_for_watch(
    fired: &mut zbus::proxy::SignalStream<'_>,
    watch: u32,
    secs: Option<u32>,
) -> bool {
    loop {
        tokio::select! {
            message = fired.next() => {
                let Some(message) = message else {
                    // GNOME Shell went away; the session ends with it.
                    std::future::pending::<()>().await;
                    return false;
                };
                if message.body().deserialize::<u32>().ok() == Some(watch) {
                    return true;
                }
            }
            _ = tokio::time::sleep(SETTINGS_CHECK), if secs.is_some() => {
                if Some(idle_secs()) != secs {
                    return false;
                }
            }
        }
    }
}

/// Follows Mutter's display power saving: screens blanked (any mode but 0,
/// e.g. after the screen-blank delay) turn the keyboard backlight off too.
pub(crate) async fn watch_screen_power() -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("session bus: {e}"))?;
    let display_config = zbus::Proxy::new(
        &connection,
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
    )
    .await
    .map_err(|e| format!("Mutter DisplayConfig: {e}"))?;
    let mut changes = display_config
        .receive_property_changed::<i32>("PowerSaveMode")
        .await;
    let mut blanked = false;
    while let Some(change) = changes.next().await {
        let Ok(mode) = change.get().await else {
            continue;
        };
        if (mode != 0) != blanked {
            blanked = mode != 0;
            report(blanked).await;
        }
    }
    Ok(())
}

async fn report(idle: bool) {
    let result = tokio::task::spawn_blocking(move || {
        crate::runtime::client::request(DaemonRequest::SetBacklightIdle { idle })
    })
    .await;
    if !matches!(result, Ok(Ok(crate::ipc::protocol::DaemonResponse::Ack))) {
        log::warn!("keyboard backlight sleep (idle={idle}) failed: {result:?}");
    }
}
