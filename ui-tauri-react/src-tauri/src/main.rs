// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The app also acts as a small privileged helper when invoked via pkexec.
    // This avoids needing to bundle a separate binary alongside the main app.
    if std::env::args().any(|a| a == "--usb-media-remap-helper") {
        match zenbook_duo_control_lib::usb_media_remap_helper::run_from_env() {
            Ok(()) => std::process::exit(0),
            Err(err) => {
                zenbook_duo_control_lib::usb_media_remap_helper::log_error(&err);
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }

    zenbook_duo_control_lib::run()
}
