#[tokio::main]
async fn main() {
    zenbook_duo_control_lib::runtime::version::print_and_exit_if_requested("zenbook-duo-daemon");
    env_logger::init();

    if let Err(err) = zenbook_duo_control_lib::runtime::daemon::run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
