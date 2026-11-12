//! Bounded local IPC; no shell, eval, document mirror, or public network listener.
#[path = "native_clipboard_transport.rs"]
pub(crate) mod native_clipboard_transport;
#[path = "native_request_bytes.rs"]
mod native_request_bytes;
#[path = "raw_envelope.rs"]
mod raw_envelope;
#[path = "stdio_transport.rs"]
mod stdio_transport;
use crate::{
    contract::{
        bounded_identifier, ApplicationRequest, ApplicationResponse, NativeApplicationRequest,
        NativeApplicationResponse, NativeHostStatus, NativeStatusRequest, MAX_MESSAGE_BYTES,
        MAX_SAFE_REVISION, NATIVE_API_VERSION, NATIVE_MAX_MESSAGE_BYTES,
    },
    registry::Endpoint,
};
use serde::{
    de::{value::MapAccessDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{fmt, io, marker::PhantomData, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
};
pub use tokio_util::sync::CancellationToken;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireRequest {
    pub secret: String,
    pub request: ApplicationRequest,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeWireRequest {
    pub secret: String,
    pub native_request: NativeApplicationRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeWireRequestFields {
    secret: String,
    #[serde(deserialize_with = "deserialize_object_only")]
    native_request: NativeApplicationRequest,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeStatusWireRequest {
    pub secret: String,
    pub native_status: NativeStatusRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeStatusWireRequestFields {
    secret: String,
    #[serde(deserialize_with = "deserialize_object_only")]
    native_status: NativeStatusRequest,
}

// Serde accepts positional structs; native v2 requires object maps at both levels.
fn deserialize_object_only<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectOnly<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectOnly<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectOnly(PhantomData))
}

impl<'de> Deserialize<'de> for NativeWireRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields: NativeWireRequestFields = deserialize_object_only(deserializer)?;
        Ok(Self {
            secret: fields.secret,
            native_request: fields.native_request,
        })
    }
}

impl<'de> Deserialize<'de> for NativeStatusWireRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields: NativeStatusWireRequestFields = deserialize_object_only(deserializer)?;
        Ok(Self {
            secret: fields.secret,
            native_status: fields.native_status,
        })
    }
}

/// Host parser: legacy `request` is unchanged; v2 application/status use distinct keys.
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
    // Preserve v1: content + newline must be strictly smaller than MAX_MESSAGE_BYTES.
    read_json_bounded(reader, MAX_MESSAGE_BYTES - 2).await
}

/// Guard the SDK's input before it deserializes nested arguments into Value.
/// Original CR/BOM bytes count toward the existing v1 outer framing budget.
pub fn guarded_stdio_input<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
) -> impl AsyncRead + Unpin + Send + 'static {
    raw_envelope::GuardedInput::new(reader, native_clipboard_transport::FRAME_BYTES)
}

/// Production stdio path: SDK codec and one cancellation-safe output owner.
pub fn guarded_stdio_transport<R, W>(
    reader: R,
    writer: W,
) -> impl rmcp::transport::Transport<rmcp::RoleServer, Error = io::Error>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    stdio_transport::new(reader, writer)
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
    bytes.pop(); // LF is framing, not part of the encoded JSON envelope.
    decode_json_bounded(&bytes, max_content_bytes)
}

/// Decode complete original bytes with a caller's bound and recursive duplicate
/// rejection before typed/Value deserialization. This grants no availability.
pub fn decode_json_bounded<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    max_bytes: usize,
) -> io::Result<T> {
    if bytes.len() > max_bytes {
        return Err(io::Error::other("message exceeds size limit"));
    }
    raw_envelope::check_unique(bytes).map_err(|error| match error {
        raw_envelope::RawEnvelopeError::Duplicate => {
            io::Error::new(io::ErrorKind::InvalidData, "duplicate JSON member")
        }
        raw_envelope::RawEnvelopeError::Malformed(error) => io::Error::from(error),
    })?;
    native_request_bytes::check(bytes)?;
    Ok(serde_json::from_slice(bytes)?)
}

/// Full original/typed operation budget; unrelated requests/replies stay4096.
pub fn native_request_byte_limit(operation: &str) -> usize {
    native_request_bytes::request_limit(operation)
}
pub use native_clipboard_transport::{
    native_response_byte_limit, read_authenticated, write_native_response,
};
/// Original-byte, duplicate-first, map-only admission; not execution availability.
pub fn decode_native_request(bytes: &[u8]) -> io::Result<NativeApplicationRequest> {
    let request: NativeApplicationRequest =
        decode_json_bounded(bytes, native_clipboard_transport::MESSAGE_BYTES)?;
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(io::Error::other("native request must be an object"));
    }
    request.validate_input().map_err(io::Error::other)?;
    Ok(request)
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
    cancellable(round_trip, cancel).await
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
        native_clipboard_transport::write_request(
            &mut stream,
            &NativeWireRequest {
                secret: endpoint.secret.clone(),
                native_request: request,
            },
        )
        .await?;
        let response: NativeApplicationResponse = read_json_bounded(
            stream,
            native_response_byte_limit(&response_contract.operation),
        )
        .await?;
        validate_native_response(&response, &response_contract)?;
        let reports_replacement = response_owner::reports_document(&response_contract, &response);
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

pub fn validate_native_response(
    response: &NativeApplicationResponse,
    request: &NativeApplicationRequest,
) -> io::Result<()> {
    if !serde_json::to_vec(response)
        .is_ok_and(|bytes| bytes.len() <= native_response_byte_limit(&request.operation))
    {
        return Err(io::Error::other(
            "native response exceeds operation encoded byte bound",
        ));
    }
    let policy_command = request.operation == crate::native_contract::layer_policy::OP;
    let track_command = request.operation == crate::native_contract::property_track::OP;
    let property_edit = request.operation == crate::native_contract::property_edit::OP;
    let property_read = request.operation == crate::native_contract::property_read::OP;
    let frame_edit_command = crate::native_contract::frame_edit::declares(&request.operation);
    let correlated_identity = response_owner::correlated(request, response);
    let identity_is_valid = correlated_identity
        && response.api_version == NATIVE_API_VERSION
        && bounded_identifier(&response.request_id)
        && bounded_identifier(&response.instance_id)
        && bounded_identifier(&response.document_id)
        && response.content_revision <= MAX_SAFE_REVISION;
    let disposition_is_valid = match (response.ok, &response.result, &response.error) {
        (true, Some(result), None) => {
            result.is_object()
                && (request.operation != "query.reproduction.report"
                    || result
                        .get("verifiedContentRevision")
                        .and_then(serde_json::Value::as_u64)
                        == Some(response.content_revision))
                && response_owner::pinned_read_correlated(request, response, result)
                && (!matches!(
                    request.operation.as_str(),
                    "command.document.object.layer.create"
                        | "command.document.object.undo"
                        | "command.document.object.redo"
                ) || request
                    .expected_revision
                    .and_then(|revision| revision.checked_add(1))
                    == Some(response.content_revision))
                && (!matches!(
                    request.operation.as_str(),
                    "command.document.object.project.settings.edit"
                        | crate::native_contract::composition::EDIT
                        | crate::native_contract::palette::EDIT
                        | crate::native_contract::guide::EDIT
                        | crate::native_contract::shadow::EDIT
                        | crate::native_contract::perspective::EDIT
                        | crate::native_contract::settings::symmetry::EDIT
                        | crate::native_contract::workspace::EDIT
                        | crate::native_contract::custom_brush::EDIT
                        | crate::native_contract::comments::EDIT
                ) || crate::native_contract::settings::revision_matches(
                    request.expected_revision,
                    response.content_revision,
                    result,
                ))
                && (!policy_command
                    || crate::native_contract::layer_policy::revision_matches(
                        request.expected_revision,
                        response.content_revision,
                        result,
                    ))
                && (!property_edit
                    || crate::native_contract::property_edit::revision_matches(
                        request.expected_revision,
                        response.content_revision,
                        result,
                    ))
                && (!track_command
                    || crate::native_contract::property_track::revision_matches(
                        request.expected_revision,
                        response.content_revision,
                        result,
                    ))
                && (!frame_edit_command
                    || crate::native_contract::frame_edit::revision_matches_for_operation(
                        &request.operation,
                        request.expected_revision,
                        response.content_revision,
                        result,
                    ))
                && (!frame_edit_command
                    || crate::native_contract::frame_edit::clipboard_identity_matches(
                        &request.operation,
                        &request.instance_id,
                        &request.document_id,
                        result,
                    ))
                && crate::native_contract::font::response_matches(request, response)
                && crate::native_contract::object_edit::response_matches(request, response)
                && crate::native_contract::object_rig::response_matches(request, response)
                && crate::native_contract::object_component::response_matches(request, response)
                && crate::native_contract::import::response_matches(request, response)
                && crate::native_contract::item::response_matches(request, response)
                && crate::native_contract::vector::response_matches(request, response)
                && crate::native_contract::brush::response_matches(request, response)
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

#[cfg(test)]
mod report_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn report_rejects_matching_result_token_in_a_mismatched_revision_envelope() {
        let request = NativeApplicationRequest {
            api_version: 2,
            request_id: "report".into(),
            instance_id: "instance".into(),
            document_id: "document".into(),
            operation: "query.reproduction.report".into(),
            payload: json!({"expectedContentRevision":1,"expectedSequence":1}),
            expected_revision: None,
            cancelled_before_dispatch: false,
        };
        let mut response = NativeApplicationResponse {
            api_version: 2,
            request_id: "report".into(),
            instance_id: "instance".into(),
            document_id: "document".into(),
            content_revision: 1,
            ok: true,
            error: None,
            result: Some(
                json!({"verifiedContentRevision":1,"verifiedSequence":1,"bundle":{
                    "format":"nemo.native-opacity-reproduction","formatVersion":1,"apiVersion":2,
                    "fixture":{"id":"native-opacity-static","version":1,"sha256":"a".repeat(64)},
                    "command":"layer.opacity.set","stableTarget":{"layerUid":"r08_curve_layer"},
                    "clock":null,"seed":null,"versions":{"nativeEngine":"0.1.0"},
                    "commands":[{"id":1,"expectedRevision":0,"value":40,"revision":1,"applied":true}]
                }}),
            ),
        };
        assert!(validate_native_response(&response, &request).is_ok());
        response.content_revision = 2;
        assert!(validate_native_response(&response, &request).is_err());
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
    cancellable_for(round_trip, cancel, Duration::from_secs(30)).await
}

// Private duration seam: all production callers retain the same 30-second bound.
async fn cancellable_for<T>(
    round_trip: impl std::future::Future<Output = io::Result<T>>,
    cancel: CancellationToken,
    timeout: Duration,
) -> io::Result<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted,
            "request cancelled; query state before retrying a write")),
        result = tokio::time::timeout(timeout, round_trip) =>
            result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut,
                "application timeout; query state before retrying a write"))?,
    }
}

#[cfg(test)]
#[path = "native_object_wire_tests.rs"]
mod object_tests;

#[cfg(test)]
#[path = "native_project_settings_wire_tests.rs"]
mod project_settings_tests;

#[cfg(test)]
#[path = "native_property_metadata_wire_tests.rs"]
mod property_metadata_tests;

#[cfg(test)]
#[path = "wire_failure_tests.rs"]
mod failure_tests;

#[cfg(test)]
#[path = "native_layer_policy_wire_tests.rs"]
mod layer_policy_tests;

#[cfg(test)]
#[path = "native_property_track_wire_tests.rs"]
mod property_track_tests;

#[cfg(test)]
#[path = "native_property_read_wire_tests.rs"]
mod property_read_tests;

#[cfg(test)]
#[path = "native_property_edit_wire_tests.rs"]
mod property_edit_tests;

#[cfg(test)]
#[path = "native_text_read_wire_tests.rs"]
mod text_read_tests;

#[cfg(test)]
#[path = "native_image_import_owner_wire_tests.rs"]
mod image_owner_tests;
#[path = "native_wire_response_owner.rs"]
mod response_owner;
