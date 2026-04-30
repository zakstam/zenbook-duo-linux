pub fn print_and_exit_if_requested(binary_name: &str) {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("{binary_name} {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
}
