fn main() {
    env_logger::init();

    match zenbook_duo_control_lib::usb_media_remap_helper::run_from_env() {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            zenbook_duo_control_lib::usb_media_remap_helper::log_error(&err);
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
