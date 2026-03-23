use zenbook_duo_control_lib::hardware::{hid, sysfs};
use zenbook_duo_control_lib::ipc::protocol::{DaemonRequest, DaemonResponse, LifecyclePhase};
use zenbook_duo_control_lib::runtime::client;

fn main() {
    env_logger::init();

    let Some(raw_phase) = std::env::args().nth(1) else {
        eprintln!("usage: zenbook-duo-lifecycle <pre|post|hibernate|thaw|boot|shutdown>");
        std::process::exit(2);
    };

    let phase = match parse_phase(&raw_phase) {
        Ok(phase) => phase,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let result = match client::request(DaemonRequest::HandleLifecycle {
        phase: phase.clone(),
    }) {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => fallback_lifecycle(&phase)
            .map_err(|fallback| format!("{message}; fallback failed: {fallback}")),
        Ok(_) => fallback_lifecycle(&phase),
        Err(_) => fallback_lifecycle(&phase),
    };

    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn parse_phase(raw: &str) -> Result<LifecyclePhase, String> {
    match raw {
        "pre" => Ok(LifecyclePhase::Pre),
        "post" => Ok(LifecyclePhase::Post),
        "hibernate" => Ok(LifecyclePhase::Hibernate),
        "thaw" => Ok(LifecyclePhase::Thaw),
        "boot" => Ok(LifecyclePhase::Boot),
        "shutdown" => Ok(LifecyclePhase::Shutdown),
        _ => Err(format!("unsupported lifecycle phase: {raw}")),
    }
}

fn fallback_lifecycle(phase: &LifecyclePhase) -> Result<(), String> {
    match phase {
        LifecyclePhase::Pre | LifecyclePhase::Hibernate | LifecyclePhase::Shutdown => {
            hid::set_backlight(0)
        }
        LifecyclePhase::Post | LifecyclePhase::Thaw | LifecyclePhase::Boot => {
            hid::set_backlight(sysfs::read_backlight_level())
        }
    }
}
