//! Bounded local IPC; no shell, eval, document mirror, or public network listener.
use crate::{
    contract::{ApplicationRequest, ApplicationResponse, MAX_MESSAGE_BYTES},
    registry::Endpoint,
};
use serde::{Deserialize, Serialize};
use std::{io, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
};
use tokio_util::sync::CancellationToken;

#[derive(Serialize, Deserialize)]
pub struct WireRequest {
    pub secret: String,
    pub request: ApplicationRequest,
}

pub async fn write_json(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= MAX_MESSAGE_BYTES {
        return Err(io::Error::other("message exceeds size limit"));
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

pub async fn read_json<T: serde::de::DeserializeOwned>(
    reader: impl AsyncRead + Unpin,
) -> io::Result<T> {
    let mut bytes = Vec::new();
    let mut bounded = BufReader::new(reader.take(MAX_MESSAGE_BYTES as u64));
    bounded.read_until(b'\n', &mut bytes).await?;
    if bytes.last() != Some(&b'\n') || bytes.len() >= MAX_MESSAGE_BYTES {
        return Err(io::Error::other("incomplete or oversized message"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub async fn call(
    endpoint: &Endpoint,
    request: ApplicationRequest,
    cancel: CancellationToken,
) -> io::Result<ApplicationResponse> {
    request.validate().map_err(io::Error::other)?;
    let request_id = request.request_id.clone();
    let round_trip = async {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, endpoint.port)).await?;
        write_json(
            &mut stream,
            &WireRequest {
                secret: endpoint.secret.clone(),
                request,
            },
        )
        .await?;
        let response: ApplicationResponse = read_json(stream).await?;
        if response.api_version != 1
            || response.request_id != request_id
            || response.instance_id != endpoint.instance_id
        {
            return Err(io::Error::other("application response identity mismatch"));
        }
        Ok(response)
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted, "request cancelled; query state before retrying a write")),
        result = tokio::time::timeout(Duration::from_secs(30), round_trip) =>
            result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "application timeout; query state before retrying a write"))?,
    }
}
