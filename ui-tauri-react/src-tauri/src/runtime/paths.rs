use nix::unistd::Uid;
use std::path::PathBuf;

pub const APP_DIR_NAME: &str = "zenbook-duo";
pub const DAEMON_SOCKET_NAME: &str = "daemon.sock";
pub const SESSION_SOCKET_NAME: &str = "session-agent.sock";
pub const STATE_FILE_NAME: &str = "state.json";
pub const LOG_FILE_NAME: &str = "daemon.log";

pub fn system_runtime_dir() -> PathBuf {
    PathBuf::from("/var/lib").join(APP_DIR_NAME)
}

pub fn daemon_socket_path() -> PathBuf {
    system_runtime_dir().join(DAEMON_SOCKET_NAME)
}

pub fn state_file_path() -> PathBuf {
    system_runtime_dir().join(STATE_FILE_NAME)
}

pub fn log_file_path() -> PathBuf {
    system_runtime_dir().join(LOG_FILE_NAME)
}

pub fn user_runtime_dir(uid: u32) -> PathBuf {
    PathBuf::from(format!("/run/user/{uid}/{APP_DIR_NAME}"))
}

pub fn current_user_runtime_dir() -> PathBuf {
    user_runtime_dir(Uid::current().as_raw())
}

pub fn current_user_session_socket_path() -> PathBuf {
    current_user_runtime_dir().join(SESSION_SOCKET_NAME)
}
