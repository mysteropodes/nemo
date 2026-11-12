use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

const BLUE: &str = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
const RED: &str = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000e49444154789c63f8cfc0f01f040110f803fd4e95c16f0000000049454e44ae426082";
fn png(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "desktop",
        &serde_json::to_vec(&json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":21,"layers":[{"layerUid":"drawing"}],"objects":[]},
        "viewportReference":null,"assets":[]}))
        .unwrap(),
    )
    .unwrap()
}
fn request(owner: &NativeObjectHistory, id: &str, action: &str, payload: Value) -> OpacityRequest {
    OpacityRequest::history_stage(
        id,
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        format!("command.document.reference.{action}"),
        payload,
    )
}
fn attach(owner: &mut NativeObjectHistory, id: &str, name: &str, hex: &str) -> Value {
    let request = request(owner, id, "attach", json!({"id":"ref","name":name}));
    let response = owner
        .dispatch_reference_command(&request, Some(&png(hex)))
        .unwrap();
    assert!(response.is_ok());
    owner.project_file_info().unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    let view = owner.project_file_info().unwrap();
    owner
        .serialize_project_file(
            owner.content_revision(),
            view["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
}

#[test]
fn actual_owner_default_settings_replace_history_and_reopen_have_one_digest() {
    let mut owner = owner();
    let empty = owner.project_file_info().unwrap();
    assert!(owner.is_project_file());
    let blue = attach(&mut owner, "first", "blue.png", BLUE);
    assert_eq!(
        blue["reference"],
        json!({"id":"ref","name":"blue.png","visible":true,"opacity":0.5,"offsetFrames":0})
    );
    assert_ne!(empty["documentDigest"], blue["documentDigest"]);
    assert_eq!(
        blue["byteCount"].as_u64().unwrap() as usize,
        bytes(&owner).len()
    );
    let settings = request(
        &owner,
        "settings",
        "settings",
        json!({"name":"named.png","visible":false,"opacity":0.25,"offsetFrames":-5}),
    );
    assert!(owner
        .dispatch_reference_command(&settings, None)
        .unwrap()
        .is_ok());
    let changed = owner.project_file_info().unwrap();
    let red = attach(&mut owner, "replace", "red.png", RED);
    assert_eq!(
        red["reference"],
        json!({"id":"ref","name":"red.png","visible":false,"opacity":0.25,"offsetFrames":-5})
    );
    assert_eq!(owner.undo(3), Ok(4));
    assert_eq!(
        owner.project_file_info().unwrap()["documentDigest"],
        changed["documentDigest"]
    );
    assert_eq!(owner.redo(4), Ok(5));
    assert_eq!(
        owner.project_file_info().unwrap()["documentDigest"],
        red["documentDigest"]
    );
    let saved = bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved).unwrap();
    assert_ne!(owner.document_id(), reopened.document_id());
    let view = reopened.project_file_info().unwrap();
    assert_eq!(view["reference"], red["reference"]);
    assert_eq!(view["documentDigest"], red["documentDigest"]);
    assert_eq!(view["layers"], json!([{"layerUid":"drawing","kind":"draw"}]));
    assert_eq!(view["totalFrames"], 21);
    assert_eq!(view["undoDepth"], 0);
    assert_eq!(bytes(&reopened), saved);
}

#[test]
fn exact_retry_is_retained_after_later_history_and_changed_bytes_or_body_refuse() {
    let mut owner = owner();
    let request = request(
        &owner,
        "once",
        "attach",
        json!({"id":"ref","name":"blue.png"}),
    );
    let response = owner
        .dispatch_reference_command(&request, Some(&png(BLUE)))
        .unwrap();
    assert_eq!(response.content_revision(), 1);
    owner.undo(1).unwrap();
    assert_eq!(
        owner
            .dispatch_reference_command(&request, Some(&png(BLUE)))
            .unwrap(),
        response
    );
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
    assert!(owner.project_file_info().unwrap()["reference"].is_null());
    assert_eq!(
        owner.dispatch_reference_command(&request, Some(&png(RED))),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut changed = request.clone();
    changed.payload["name"] = "other.png".into();
    assert_eq!(
        owner.dispatch_reference_command(&changed, Some(&png(BLUE))),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn cancellation_stale_and_invalid_png_results_never_change_or_resurrect_history() {
    let mut owner = owner();
    let cancelled = request(
        &owner,
        "cancel",
        "attach",
        json!({"id":"ref","name":"blue.png"}),
    )
    .cancelled();
    let bad = request(
        &owner,
        "bad",
        "attach",
        json!({"id":"ref","name":"bad.png"}),
    );
    let stale = request(&owner, "stale", "remove", json!({}));
    let original = bytes(&owner);
    let cancel_result = owner
        .dispatch_reference_command(&cancelled, Some(&png(BLUE)))
        .unwrap();
    assert_eq!(
        cancel_result.error().unwrap().code(),
        DispatchErrorCode::CancelledBeforeDispatch
    );
    let bad_result = owner
        .dispatch_reference_command(&bad, Some(b"not a PNG"))
        .unwrap();
    assert!(!bad_result.is_ok());
    assert_eq!(bytes(&owner), original);
    attach(&mut owner, "good", "blue.png", BLUE);
    let stale_result = owner.dispatch_reference_command(&stale, None).unwrap();
    assert_eq!(
        stale_result.error().unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        owner
            .dispatch_reference_command(&cancelled, Some(&png(BLUE)))
            .unwrap(),
        cancel_result
    );
    assert_eq!(
        owner
            .dispatch_reference_command(&bad, Some(b"not a PNG"))
            .unwrap(),
        bad_result
    );
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn no_op_preserves_redo_and_payload_refusals_leave_saved_bytes_unchanged() {
    let mut owner = owner();
    attach(&mut owner, "blue", "blue.png", BLUE);
    attach(&mut owner, "red", "red.png", RED);
    owner.undo(2).unwrap();
    let saved = bytes(&owner);
    let no_op = request(
        &owner,
        "same",
        "attach",
        json!({"id":"ref","name":"blue.png"}),
    );
    let result = owner
        .dispatch_reference_command(&no_op, Some(&png(BLUE)))
        .unwrap();
    assert_eq!(
        result.result().unwrap(),
        &json!({"applied":false,"historyEntriesAdded":0})
    );
    for (i, (action, payload)) in [
        (
            "settings",
            json!({"name":"x","visible":true,"opacity":2,"offsetFrames":0}),
        ),
        (
            "settings",
            json!({"name":"../x","visible":true,"opacity":0.5,"offsetFrames":0}),
        ),
        (
            "settings",
            json!({"name":"x","visible":true,"opacity":0.5,"offsetFrames":0,"extra":true}),
        ),
        ("remove", json!({"extra":true})),
    ]
    .into_iter()
    .enumerate()
    {
        let request = request(&owner, &format!("invalid-{i}"), action, payload);
        assert!(!owner
            .dispatch_reference_command(&request, None)
            .unwrap()
            .is_ok());
        assert_eq!(bytes(&owner), saved);
        assert_eq!(owner.history_depths(), (1, 1));
    }
    assert_eq!(owner.redo(3), Ok(4));
    let remove = request(&owner, "remove", "remove", json!({}));
    assert!(owner
        .dispatch_reference_command(&remove, None)
        .unwrap()
        .is_ok());
    assert!(owner.project_file_info().unwrap()["reference"].is_null());
}

#[test]
fn wrong_identity_and_mismatched_binary_input_never_enter_the_owner() {
    let mut owner = owner();
    let before = bytes(&owner);
    let other = NativeObjectHistory::from_project_file("foreign", &before).unwrap();
    let req = request(
        &other,
        "wrong",
        "attach",
        json!({"id":"ref","name":"blue.png"}),
    );
    assert_eq!(
        owner.dispatch_reference_command(&req, Some(&png(BLUE))),
        Err(DispatchErrorCode::WrongInstance)
    );
    let other = NativeObjectHistory::from_project_file("desktop", &before).unwrap();
    let req = request(
        &other,
        "wrong-doc",
        "attach",
        json!({"id":"ref","name":"blue.png"}),
    );
    assert_eq!(
        owner.dispatch_reference_command(&req, Some(&png(BLUE))),
        Err(DispatchErrorCode::WrongDocument)
    );
    let req = request(
        &owner,
        "missing",
        "attach",
        json!({"id":"ref","name":"blue.png"}),
    );
    assert_eq!(
        owner.dispatch_reference_command(&req, None),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let req = request(&owner, "extra-bytes", "remove", json!({}));
    assert_eq!(
        owner.dispatch_reference_command(&req, Some(&png(BLUE))),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
}
