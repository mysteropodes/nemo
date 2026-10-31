//! `object.duplicator.config` through the one history owner (#2256 Q4): one revision and one undo entry per real edit,
//! exact undo / redo bytes, no-ops that publish nothing, refusals that publish nothing, strict payloads.
use super::fixture::{document, duplicator_table, layer};
use super::*;
use crate::object_codec::encode_storage_project;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn base() -> Value {
    let layers = [
        layer("ly_dup", "Host", &[(0, "h1")]),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    let host = json!({"mode":"grid","rows":2,"cols":3,"spacingX":150,"spacingY":150,"count":8,"radius":200,
        "startAngle":0,"endAngle":null,"radialOrient":false,"pathLayerUid":null,"pathAlignTangent":true,"seed":17});
    document(
        &layers,
        json!({"componentTables":{"duplicators":duplicator_table(&[("ly_dup", host)])}}),
    )
}
fn config(extra: Value) -> Value {
    let mut payload = json!({"command":"object.duplicator.config","layerUid":"ly_dup"});
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
    owner.dispatch_component_setup("command.document.object.duplicator.config", payload)
}

#[test]
fn one_edit_is_one_revision_and_one_undo_entry_with_exact_undo_and_redo_bytes() {
    let mut owner = NativeObjectHistory::new("config", super::fixture::decode(&base())).unwrap();
    let original = snapshot(&owner);
    let reply = send(
        &mut owner,
        &config(json!({"rows":4,"seed":9,"timeOffset":{"enabled":true}})),
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
    let mut owner = NativeObjectHistory::new("config", super::fixture::decode(&base())).unwrap();
    send(&mut owner, &config(json!({"rows":4}))).unwrap();
    owner.undo(1).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let bytes = snapshot(&owner);
    let revision = owner.content_revision();
    let reply = send(
        &mut owner,
        &config(json!({"rows":2,"mode":"grid","seed":17})),
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
    let mut owner = NativeObjectHistory::new("config", super::fixture::decode(&base())).unwrap();
    let bytes = snapshot(&owner);
    let cases: Vec<(Value, DispatchErrorCode)> = vec![
        (
            json!({"command":"object.duplicator.config","layerUid":"ly_plain","rows":3}),
            DispatchErrorCode::NotFound,
        ),
        (
            json!({"command":"object.duplicator.config","layerUid":"missing","rows":3}),
            DispatchErrorCode::NotFound,
        ),
        (
            config(json!({"mode":"spiral"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"seed":1_000_000})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"seed":-1})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"seed":1.5})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"rows":"3"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"rows":null})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"radialOrient":1})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"pathLayerUid":"ly_plain"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"endAngle":90})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"staggerRandom":{"depth":true}})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"staggerRandom":[true]})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"timeOffset":{"direction":"sideways"}})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            config(json!({"timeOffset":{"offsetFrames":"1"}})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.duplicator.set","layerUid":"ly_dup"}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.duplicator.config"}),
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
