use std::collections::HashMap;
use std::env;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
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
const HYPRLAND_READINESS_ARGS: &[&str] = &["monitors", "-j"];

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

pub const SUPPORTED_BACKENDS: [SessionBackend; 4] = [
    SessionBackend::Hyprland,
    SessionBackend::Niri,
    SessionBackend::Gnome,
    SessionBackend::Kde,
];

pub fn detect_backend_from_env() -> SessionBackend {
    detect_backend_from_hint(&desktop_hint_from_env(), resolve_niri_socket().is_some())
}

pub fn detect_backend_from_hint(current: &str, has_niri_socket: bool) -> SessionBackend {
    let current = current.to_ascii_lowercase().replace(':', " ");
    if contains_desktop_token(&current, "hyprland") || contains_desktop_token(&current, "hypr") {
        SessionBackend::Hyprland
    } else if contains_desktop_token(&current, "gnome") {
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
        SessionBackend::Hyprland => Some(BackendProbe {
            backend: SessionBackend::Hyprland,
            readiness_program: "hyprctl",
            readiness_args: HYPRLAND_READINESS_ARGS,
            readiness_runner: BackendCommandRunner::Compositor,
            requires_gui_session: true,
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

/// The desktop session the session agent follows. A lingering user manager
/// keeps the agent alive across logout, so the agent must find the desktop
/// that is running now instead of trusting the environment it started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSession {
    pub backend: SessionBackend,
    /// Value written to XDG_CURRENT_DESKTOP and friends.
    pub desktop_name: String,
    pub wayland_display: Option<String>,
    pub niri_socket: Option<PathBuf>,
    pub hyprland_signature: Option<String>,
}

impl LiveSession {
    pub fn describe(&self) -> String {
        let mut text = self.desktop_name.clone();
        if let Some(display) = &self.wayland_display {
            text.push_str(&format!(" on {display}"));
        }
        if let Some(signature) = &self.hyprland_signature {
            text.push_str(&format!(" (instance {signature})"));
        }
        if let Some(socket) = &self.niri_socket {
            text.push_str(&format!(" (socket {})", socket.display()));
        }
        text
    }

    /// Point this process's environment at the session, clearing the other
    /// compositor's variables. Call before any thread starts.
    pub fn adopt_into_process_env(&self) {
        for name in DESKTOP_ENV_VARS {
            env::set_var(name, &self.desktop_name);
        }
        if let Some(display) = &self.wayland_display {
            env::set_var("WAYLAND_DISPLAY", display);
        }
        match &self.niri_socket {
            Some(socket) => env::set_var("NIRI_SOCKET", socket),
            None => env::remove_var("NIRI_SOCKET"),
        }
        match &self.hyprland_signature {
            Some(signature) => env::set_var("HYPRLAND_INSTANCE_SIGNATURE", signature),
            None => env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    }
}

/// Find the desktop session running now for this user. Hyprland and niri are
/// found from their live IPC sockets in the runtime dir, so no desktop has to
/// export its environment first; GNOME and KDE are taken from the user
/// manager's current environment when its Wayland socket is live.
pub fn discover_live_session(
    runtime_dir: &Path,
    manager_env: &HashMap<String, String>,
) -> Option<LiveSession> {
    let hint = desktop_hint_from_map(manager_env);
    let hinted = detect_backend_from_hint(&hint, false);

    let mut candidates: Vec<(SystemTime, LiveSession)> = Vec::new();
    candidates.extend(live_hyprland_sessions(runtime_dir));
    candidates.extend(live_niri_sessions(runtime_dir));

    let pick = |only_hinted: bool| {
        candidates
            .iter()
            .filter(|(_, session)| !only_hinted || session.backend == hinted)
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, session)| session.clone())
    };
    if let Some(session) = pick(true).or_else(|| pick(false)) {
        return Some(session);
    }

    if matches!(hinted, SessionBackend::Gnome | SessionBackend::Kde) {
        let wayland_display = manager_env
            .get("WAYLAND_DISPLAY")
            .filter(|value| !value.is_empty())
            .cloned();
        let live = match &wayland_display {
            Some(display) => socket_answers(&wayland_socket_path(runtime_dir, display)),
            None => manager_env.get("DISPLAY").is_some_and(|v| !v.is_empty()),
        };
        if live {
            let desktop_name = manager_env
                .get("XDG_CURRENT_DESKTOP")
                .filter(|value| !value.is_empty())
                .cloned()
                .unwrap_or(hint);
            return Some(LiveSession {
                backend: hinted,
                desktop_name,
                wayland_display,
                niri_socket: None,
                hyprland_signature: None,
            });
        }
    }

    None
}

/// Whether the session's compositor still answers. False once the user has
/// logged out of it.
pub fn session_is_live(session: &LiveSession, runtime_dir: &Path) -> bool {
    if let Some(signature) = &session.hyprland_signature {
        return socket_answers(&hyprland_socket_path(runtime_dir, signature));
    }
    if let Some(socket) = &session.niri_socket {
        return socket_answers(socket);
    }
    match &session.wayland_display {
        Some(display) => socket_answers(&wayland_socket_path(runtime_dir, display)),
        // An X11-only session leaves nothing to probe; assume it is live.
        None => true,
    }
}

/// The user manager's environment as it is now (desktops update it at login),
/// falling back to this process's environment.
pub fn user_manager_environment() -> HashMap<String, String> {
    let output = Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            parse_show_environment(&String::from_utf8_lossy(&output.stdout))
        }
        _ => env::vars().collect(),
    }
}

pub fn parse_show_environment(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            let value = value
                .strip_prefix("$'")
                .and_then(|v| v.strip_suffix('\''))
                .unwrap_or(value);
            (key.to_string(), value.to_string())
        })
        .collect()
}

fn desktop_hint_from_map(vars: &HashMap<String, String>) -> String {
    // XDG_CURRENT_DESKTOP is what a desktop sets for itself; the other two can
    // be left over from a previous login, so they only count when it is empty.
    if let Some(current) = vars.get("XDG_CURRENT_DESKTOP").filter(|v| !v.is_empty()) {
        return current.clone();
    }
    DESKTOP_ENV_VARS
        .iter()
        .filter_map(|name| vars.get(*name).cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn live_hyprland_sessions(runtime_dir: &Path) -> Vec<(SystemTime, LiveSession)> {
    let Ok(entries) = std::fs::read_dir(runtime_dir.join("hypr")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let signature = entry.file_name().to_str()?.to_string();
            let socket = hyprland_socket_path(runtime_dir, &signature);
            if !socket_answers(&socket) {
                return None;
            }
            let wayland_display = std::fs::read_to_string(entry.path().join("hyprland.lock"))
                .ok()
                .and_then(|lock| lock.lines().nth(1).map(|line| line.trim().to_string()))
                .filter(|line| !line.is_empty());
            Some((
                modified_time(&socket),
                LiveSession {
                    backend: SessionBackend::Hyprland,
                    desktop_name: "Hyprland".into(),
                    wayland_display,
                    niri_socket: None,
                    hyprland_signature: Some(signature),
                },
            ))
        })
        .collect()
}

fn live_niri_sessions(runtime_dir: &Path) -> Vec<(SystemTime, LiveSession)> {
    let Ok(entries) = std::fs::read_dir(runtime_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            // niri.<wayland-display>.<pid>.sock
            let inner = name.strip_prefix("niri.")?.strip_suffix(".sock")?;
            let (wayland_display, _pid) = inner.rsplit_once('.')?;
            let socket = entry.path();
            if !socket_answers(&socket) {
                return None;
            }
            Some((
                modified_time(&socket),
                LiveSession {
                    backend: SessionBackend::Niri,
                    desktop_name: "niri".into(),
                    wayland_display: Some(wayland_display.to_string()),
                    niri_socket: Some(socket),
                    hyprland_signature: None,
                },
            ))
        })
        .collect()
}

fn hyprland_socket_path(runtime_dir: &Path, signature: &str) -> PathBuf {
    runtime_dir.join("hypr").join(signature).join(".socket.sock")
}

fn wayland_socket_path(runtime_dir: &Path, display: &str) -> PathBuf {
    let display = Path::new(display);
    if display.is_absolute() {
        display.to_path_buf()
    } else {
        runtime_dir.join(display)
    }
}

fn socket_answers(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

fn modified_time(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
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
    fn detects_hyprland_from_current_desktop_hint() {
        assert_eq!(
            detect_backend_from_hint("Hyprland", false),
            SessionBackend::Hyprland
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
                SessionBackend::Hyprland,
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

    fn env_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn live_hyprland(runtime_dir: &Path, signature: &str) -> std::os::unix::net::UnixListener {
        let dir = runtime_dir.join("hypr").join(signature);
        std::fs::create_dir_all(&dir).expect("hypr dir");
        std::fs::write(dir.join("hyprland.lock"), "4242\nwayland-1\n").expect("lock");
        std::os::unix::net::UnixListener::bind(dir.join(".socket.sock")).expect("bind hypr")
    }

    #[test]
    fn discovers_live_hyprland_even_when_manager_env_still_says_niri() {
        let runtime_dir = temp_runtime_dir("live-hypr");
        // niri left a dead socket behind; Hyprland is the one answering.
        drop(
            std::os::unix::net::UnixListener::bind(runtime_dir.join("niri.wayland-1.99.sock"))
                .expect("bind niri"),
        );
        let _hypr = live_hyprland(&runtime_dir, "sig1");
        let manager = env_map(&[("XDG_CURRENT_DESKTOP", "niri"), ("NIRI_SOCKET", "x")]);

        let session = discover_live_session(&runtime_dir, &manager).expect("live session");

        assert_eq!(session.backend, SessionBackend::Hyprland);
        assert_eq!(session.hyprland_signature.as_deref(), Some("sig1"));
        assert_eq!(session.wayland_display.as_deref(), Some("wayland-1"));
        assert_eq!(session.niri_socket, None);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn discovers_live_niri_and_its_wayland_display() {
        let runtime_dir = temp_runtime_dir("live-niri");
        // A Hyprland instance dir from an earlier login with a dead socket.
        drop(live_hyprland(&runtime_dir, "old"));
        let socket = runtime_dir.join("niri.wayland-2.123.sock");
        let _niri = std::os::unix::net::UnixListener::bind(&socket).expect("bind niri");
        let manager = env_map(&[("XDG_CURRENT_DESKTOP", "Hyprland")]);

        let session = discover_live_session(&runtime_dir, &manager).expect("live session");

        assert_eq!(session.backend, SessionBackend::Niri);
        assert_eq!(session.niri_socket.as_deref(), Some(socket.as_path()));
        assert_eq!(session.wayland_display.as_deref(), Some("wayland-2"));
        assert_eq!(session.hyprland_signature, None);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn prefers_the_hinted_compositor_when_two_are_live() {
        let runtime_dir = temp_runtime_dir("live-both");
        let _hypr = live_hyprland(&runtime_dir, "sig2");
        let _niri = std::os::unix::net::UnixListener::bind(runtime_dir.join("niri.wayland-2.5.sock"))
            .expect("bind niri");

        let niri = discover_live_session(&runtime_dir, &env_map(&[("XDG_CURRENT_DESKTOP", "niri")]));
        let hypr =
            discover_live_session(&runtime_dir, &env_map(&[("XDG_CURRENT_DESKTOP", "Hyprland")]));

        assert_eq!(niri.expect("niri").backend, SessionBackend::Niri);
        assert_eq!(hypr.expect("hypr").backend, SessionBackend::Hyprland);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn nothing_live_after_logout_means_no_session() {
        let runtime_dir = temp_runtime_dir("logged-out");
        drop(live_hyprland(&runtime_dir, "gone"));
        let manager = env_map(&[("XDG_CURRENT_DESKTOP", "Hyprland"), ("WAYLAND_DISPLAY", "wayland-1")]);

        assert_eq!(discover_live_session(&runtime_dir, &manager), None);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn gnome_comes_from_manager_env_while_its_wayland_socket_is_live() {
        let runtime_dir = temp_runtime_dir("live-gnome");
        let listener = std::os::unix::net::UnixListener::bind(runtime_dir.join("wayland-0"))
            .expect("bind wayland");
        let manager = env_map(&[
            ("XDG_CURRENT_DESKTOP", "GNOME"),
            ("DESKTOP_SESSION", "niri"),
            ("WAYLAND_DISPLAY", "wayland-0"),
        ]);

        let session = discover_live_session(&runtime_dir, &manager).expect("gnome");
        assert_eq!(session.backend, SessionBackend::Gnome);
        assert!(session_is_live(&session, &runtime_dir));

        drop(listener);
        assert!(!session_is_live(&session, &runtime_dir));
        assert_eq!(discover_live_session(&runtime_dir, &manager), None);
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn session_stops_being_live_when_its_compositor_exits() {
        let runtime_dir = temp_runtime_dir("hypr-exit");
        let hypr = live_hyprland(&runtime_dir, "sig3");
        let session =
            discover_live_session(&runtime_dir, &HashMap::new()).expect("live hyprland");
        assert!(session_is_live(&session, &runtime_dir));

        drop(hypr);

        assert!(!session_is_live(&session, &runtime_dir));
        let _ = std::fs::remove_dir_all(&runtime_dir);
    }

    #[test]
    fn parses_systemctl_show_environment() {
        let vars = parse_show_environment(
            "XDG_CURRENT_DESKTOP=Hyprland\nWAYLAND_DISPLAY=wayland-1\nODD=$'a b'\n",
        );
        assert_eq!(vars.get("XDG_CURRENT_DESKTOP").map(String::as_str), Some("Hyprland"));
        assert_eq!(vars.get("WAYLAND_DISPLAY").map(String::as_str), Some("wayland-1"));
        assert_eq!(vars.get("ODD").map(String::as_str), Some("a b"));
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
