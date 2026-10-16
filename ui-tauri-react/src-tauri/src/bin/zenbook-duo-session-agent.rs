use zenbook_duo_control_lib::runtime::session_agent;

fn main() {
    zenbook_duo_control_lib::runtime::version::print_and_exit_if_requested(
        "zenbook-duo-session-agent",
    );
    env_logger::init();

    // Adopt the running desktop before any thread exists: it rewrites the
    // process environment, which the async runtime's threads then read.
    let live = session_agent::adopt_live_session_blocking();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("Failed to start async runtime: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = runtime.block_on(session_agent::run(live)) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
