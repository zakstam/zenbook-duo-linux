use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ipc::protocol::SessionBackend;

const DESKTOP_ENV_VARS: [&str; 3] = [
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "DESKTOP_SESSION",
];

const GNOME_READINESS_ARGS: &[&str] = &["show"];
const KDE_READINESS_ARGS: &[&str] = &["-j"];
const NIRI_READINESS_ARGS: &[&str] = &["msg", "--json", "outputs"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendCommandRunner {
    Compositor,
    Niri,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendProbe {
    pub backend: SessionBackend,
    pub readiness_program: &'static str,
    pub readiness_args: &'static [&'static str],
    pub readiness_runner: BackendCommandRunner,
    pub requires_gui_session: bool,
}

pub const SUPPORTED_BACKENDS: [SessionBackend; 3] = [
    SessionBackend::Niri,
    SessionBackend::Gnome,
    SessionBackend::Kde,
];

pub fn detect_backend_from_env() -> SessionBackend {
    detect_backend_from_hint(&desktop_hint_from_env(), resolve_niri_socket().is_some())
}

pub fn detect_backend_from_hint(current: &str, has_niri_socket: bool) -> SessionBackend {
    let current = current.to_ascii_lowercase().replace(':', " ");
    if contains_desktop_token(&current, "gnome") {
        SessionBackend::Gnome
    } else if contains_desktop_token(&current, "plasma") || contains_desktop_token(&current, "kde")
    {
        SessionBackend::Kde
    } else if contains_desktop_token(&current, "niri") || has_niri_socket {
        SessionBackend::Niri
    } else {
        SessionBackend::Unknown
    }
}

fn contains_desktop_token(haystack: &str, needle: &str) -> bool {
    haystack
        .split_whitespace()
        .any(|token| token == needle || token.contains(needle))
}

fn desktop_hint_from_env() -> String {
    DESKTOP_ENV_VARS
        .iter()
        .filter_map(|name| env::var(name).ok())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn backend_probe_order(hinted: &SessionBackend) -> Vec<SessionBackend> {
    backend_probe_sequence(hinted)
        .into_iter()
        .map(|probe| probe.backend)
        .collect()
}

pub fn backend_probe_sequence(hinted: &SessionBackend) -> Vec<BackendProbe> {
    let mut order = Vec::new();
    if hinted != &SessionBackend::Unknown {
        if let Some(probe) = backend_probe(hinted) {
            order.push(probe);
        }
    }
    for backend in SUPPORTED_BACKENDS {
        if !order
            .iter()
            .any(|probe: &BackendProbe| probe.backend == backend)
        {
            if let Some(probe) = backend_probe(&backend) {
                order.push(probe);
            }
        }
    }
    order
}

pub fn backend_probe(backend: &SessionBackend) -> Option<BackendProbe> {
    match backend {
        SessionBackend::Gnome => Some(BackendProbe {
            backend: SessionBackend::Gnome,
            readiness_program: "gdctl",
            readiness_args: GNOME_READINESS_ARGS,
            readiness_runner: BackendCommandRunner::Compositor,
            requires_gui_session: true,
        }),
        SessionBackend::Kde => Some(BackendProbe {
            backend: SessionBackend::Kde,
            readiness_program: "kscreen-doctor",
            readiness_args: KDE_READINESS_ARGS,
            readiness_runner: BackendCommandRunner::Compositor,
            requires_gui_session: true,
        }),
        SessionBackend::Niri => Some(BackendProbe {
            backend: SessionBackend::Niri,
            readiness_program: "niri",
            readiness_args: NIRI_READINESS_ARGS,
            readiness_runner: BackendCommandRunner::Niri,
            requires_gui_session: false,
        }),
        SessionBackend::Unknown => None,
    }
}

pub fn resolve_niri_socket() -> Option<PathBuf> {
    let env_socket = env::var_os("NIRI_SOCKET").map(PathBuf::from);
    resolve_niri_socket_from(env_socket.as_deref(), niri_runtime_dir().as_deref())
}

fn niri_runtime_dir() -> Option<PathBuf> {
    env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

pub fn resolve_niri_socket_from(
    env_socket: Option<&Path>,
    runtime_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(env_socket) = env_socket {
        if env_socket.exists() {
            return Some(env_socket.to_path_buf());
        }
    }

    let runtime_dir = runtime_dir?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    for entry in std::fs::read_dir(runtime_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if !name.starts_with("niri.") || !name.ends_with(".sock") {
            continue;
        }

        let modified = entry
            .metadata()
            .ok()?
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &newest {
            Some((current_modified, _)) if *current_modified >= modified => {}
            _ => newest = Some((modified, path)),
        }
    }

    newest.map(|(_, path)| path)
}

pub fn build_niri_command(args: &[&str]) -> Command {
    let mut command = Command::new("niri");
    command.args(args);
    if let Some(socket) = resolve_niri_socket() {
        command.env("NIRI_SOCKET", socket);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn detects_gnome_from_desktop_hint() {
        assert_eq!(
            detect_backend_from_hint("GNOME", false),
            SessionBackend::Gnome
        );
    }

    #[test]
    fn detects_kde_from_plasma_desktop_hint() {
        assert_eq!(
            detect_backend_from_hint("KDE Plasma", false),
            SessionBackend::Kde
        );
    }

    #[test]
    fn detects_niri_from_desktop_hint() {
        assert_eq!(
            detect_backend_from_hint("niri", false),
            SessionBackend::Niri
        );
    }

    #[test]
    fn detects_niri_from_socket_without_desktop_hint() {
        assert_eq!(detect_backend_from_hint("", true), SessionBackend::Niri);
    }

    #[test]
    fn empty_hint_without_niri_socket_remains_unknown() {
        assert_eq!(detect_backend_from_hint("", false), SessionBackend::Unknown);
    }

    #[test]
    fn probe_order_prefers_hint_then_supported_fallbacks() {
        assert_eq!(
            backend_probe_order(&SessionBackend::Kde),
            vec![
                SessionBackend::Kde,
                SessionBackend::Niri,
                SessionBackend::Gnome,
            ]
        );
    }

    #[test]
    fn resolve_niri_socket_prefers_existing_env_socket() {
        let runtime_dir = temp_runtime_dir("niri-env");
        let env_socket = runtime_dir.join("niri.wayland-1.env.sock");
        let listener =
            std::os::unix::net::UnixListener::bind(&env_socket).expect("bind env socket");

        let resolved =
            resolve_niri_socket_from(Some(env_socket.as_path()), Some(runtime_dir.as_path()))
                .expect("resolve niri socket");

        assert_eq!(resolved, env_socket);

        drop(listener);
        let _ = std::fs::remove_file(&env_socket);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn resolve_niri_socket_falls_back_to_latest_runtime_socket() {
        let runtime_dir = temp_runtime_dir("niri-fallback");
        let older_socket = runtime_dir.join("niri.wayland-1.older.sock");
        let newer_socket = runtime_dir.join("niri.wayland-1.newer.sock");
        let older_listener =
            std::os::unix::net::UnixListener::bind(&older_socket).expect("bind older socket");
        std::thread::sleep(Duration::from_millis(10));
        let newer_listener =
            std::os::unix::net::UnixListener::bind(&newer_socket).expect("bind newer socket");

        let resolved = resolve_niri_socket_from(None, Some(runtime_dir.as_path()))
            .expect("resolve niri socket");

        assert_eq!(resolved, newer_socket);

        drop(older_listener);
        drop(newer_listener);
        let _ = std::fs::remove_file(&older_socket);
        let _ = std::fs::remove_file(&newer_socket);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    fn temp_runtime_dir(label: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("zenbook-duo-{label}-{nanos}-{id}"));
        std::fs::create_dir_all(&dir).expect("create temp runtime dir");
        dir
    }
}
