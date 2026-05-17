use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::ipc::protocol::{Envelope, SessionCommand};
use crate::runtime::session_agent;

/// Session-agent IPC Interface.
///
/// Owns the wire-level command loop and response encoding while the
/// session-agent Module keeps command behavior behind `dispatch_session_command`.
pub(crate) async fn handle_session_command(stream: UnixStream) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed to read session command: {e}"))?
    {
        let envelope: Envelope<SessionCommand> = serde_json::from_str(&line)
            .map_err(|e| format!("Invalid session command JSON: {e}"))?;
        let response = session_agent::dispatch_session_command(envelope.payload);
        let line = serde_json::to_string(&Envelope::new(response))
            .map_err(|e| format!("Failed to encode session response: {e}"))?;
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("Failed to write session response: {e}"))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|e| format!("Failed to terminate session response: {e}"))?;
    }

    Ok(())
}
