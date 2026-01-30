mod commands;
mod hotkeys;
pub mod hardware;
mod models;
pub mod usb_media_remap_helper;
mod watchers;

use std::io::Write;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

use commands::events::create_event_buffer;
use hotkeys::HotkeyState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let event_buffer = create_event_buffer();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(event_buffer.clone())
        .manage(HotkeyState::default())
        .setup(move |app| {
            let handle = app.handle().clone();

            // Build tray menu
            build_tray(&handle)?;

            // Apply hotkey from saved settings (best-effort).
            let settings = commands::settings::load_settings();
            let hotkeys = app.state::<HotkeyState>();
            if let Err(e) = hotkeys.set_usb_media_remap_hotkey(
                &handle,
                settings.usb_media_remap_hotkey_enabled,
                &settings.usb_media_remap_hotkey,
            ) {
                eprintln!("Failed to register hotkey: {e}");
            }

            // Start background watchers
            watchers::start_all_watchers(&handle, event_buffer.clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            // Minimize to tray on close instead of quitting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::status::get_status,
            commands::backlight::get_backlight,
            commands::backlight::set_backlight,
            commands::display::get_display_layout,
            commands::display::apply_display_layout,
            commands::display::set_orientation,
            commands::service::is_service_active,
            commands::service::restart_service,
            commands::settings::load_settings,
            commands::settings::save_settings,
            commands::logs::read_log,
            commands::logs::clear_log,
            commands::profiles::list_profiles,
            commands::profiles::save_profile,
            commands::profiles::delete_profile,
            commands::profiles::activate_profile,
            commands::events::get_recent_events,
            commands::diagnostics::diag_list_evdev,
            commands::diagnostics::diag_capture_evdev,
            commands::diagnostics::diag_capture_evdev_multi,
            commands::diagnostics::diag_list_hid,
            commands::diagnostics::diag_read_report_descriptor,
            commands::diagnostics::diag_capture_hidraw_pkexec,
            commands::usb_media_remap::usb_media_remap_status,
            commands::usb_media_remap::usb_media_remap_start,
            commands::usb_media_remap::usb_media_remap_stop,
            commands::hotkeys::apply_usb_media_remap_hotkey,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Small CLI helper used by the main binary when invoked with flags.
pub fn toggle_usb_media_remap_cli() -> Result<(), String> {
    // Don't rely on stdout/stderr visibility (e.g. GNOME custom shortcuts). Log to /tmp so users
    // can verify the command actually ran.
    fn log_line(msg: &str) {
        let uid = nix::unistd::Uid::current().as_raw();
        let path = format!("/tmp/zenbook-duo-control-hotkey-{uid}.log");
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{ts} - {msg}");
        }
    }

    log_line("toggle requested");

    let status = commands::usb_media_remap::get_status();
    log_line(&format!("current running={}", status.running));

    let res = if status.running {
        commands::usb_media_remap::stop_remap()
    } else {
        // Keep the process alive long enough for the polkit auth dialog to be completed when
        // launched from GNOME custom shortcuts.
        commands::usb_media_remap::start_remap_wait(std::time::Duration::from_secs(180))
    };

    match &res {
        Ok(()) => log_line("toggle completed ok"),
        Err(e) => log_line(&format!("toggle failed: {e}")),
    }

    res
}

fn build_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;

    let profile_docked = MenuItem::with_id(app, "profile_docked", "Docked", true, None::<&str>)?;
    let profile_tablet = MenuItem::with_id(app, "profile_tablet", "Tablet", true, None::<&str>)?;
    let profile_presentation = MenuItem::with_id(
        app,
        "profile_presentation",
        "Presentation",
        true,
        None::<&str>,
    )?;
    let profiles_submenu = Submenu::with_items(
        app,
        "Profiles",
        true,
        &[&profile_docked, &profile_tablet, &profile_presentation],
    )?;

    let bl_0 = MenuItem::with_id(app, "bl_0", "Backlight Off", true, None::<&str>)?;
    let bl_1 = MenuItem::with_id(app, "bl_1", "Backlight Low", true, None::<&str>)?;
    let bl_2 = MenuItem::with_id(app, "bl_2", "Backlight Medium", true, None::<&str>)?;
    let bl_3 = MenuItem::with_id(app, "bl_3", "Backlight High", true, None::<&str>)?;
    let backlight_submenu =
        Submenu::with_items(app, "Backlight", true, &[&bl_0, &bl_1, &bl_2, &bl_3])?;

    let separator2 = PredefinedMenuItem::separator(app)?;
    let usb_media_remap = MenuItem::with_id(
        app,
        "usb_media_remap",
        "Toggle USB Media Remap",
        true,
        None::<&str>,
    )?;
    let separator3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &separator1,
            &profiles_submenu,
            &backlight_submenu,
            &separator2,
            &usb_media_remap,
            &separator3,
            &quit,
        ],
    )?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Zenbook Duo Control")
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                "profile_docked" | "profile_tablet" | "profile_presentation" => {
                    let profile_id = id.strip_prefix("profile_").unwrap_or(id);
                    let _ = commands::profiles::activate_profile(profile_id.to_string());
                }
                id if id.starts_with("bl_") => {
                    if let Ok(level) = id[3..].parse::<u8>() {
                        let _ = hardware::hid::set_backlight(level);
                    }
                }
                "usb_media_remap" => {
                    let status = commands::usb_media_remap::get_status();
                    if status.running {
                        let _ = commands::usb_media_remap::stop_remap();
                    } else {
                        let _ = commands::usb_media_remap::start_remap();
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
