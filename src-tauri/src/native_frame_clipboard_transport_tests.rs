//! Real ApplicationMcp active object authority; no app/window/GPU or fake document sink.
use super::ApplicationMcp;
use native_engine::object_codec::decode_storage_project;
use nemo_mcp::{
    contract::{NativeApplicationRequest, NativeApplicationResponse},
    wire,
};
use serde_json::{json, Value};
const COPY: &str = "command.document.object.frames.clipboard.copy";
const CUT: &str = "command.document.object.frames.clipboard.cut";
const PASTE: &str = "command.document.object.frames.clipboard.paste";
fn owner() -> (ApplicationMcp, String) {
    let state = ApplicationMcp::default();
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let records: Vec<Value> = (0..4100)
        .map(|index| {
            let mut record = cases["records"][0].clone();
            record["target"] = json!({
                "contextId": "scene-root",
                "frameScope": {"kind": "authored", "frame": 0},
                "layerUid": "L0",
                "strokeId": format!("record-{index}")
            });
            record
        })
        .collect();
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,
            "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":records});
    let document = decode_storage_project(&serde_json::to_vec(&drawing).unwrap()).unwrap();
    let (document, _) = state.install_native_object(document).unwrap();
    (state, document)
}
fn q(
    state: &ApplicationMcp,
    document: &str,
    id: &str,
    revision: u64,
    operation: &str,
    payload: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":document,"expectedRevision":revision,"operation":operation,"payload":payload}),
    )
    .unwrap()
}
fn selection(command: &str) -> Value {
    json!({"command":command,"selection":[{"layerUid":"L0","frame":0}]})
}
fn send(state: &ApplicationMcp, request: &NativeApplicationRequest) -> NativeApplicationResponse {
    let response = state.dispatch_native(request.clone()).unwrap();
    // Same production common validator used by the Tauri command and TCP reply.
    wire::validate_native_response(&response, request).unwrap();
    response
}
#[test]
fn active_host_public_route_pairs_large_copy_cut_paste_and_retained_receipts() {
    let (state, document) = owner();
    let copy = q(
        &state,
        &document,
        "host-copy",
        0,
        COPY,
        selection("object.frames.clipboard.copy"),
    );
    let copied = send(&state, &copy);
    assert!(copied.ok);
    assert_eq!(copied.content_revision, 0);
    assert!(serde_json::to_vec(&copied).unwrap().len() > 1_048_576);
    let packet = copied.result.as_ref().unwrap()["clipboard"].clone();
    assert_eq!(
        packet["cells"][0]["records"].as_array().unwrap().len(),
        4100
    );
    let cut = q(
        &state,
        &document,
        "host-cut",
        0,
        CUT,
        selection("object.frames.clipboard.cut"),
    );
    let cut_reply = send(&state, &cut);
    assert!(cut_reply.ok);
    assert_eq!(cut_reply.content_revision, 1);
    assert_eq!(cut_reply.result.as_ref().unwrap()["clipboard"], packet);
    let paste = q(
        &state,
        &document,
        "host-paste",
        1,
        PASTE,
        json!({"command":"object.frames.clipboard.paste",
        "instanceId":packet["instanceId"],"documentId":packet["documentId"],"layerUid":"L0","frame":1,"cells":packet["cells"]}),
    );
    assert!(serde_json::to_vec(&paste).unwrap().len() > 1_048_576);
    let pasted = send(&state, &paste);
    assert!(pasted.ok);
    assert_eq!(pasted.content_revision, 2);
    assert_eq!(send(&state, &copy), copied);
    assert_eq!(send(&state, &cut), cut_reply);
    assert_eq!(send(&state, &paste), pasted);
    let mut changed = copy.clone();
    changed.payload["selection"][0]["frame"] = json!(1);
    let rejected = send(&state, &changed);
    assert!(!rejected.ok);
    assert_eq!(rejected.content_revision, 2);
}
#[test]
fn common_host_refusals_keep_revision_and_clipboard_source_correlated() {
    let (state, document) = owner();
    let q = q(
        &state,
        &document,
        "cancelled-host-copy",
        0,
        COPY,
        selection("object.frames.clipboard.copy"),
    );
    let mut cancelled = q.clone();
    cancelled.cancelled_before_dispatch = true;
    let refused = send(&state, &cancelled);
    assert!(!refused.ok);
    assert_eq!(refused.content_revision, 0);
    let mut normal = q.clone();
    normal.request_id = "host-normal-copy".into();
    let good = send(&state, &normal);
    let mut forged = good.clone();
    forged.result.as_mut().unwrap()["clipboard"]["documentId"] = json!("foreign");
    assert!(wire::validate_native_response(&forged, &normal).is_err());
    let mut malformed = normal;
    malformed.request_id = "host-malformed".into();
    malformed.payload["selection"] = Value::Null;
    assert!(state.dispatch_native(malformed).is_err());
}
