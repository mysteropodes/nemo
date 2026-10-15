//! Bounded local IPC; no shell, eval, document mirror, or public network listener.
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

// Serde's derived struct reader also accepts positional arrays. Native v2
// authentication and dispatch require named object members at both levels.
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

/// Guard the SDK's input before it deserializes nested arguments into Value.
/// Original CR/BOM bytes count toward the existing v1 outer framing budget.
pub fn guarded_stdio_input<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
) -> impl AsyncRead + Unpin + Send + 'static {
    raw_envelope::GuardedInput::new(reader, MAX_MESSAGE_BYTES - 2)
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
    decode_json_bounded(&bytes, limit)
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
    Ok(serde_json::from_slice(bytes)?)
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
                && (request.operation != "query.reproduction.report"
                    || result
                        .get("verifiedContentRevision")
                        .and_then(serde_json::Value::as_u64)
                        == Some(response.content_revision))
                && (request.operation != "query.document.object"
                    || result.get("atRevision").and_then(serde_json::Value::as_u64)
                        == Some(response.content_revision))
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
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted,
            "request cancelled; query state before retrying a write")),
        result = tokio::time::timeout(Duration::from_secs(30), round_trip) =>
            result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut,
                "application timeout; query state before retrying a write"))?,
    }
}

#[cfg(test)]
mod object_tests {
    use super::*;
    use serde_json::{json, Value};

    fn pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
        let cases: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let mut request = cases["read"]["request"].clone();
        request["payload"]["atRevision"] = json!(0);
        let response = json!({"apiVersion":2,"requestId":request["requestId"],
            "instanceId":request["instanceId"],"documentId":request["documentId"],
            "contentRevision":0,"ok":true,"result":{"atRevision":0,
            "documentSnapshotId":format!("native-object:{}:0",request["documentId"].as_str().unwrap()),
            "object":cases["records"][0]}});
        (
            serde_json::from_value(request).unwrap(),
            serde_json::from_value(response).unwrap(),
        )
    }

    #[tokio::test]
    async fn authenticated_native_envelopes_require_named_objects_at_both_levels() {
        async fn accepts(value: Value) -> bool {
            let mut bytes = serde_json::to_vec(&value).unwrap();
            bytes.push(b'\n');
            read_json::<AuthenticatedWireRequest>(bytes.as_slice())
                .await
                .is_ok()
        }
        let (mut request, _) = pair();
        request.expected_revision = Some(0);
        let native = serde_json::to_value(&request).unwrap();
        let positional = json!([
            request.api_version,
            request.request_id,
            request.instance_id,
            request.document_id,
            0,
            request.operation,
            request.payload,
            false
        ]);
        assert!(serde_json::from_value::<NativeApplicationRequest>(positional.clone()).is_ok());
        assert!(accepts(json!({"secret":"test","nativeRequest":native.clone()})).await);
        assert!(!accepts(json!(["test", native])).await);
        assert!(!accepts(json!({"secret":"test","nativeRequest":positional.clone()})).await);
        assert!(!accepts(json!(["test", positional])).await);
        let status = json!({"apiVersion":2,"requestId":"status-1","instanceId":"instance"});
        let status_seq = json!([2, "status-1", "instance"]);
        assert!(serde_json::from_value::<NativeStatusRequest>(status_seq.clone()).is_ok());
        assert!(accepts(json!({"secret":"test","nativeStatus":status.clone()})).await);
        assert!(!accepts(json!(["test", status])).await);
        assert!(!accepts(json!({"secret":"test","nativeStatus":status_seq})).await);
    }

    #[test]
    fn object_response_requires_the_complete_correlated_immutable_result() {
        let (request, response) = pair();
        assert!(validate_native_response(&response, &request).is_ok());
        let mut later = response.clone();
        later.content_revision = 1;
        assert!(validate_native_response(&later, &request).is_err());
        let mut wrong = response.clone();
        wrong.result.as_mut().unwrap()["object"]["target"]["strokeId"] = json!("replacement");
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut extra = response.clone();
        extra.result.as_mut().unwrap()["truncated"] = json!(true);
        assert!(validate_native_response(&extra, &request).is_err());
        let mut missing = response;
        missing
            .result
            .as_mut()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("object");
        assert!(validate_native_response(&missing, &request).is_err());
    }

    #[tokio::test]
    async fn available_object_call_uses_authenticated_native_endpoint() {
        let (request, response) = pair();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: request.instance_id.clone(),
            port: listener.local_addr().unwrap().port(),
            secret: "test".into(),
            build_id: "test".into(),
        };
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let message: NativeWireRequest = read_json(&mut stream).await.unwrap();
            assert_eq!(message.secret, endpoint.secret);
            assert_eq!(message.native_request, request);
            write_json(&mut stream, &response).await.unwrap();
        };
        let client = call_native(&endpoint, request.clone(), CancellationToken::new());
        let (_, actual) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(server, client)
        })
        .await
        .unwrap();
        assert_eq!(actual.unwrap(), response);
    }

    #[tokio::test]
    async fn object_reader_preserves_complete_bytes_and_rejects_oversized_envelopes() {
        let (request, response) = pair();
        let original = serde_json::to_vec(&response).unwrap();
        assert!(original.len() < NATIVE_MAX_MESSAGE_BYTES);
        let mut bytes = original;
        bytes.push(b'\n');
        let received: NativeApplicationResponse =
            read_json_bounded(bytes.as_slice(), NATIVE_MAX_MESSAGE_BYTES)
                .await
                .unwrap();
        assert_eq!(received, response);
        assert!(validate_native_response(&received, &request).is_ok());
        let mut too_large = response.clone();
        let segments = &mut too_large.result.as_mut().unwrap()["object"]["geometry"]["segments"];
        *segments = Value::Array(vec![segments[0].clone(); 256]);
        // Still a complete schema-valid record; encoded transport refuses it.
        assert!(validate_native_response(&too_large, &request).is_ok());
        let before = too_large.clone();
        let mut bytes = serde_json::to_vec(&too_large).unwrap();
        assert!(bytes.len() > NATIVE_MAX_MESSAGE_BYTES);
        bytes.push(b'\n');
        assert!(read_json_bounded::<NativeApplicationResponse>(
            bytes.as_slice(),
            NATIVE_MAX_MESSAGE_BYTES
        )
        .await
        .is_err());
        assert_eq!(too_large, before);
    }
}
