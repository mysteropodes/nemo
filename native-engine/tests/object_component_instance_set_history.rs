//! `object.component.instance.set` through the one history owner (#2256 Q5): one revision and one undo entry per real edit,
//! exact undo / redo bytes, no-ops that publish nothing, refusals that publish nothing, strict payloads.
use super::fixture::{decode, document, layer};
use super::*;
use crate::object_codec::encode_storage_project;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn base() -> Value {
    let layers = [
        layer("ly_inst", "Inst", &[]),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"sym_x","totalFrames":24,"layers":[{"layerUid":"xi"}],"objects":[]}],
        "instances":[{"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"once",
            "symSpeed":1,"symPlacedAt":0,"symSingleFrame":0}]});
    document(
        &layers,
        json!({"componentTables":{"components":components}}),
    )
}
fn set(extra: Value) -> Value {
    let mut payload = json!({"command":"object.component.instance.set","layerUid":"ly_inst"});
    for (key, value) in extra.as_object().expect("object") {
        payload[key] = value.clone();
    }
    payload
}
fn snapshot(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    encode_storage_project(snapshot.document()).unwrap()
}
fn send(
    owner: &mut NativeObjectHistory,
    payload: &Value,
) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
    owner.dispatch_component_setup("command.document.object.component.instance.set", payload)
}

#[test]
fn one_edit_is_one_revision_and_one_undo_entry_with_exact_undo_and_redo_bytes() {
    let mut owner = NativeObjectHistory::new("instance", decode(&base())).unwrap();
    let original = snapshot(&owner);
    let reply = send(
        &mut owner,
        &set(json!({"mode":"pingpong","speed":2,"placedAt":5,"singleFrame":3})),
    )
    .unwrap();
    assert_eq!(
        reply,
        CompactResult::Command {
            applied: true,
            history_entries_added: 1
        }
    );
    assert_eq!(owner.history_depths(), (1, 0));
    let after = snapshot(&owner);
    assert_ne!(after, original);
    owner.undo(1).unwrap();
    assert_eq!(snapshot(&owner), original);
    owner.redo(2).unwrap();
    assert_eq!(snapshot(&owner), after);
}

#[test]
fn an_edit_that_changes_nothing_adds_no_revision_and_keeps_the_redo_stack() {
    let mut owner = NativeObjectHistory::new("instance", decode(&base())).unwrap();
    send(&mut owner, &set(json!({"speed":4}))).unwrap();
    owner.undo(1).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let bytes = snapshot(&owner);
    let revision = owner.content_revision();
    let reply = send(
        &mut owner,
        &set(json!({"mode":"once","speed":1,"placedAt":0,"singleFrame":0})),
    )
    .unwrap();
    assert_eq!(
        reply,
        CompactResult::Command {
            applied: false,
            history_entries_added: 0
        }
    );
    assert_eq!(owner.content_revision(), revision);
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(snapshot(&owner), bytes);
}

#[test]
fn refusals_publish_nothing() {
    let mut owner = NativeObjectHistory::new("instance", decode(&base())).unwrap();
    let bytes = snapshot(&owner);
    let cases: Vec<(Value, DispatchErrorCode)> = vec![
        (
            json!({"command":"object.component.instance.set","layerUid":"ly_plain","speed":2}),
            DispatchErrorCode::NotFound,
        ),
        (
            json!({"command":"object.component.instance.set","layerUid":"xi","speed":2}),
            DispatchErrorCode::NotFound,
        ),
        (
            json!({"command":"object.component.instance.set","layerUid":"missing","speed":2}),
            DispatchErrorCode::NotFound,
        ),
        (
            set(json!({"mode":"bounce"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (set(json!({"speed":0})), DispatchErrorCode::InvalidRequest),
        (set(json!({"speed":-2})), DispatchErrorCode::InvalidRequest),
        (set(json!({"speed":"2"})), DispatchErrorCode::InvalidRequest),
        (
            set(json!({"speed":null})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"singleFrame":-1})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"singleFrame":1.5})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"singleFrame":4_294_967_296_u64})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"placedAt":1.5})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"placedAt":2_147_483_648_u64})),
            DispatchErrorCode::InvalidRequest,
        ),
        (set(json!({"atFrame":2})), DispatchErrorCode::InvalidRequest),
        (
            set(json!({"singleFrame":1,"atFrame":24})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"symTrimIn":3})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            set(json!({"symbolId":"sym_y"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.duplicator.set","layerUid":"ly_inst"}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.component.instance.set"}),
            DispatchErrorCode::InvalidRequest,
        ),
        (json!([1]), DispatchErrorCode::InvalidRequest),
    ];
    for (payload, code) in cases {
        assert_eq!(send(&mut owner, &payload).unwrap_err().0, code, "{payload}");
        assert_eq!(snapshot(&owner), bytes, "{payload}");
        assert_eq!(owner.history_depths(), (0, 0), "{payload}");
    }
}
