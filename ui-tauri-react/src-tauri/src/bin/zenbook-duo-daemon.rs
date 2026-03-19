#[tokio::main]
async fn main() {
    env_logger::init();

    if let Err(err) = zenbook_duo_control_lib::runtime::daemon::run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
