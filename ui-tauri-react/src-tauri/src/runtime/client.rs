use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use crate::ipc::protocol::{DaemonRequest, DaemonResponse, Envelope};
use crate::runtime::paths;

pub fn request(request: DaemonRequest) -> Result<DaemonResponse, String> {
    let mut stream = UnixStream::connect(paths::daemon_socket_path())
        .map_err(|e| format!("Failed to connect to daemon socket: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("Failed to set daemon read timeout: {e}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("Failed to set daemon write timeout: {e}"))?;

    let line = serde_json::to_string(&Envelope::new(request))
        .map_err(|e| format!("Failed to encode daemon request: {e}"))?;
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write daemon request: {e}"))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("Failed to terminate daemon request: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader
        .read_line(&mut reply)
        .map_err(|e| format!("Failed to read daemon response: {e}"))?;
    if reply.trim().is_empty() {
        return Err("Daemon returned an empty response".into());
    }

    let envelope: Envelope<DaemonResponse> = serde_json::from_str(reply.trim_end())
        .map_err(|e| format!("Invalid daemon response JSON: {e}"))?;
    Ok(envelope.payload)
}
