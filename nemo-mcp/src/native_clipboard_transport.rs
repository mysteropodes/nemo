//! F12 operation-specific complete UTF8 budgets on the existing authenticated wire.
//! 4MiB serialized records/order +4096*256 cell/container framing +4096 headers.
//! Native record minimum is >64 bytes, so remaining cell reserve covers all record
//! commas too. These are per-message limits, not aggregate retained-memory limits.
use super::{
    AuthenticatedWireRequest, NativeApplicationRequest, NativeApplicationResponse,
    NativeWireRequest,
};
use serde_json::Value;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
pub(crate) const DATA_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MESSAGE_BYTES: usize = DATA_BYTES + 4096 * 256 + 4096;
pub(crate) const WRAPPER_BYTES: usize = 4096;
// Authenticated TCP and MCP ingress only; SDK structured stdout duplicates/escapes
// the native reply and has separately derived resource accounting, not this cap.
pub(crate) const FRAME_BYTES: usize = MESSAGE_BYTES + WRAPPER_BYTES;
pub(crate) fn owns(op: &str) -> bool {
    matches!(
        op,
        "command.document.object.frames.clipboard.copy"
            | "command.document.object.frames.clipboard.cut"
            | "command.document.object.frames.clipboard.paste"
    )
}
pub(crate) fn response_limit(op: &str) -> usize {
    if matches!(
        op,
        "command.document.object.frames.clipboard.copy"
            | "command.document.object.frames.clipboard.cut"
    ) {
        MESSAGE_BYTES
    } else {
        4096
    }
}
pub fn native_response_byte_limit(operation: &str) -> usize {
    response_limit(operation)
}
pub(crate) fn data_admitted(cells: &Value) -> bool {
    let Some(cells) = cells.as_array() else {
        return false;
    };
    let mut total = 0usize;
    for cell in cells {
        let Some(records) = cell.get("records").and_then(Value::as_array) else {
            return false;
        };
        for value in records.iter().chain(cell.get("orderElements")) {
            let Ok(bytes) = serde_json::to_vec(value) else {
                return false;
            };
            let Some(next) = total
                .checked_add(bytes.len())
                .filter(|next| *next <= DATA_BYTES)
            else {
                return false;
            };
            total = next;
        }
    }
    true
}
async fn write(writer: &mut (impl AsyncWrite + Unpin), bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}
pub(super) async fn write_request(
    writer: &mut (impl AsyncWrite + Unpin),
    request: &NativeWireRequest,
) -> io::Result<()> {
    if !owns(&request.native_request.operation) {
        return super::write_json(writer, request).await;
    }
    let inner = serde_json::to_vec(&request.native_request)?;
    let bytes = serde_json::to_vec(request)?;
    if inner.len() > MESSAGE_BYTES
        || (bytes.len() > crate::contract::MAX_MESSAGE_BYTES - 2
            && bytes.len().saturating_sub(inner.len()) > WRAPPER_BYTES)
    {
        return Err(io::Error::other(
            "clipboard authenticated request exceeds original encoded bound",
        ));
    }
    write(writer, &bytes).await
}
pub async fn write_native_response(
    writer: &mut (impl AsyncWrite + Unpin),
    response: &NativeApplicationResponse,
    request: &NativeApplicationRequest,
) -> io::Result<()> {
    if !owns(&request.operation) {
        return super::write_json(writer, response).await;
    }
    super::validate_native_response(response, request)?;
    let bytes = serde_json::to_vec(response)?;
    write(writer, &bytes).await
}
pub async fn read_authenticated(
    reader: impl AsyncRead + Unpin,
) -> io::Result<AuthenticatedWireRequest> {
    let mut bytes = Vec::new();
    BufReader::new(reader.take((FRAME_BYTES + 2) as u64))
        .read_until(b'\n', &mut bytes)
        .await?;
    if bytes.pop() != Some(b'\n') || bytes.len() > FRAME_BYTES {
        return Err(io::Error::other(
            "incomplete or oversized authenticated message",
        ));
    }
    let message: AuthenticatedWireRequest = super::decode_json_bounded(&bytes, FRAME_BYTES)?;
    if bytes.len() > crate::contract::MAX_MESSAGE_BYTES - 2
        && !super::native_request_bytes::clipboard_wrapper_admitted(&bytes, false, 0)
    {
        return Err(io::Error::other(
            "larger framing requires an actual clipboard native request",
        ));
    }
    Ok(message)
}
#[cfg(test)]
#[path = "native_clipboard_transport_tests.rs"]
mod tests;
