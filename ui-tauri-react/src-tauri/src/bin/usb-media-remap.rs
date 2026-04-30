fn main() {
    zenbook_duo_control_lib::runtime::version::print_and_exit_if_requested("usb-media-remap");

    if let Err(err) = zenbook_duo_control_lib::usb_media_remap_helper::run_from_env() {
        zenbook_duo_control_lib::usb_media_remap_helper::log_error(&err);
        eprintln!("{err}");
        std::process::exit(1);
    }
}
