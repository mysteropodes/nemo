//! Discovery reports availability per document kind from the capability declarations;
//! these tests hold the real object owner to that report (#1049).
use crate::application_mcp::ApplicationMcp;
use nemo_mcp::{
    contract::{NativeApplicationRequest, NativeStatusRequest},
    discovery_catalog::{self as catalog, Applicability},
};
use serde_json::{json, Value};

fn open_object_project() -> (ApplicationMcp, String) {
    let state = ApplicationMcp::default();
    let (document, revision, _) = state.install_native_blank_project(640, 360, 24.0).unwrap();
    assert_eq!(revision, 0);
    (state, document)
}

/// The route every MCP request takes: the shared application route, not one owner, with
/// file sources admitted as the desktop does once the window's scope allows them.
async fn dispatch(state: &ApplicationMcp, request: Value) -> Result<Value, String> {
    let request: NativeApplicationRequest = serde_json::from_value(request).unwrap();
    let response = state.dispatch_native_scoped(request, std::sync::Arc::new(|_: &std::path::Path| true)).await;
    response.map(|response| serde_json::to_value(response).unwrap())
}

fn revision(state: &ApplicationMcp) -> u64 {
    let native = state.native_state();
    let authority = native.lock().unwrap();
    authority.active().unwrap().1.content_revision()
}

/// Payloads for operations whose declaration carries no example and for which `{}` is not
/// a valid request; each is the smallest payload their contract admits.
fn declared_payload(operation: &str, document: &str) -> Option<Value> {
    let pin = format!("native-object:{document}:0");
    Some(match operation {
        "command.document.object.asset.attach" => json!({"documentSnapshotId": pin, "lifecycleGeneration": 1,
            "name": "still.png", "mediaType": "image/png", "source": {"kind": "file", "path": "/picked.png"},
            "placement": {"kind": "newLayer"}}),
        "command.document.object.asset.video.link" => json!({"documentSnapshotId": pin, "lifecycleGeneration": 1,
            "source": {"kind": "file", "path": "/admitted/clip.mov"}, "intent": {"kind": "newLayer"}}),
        "command.document.object.asset.font.import" => json!({"command": "object.asset.font.import",
            "documentSnapshotId": pin, "lifecycleGeneration": 1,
            "source": {"kind": "google", "family": "Roboto", "weight": 700, "italic": true}}),
        "query.document.object.asset.font" => json!({"action": "list", "atRevision": 0,
            "documentSnapshotId": pin, "offset": 0, "limit": 1}),
        _ => return None,
    })
}

/// A request the shared route admits for `operation`: its declared payload, else the
/// declaration's own example, else `{}`; revision only where the declaration shows one.
async fn probe(state: &ApplicationMcp, document: &str, index: usize, operation: &str) -> Result<Value, String> {
    let example = catalog::descriptor(operation).and_then(|d| {
        d["examples"].as_array()?.iter().find(|e| e["request"]["operation"] == operation).map(|e| e["request"].clone())
    });
    let mut request = example.unwrap_or_else(|| json!({"payload": {}}));
    if let Some(payload) = declared_payload(operation, document) {
        request["payload"] = payload;
    }
    request["apiVersion"] = json!(2);
    request["requestId"] = json!(format!("probe-{index}"));
    request["instanceId"] = json!(state.instance_id());
    request["documentId"] = json!(document);
    request["operation"] = json!(operation);
    request.as_object_mut().unwrap().remove("expectedRevision");
    if operation.starts_with("command.") && !operation.starts_with("command.reproduction") {
        request["expectedRevision"] = json!(revision(state));
    }
    dispatch(state, request).await
}

/// The owner's own answer to `{}`, for operations the shared route's input check rejects.
fn owner_probe(state: &ApplicationMcp, document: &str, operation: &str) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let mut request = json!({"apiVersion": 2, "requestId": "owner-probe", "instanceId": state.instance_id(),
        "documentId": document, "operation": operation, "payload": {}});
    if operation.starts_with("command.") {
        request["expectedRevision"] = json!(authority.active().unwrap().1.content_revision());
    }
    let request = serde_json::from_value(request).unwrap();
    serde_json::to_value(authority.active_mut(generation).unwrap().dispatch(request).unwrap()).unwrap()
}

/// The two refusals an object document gives an operation it does not own: the read
/// route's own, and the command that reaches the read route and is refused as a read.
fn refused_as_unsupported(response: &Value) -> bool {
    let message = response["error"]["message"].as_str().unwrap_or_default();
    response["ok"] == false
        && (message.starts_with("Unsupported object read operation.")
            || message == "Only pinned object reads without expectedRevision are supported.")
}

#[tokio::test]
async fn every_operation_discovery_reports_available_for_an_object_document_is_not_refused_as_unsupported() {
    let (state, document) = open_object_project();
    let available = catalog::available_native_operations("object");
    assert!(available.len() > 100, "only {} operations reported", available.len());
    let (mut refused, mut owner_checked) = (Vec::new(), 0);
    for (index, operation) in available.iter().enumerate() {
        let response = match probe(&state, &document, index, operation).await {
            Ok(response) => response,
            // The shared route's input check refused the smallest payload before any owner
            // saw it; the owner is then asked directly, so no operation goes unprobed.
            Err(_) => {
                owner_checked += 1;
                owner_probe(&state, &document, operation)
            }
        };
        if refused_as_unsupported(&response) {
            refused.push(format!("{operation}: {}", response["error"]["message"]));
        }
    }
    eprintln!("{} of {} operations were probed on the owner directly", owner_checked, available.len());
    assert!(refused.is_empty(), "reported available yet refused as unsupported:\n{}", refused.join("\n"));
    assert!(owner_checked <= 12, "too many operations never reached the shared route: {owner_checked}");
}

#[test]
fn an_operation_declared_for_another_kind_is_refused_and_names_the_one_to_use() {
    let (state, document) = open_object_project();
    let (mut checked, mut named) = (0, 0);
    for operation in catalog::all_native_operations() {
        let Applicability::OtherKind { use_instead } = catalog::native_applicability("object", operation) else {
            continue;
        };
        let response = owner_probe(&state, &document, operation);
        assert!(refused_as_unsupported(&response), "{operation}: {response}");
        assert_eq!(response["error"]["code"], "invalid_request", "{operation}");
        if let Some(other) = use_instead {
            let message = response["error"]["message"].as_str().unwrap();
            assert!(message.contains(&format!("Use {other} instead.")), "{operation}: {message}");
            named += 1;
        }
        checked += 1;
    }
    assert!(named >= 16 && checked > named, "{named} named of {checked} checked");
}

#[test]
fn the_engine_replacement_table_equals_the_declared_one() {
    let declared: std::collections::BTreeMap<String, String> = catalog::all_native_operations()
        .into_iter()
        .filter_map(|operation| catalog::use_instead("object", operation).map(|other| (operation.to_owned(), other)))
        .collect();
    let engine: std::collections::BTreeMap<String, String> = native_engine::object_snapshot::OPACITY_DOCUMENT_ALTERNATIVES
        .iter()
        .map(|(operation, other)| ((*operation).to_owned(), (*other).to_owned()))
        .collect();
    assert_eq!(engine, declared);
}

#[test]
fn native_status_reports_the_document_kind_and_the_process() {
    let (state, document) = open_object_project();
    let request = NativeStatusRequest { api_version: 2, request_id: "status-kind".into(), instance_id: state.instance_id().into() };
    let status = state.native_status(request).unwrap();
    assert!(status.available);
    assert_eq!(status.document_id.as_deref(), Some(document.as_str()));
    assert_eq!(status.document_kind.as_deref(), Some("object"));
    assert_eq!(status.pid, Some(std::process::id()));
}
