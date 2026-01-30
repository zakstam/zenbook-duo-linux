fn main() {
    if let Err(err) = zenbook_duo_control_lib::usb_media_remap_helper::run_from_env() {
        zenbook_duo_control_lib::usb_media_remap_helper::log_error(&err);
        eprintln!("{err}");
        std::process::exit(1);
    }
}

