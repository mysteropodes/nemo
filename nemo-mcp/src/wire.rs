//! Bounded local IPC; no shell, eval, document mirror, or public network listener.
use crate::{
    contract::{
        bounded_identifier, ApplicationRequest, ApplicationResponse, NativeApplicationRequest,
        NativeApplicationResponse, NativeHostStatus, NativeStatusRequest, MAX_MESSAGE_BYTES,
        MAX_SAFE_REVISION, NATIVE_API_VERSION, NATIVE_MAX_MESSAGE_BYTES,
    },
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
#[serde(deny_unknown_fields)]
pub struct WireRequest {
    pub secret: String,
    pub request: ApplicationRequest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeWireRequest {
    pub secret: String,
    pub native_request: NativeApplicationRequest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeStatusWireRequest {
    pub secret: String,
    pub native_status: NativeStatusRequest,
}

/// Host-side authenticated request parser. The legacy member remains `request`
/// byte-for-byte; v2 application and status traffic have distinct top-level keys.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthenticatedWireRequest {
    Legacy(WireRequest),
    Native(NativeWireRequest),
    NativeStatus(NativeStatusWireRequest),
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
    // Preserve the v1 rule exactly: content + newline must be strictly smaller
    // than MAX_MESSAGE_BYTES.
    read_json_bounded(reader, MAX_MESSAGE_BYTES - 2).await
}

async fn read_json_bounded<T: serde::de::DeserializeOwned>(
    reader: impl AsyncRead + Unpin,
    max_content_bytes: usize,
) -> io::Result<T> {
    let mut bytes = Vec::new();
    let limit = max_content_bytes
        .checked_add(1)
        .ok_or_else(|| io::Error::other("message limit overflow"))?;
    let mut bounded = BufReader::new(reader.take(limit as u64));
    bounded.read_until(b'\n', &mut bytes).await?;
    if bytes.last() != Some(&b'\n') || bytes.len() > limit {
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

pub async fn call_native(
    endpoint: &Endpoint,
    request: NativeApplicationRequest,
    cancel: CancellationToken,
) -> io::Result<NativeApplicationResponse> {
    request.validate().map_err(io::Error::other)?;
    let request_id = request.request_id.clone();
    let document_id = request.document_id.clone();
    let response_contract = request.clone();
    let round_trip = async {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, endpoint.port)).await?;
        write_json(
            &mut stream,
            &NativeWireRequest {
                secret: endpoint.secret.clone(),
                native_request: request,
            },
        )
        .await?;
        let response: NativeApplicationResponse =
            read_json_bounded(stream, NATIVE_MAX_MESSAGE_BYTES).await?;
        validate_native_response(&response, &response_contract)?;
        let reports_replacement = !response.ok
            && response
                .error
                .as_ref()
                .is_some_and(|error| error.code == "wrong_document");
        if response.api_version != NATIVE_API_VERSION
            || response.request_id != request_id
            || response.instance_id != endpoint.instance_id
            || (response.document_id != document_id && !reports_replacement)
        {
            return Err(io::Error::other("native response identity mismatch"));
        }
        Ok(response)
    };
    cancellable(round_trip, cancel).await
}

fn validate_native_response(
    response: &NativeApplicationResponse,
    request: &NativeApplicationRequest,
) -> io::Result<()> {
    let identity_is_valid = response.api_version == NATIVE_API_VERSION
        && bounded_identifier(&response.request_id)
        && bounded_identifier(&response.instance_id)
        && bounded_identifier(&response.document_id)
        && response.content_revision <= MAX_SAFE_REVISION;
    let disposition_is_valid = match (response.ok, &response.result, &response.error) {
        (true, Some(result), None) => {
            result.is_object()
                && crate::native_contract::validate_result(
                    &request.operation,
                    &request.document_id,
                    &request.payload,
                    result,
                )
        }
        (false, None, Some(error)) => {
            !error.code.is_empty()
                && !error.message.is_empty()
                && error
                    .details
                    .as_ref()
                    .is_none_or(serde_json::Value::is_object)
        }
        _ => false,
    };
    if identity_is_valid && disposition_is_valid {
        Ok(())
    } else {
        Err(io::Error::other("invalid native application response"))
    }
}

pub async fn native_status(
    endpoint: &Endpoint,
    request: NativeStatusRequest,
    cancel: CancellationToken,
) -> io::Result<NativeHostStatus> {
    request.validate().map_err(io::Error::other)?;
    let request_id = request.request_id.clone();
    let round_trip = async {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, endpoint.port)).await?;
        write_json(
            &mut stream,
            &NativeStatusWireRequest {
                secret: endpoint.secret.clone(),
                native_status: request,
            },
        )
        .await?;
        let response: NativeHostStatus =
            read_json_bounded(stream, NATIVE_MAX_MESSAGE_BYTES).await?;
        response.validate().map_err(io::Error::other)?;
        if response.request_id != request_id || response.instance_id != endpoint.instance_id {
            return Err(io::Error::other("native status identity mismatch"));
        }
        Ok(response)
    };
    cancellable(round_trip, cancel).await
}

async fn cancellable<T>(
    round_trip: impl std::future::Future<Output = io::Result<T>>,
    cancel: CancellationToken,
) -> io::Result<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted,
            "request cancelled; query state before retrying a write")),
        result = tokio::time::timeout(Duration::from_secs(30), round_trip) =>
            result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut,
                "application timeout; query state before retrying a write"))?,
    }
}
