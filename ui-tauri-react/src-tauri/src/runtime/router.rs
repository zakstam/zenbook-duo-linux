use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;

use crate::ipc::protocol::{DaemonRequest, DaemonResponse, Envelope, PROTOCOL_VERSION};
use crate::runtime::{daemon, state::RuntimeState};

/// Daemon request-router Interface.
///
/// This Module owns the wire-level loop and protocol error semantics. The
/// daemon Implementation owns the command behavior behind `dispatch_request`.
pub async fn handle_client(stream: UnixStream, state: Arc<RwLock<RuntimeState>>) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed to read daemon request: {e}"))?
    {
        let envelope: Envelope<DaemonRequest> =
            serde_json::from_str(&line).map_err(|e| format!("Invalid daemon request JSON: {e}"))?;

        if envelope.protocol_version != PROTOCOL_VERSION {
            write_response(
                &mut writer,
                DaemonResponse::Error {
                    message: format!(
                        "Protocol mismatch: expected {}, got {}",
                        PROTOCOL_VERSION, envelope.protocol_version
                    ),
                },
            )
            .await?;
            continue;
        }

        let response = daemon::dispatch_request(envelope.payload, state.clone()).await;
        write_response(&mut writer, response).await?;
    }

    Ok(())
}

async fn write_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: DaemonResponse,
) -> Result<(), String> {
    let line = serde_json::to_string(&Envelope::new(response))
        .map_err(|e| format!("Failed to encode daemon response: {e}"))?;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("Failed to write daemon response: {e}"))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|e| format!("Failed to terminate daemon response: {e}"))
}
