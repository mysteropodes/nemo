//! Independent actual-owner command/replay/content controls.
use crate::{commands::OpacityRequest, history::NativeObjectHistory, object_codec};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]})
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "instance-A",
        object_codec::decode_project(&document().to_string().into_bytes()).unwrap(),
    )
    .unwrap()
}
fn request(owner: &NativeObjectHistory, id: &str, revision: u64) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"operation":"command.document.object.fill.set",
        "expectedRevision":revision,"payload":cases()["command"]["request"]["payload"]})
}
fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(owner.try_dispatch_object_fill(&typed).unwrap().unwrap()).unwrap()
}
fn state(owner: &NativeObjectHistory) -> Value {
    json!({"revision":owner.content_revision(),"depths":owner.history_depths(),
        "document":owner.acquire_snapshot(owner.content_revision()).unwrap().document()})
}

#[test]
fn changed_command_commits_exact_golden_record_and_preserves_old_pins() {
    let mut owner = owner();
    let zero = owner.acquire_snapshot(0).unwrap();
    let request = request(&owner, "changed", 0);
    let response = dispatch(&mut owner, &request);
    assert_eq!(response["ok"], true);
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    for field in ["apiVersion", "requestId", "instanceId", "documentId"] {
        assert_eq!(response[field], request[field]);
    }
    assert_eq!(owner.history_depths(), (1, 0));
    let expected = cases()["command"]["afterRecord"].clone();
    let after = serde_json::to_value(owner.acquire_snapshot(1).unwrap().document()).unwrap();
    assert_eq!(after["objects"][0], expected);
    assert_eq!(after["objects"][1], document()["objects"][1]);
    assert_eq!(after["layers"], document()["layers"]);
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), document());
    let encoded =
        object_codec::encode_project(owner.acquire_snapshot(1).unwrap().document()).unwrap();
    assert_eq!(
        serde_json::to_value(object_codec::decode_project(&encoded).unwrap()).unwrap(),
        after
    );
}

#[test]
fn exact_retry_after_later_history_replays_old_receipt_without_effect() {
    let mut owner = owner();
    let request = request(&owner, "retained", 0);
    let first = dispatch(&mut owner, &request);
    assert_eq!(dispatch(&mut owner, &request), first);
    assert_eq!(owner.content_revision(), 1);
    owner.undo(1).unwrap();
    let before = state(&owner);
    assert_eq!(dispatch(&mut owner, &request), first);
    assert_eq!(state(&owner), before);
    owner.redo(2).unwrap();
    let before = state(&owner);
    let mut changed = request.clone();
    changed["expectedRevision"] = json!(3);
    changed["payload"]["fill"]["r"] = json!(0.1);
    assert_eq!(
        dispatch(&mut owner, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&owner), before);
    assert_eq!(dispatch(&mut owner, &request), first);
}

#[test]
fn numeric_noop_preserves_bytes_revision_and_redo_and_has_stable_retry() {
    let mut owner = owner();
    let changed = request(&owner, "first", 0);
    dispatch(&mut owner, &changed);
    owner.undo(1).unwrap();
    let before = state(&owner);
    let bytes =
        object_codec::encode_project(owner.acquire_snapshot(2).unwrap().document()).unwrap();
    let mut noop = request(&owner, "noop", 2);
    noop["payload"]["fill"] = cases()["records"][0]["fill"].clone();
    let first = dispatch(&mut owner, &noop);
    assert_eq!(
        first["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(first["contentRevision"], 2);
    assert_eq!(state(&owner), before);
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(
        object_codec::encode_project(owner.acquire_snapshot(2).unwrap().document()).unwrap(),
        bytes
    );
    assert_eq!(dispatch(&mut owner, &noop), first);
    owner.redo(2).unwrap();
    let before = state(&owner);
    assert_eq!(dispatch(&mut owner, &noop), first);
    assert_eq!(state(&owner), before);
    let mut equivalent = request(&owner, "numeric-equivalent", 3);
    equivalent["payload"]["stableTarget"] = cases()["records"][1]["target"].clone();
    equivalent["payload"]["fill"] = json!({"kind":"solid","r":1.0,"g":-0.0,"b":0.0,"a":1.0});
    assert_eq!(
        dispatch(&mut owner, &equivalent)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&owner), before);
}

#[test]
fn admitted_failures_are_exactly_retained_and_preserve_all_owner_state() {
    for (path, value, code) in [
        ("/expectedRevision", json!(1), "stale_revision"),
        (
            "/payload/stableTarget/strokeId",
            json!("absent"),
            "not_found",
        ),
        (
            "/payload/stableTarget/frameScope/kind",
            json!("all-frames"),
            "invalid_request",
        ),
        ("/payload/fill/r", json!(2), "invalid_request"),
    ] {
        let mut owner = owner();
        let mut bad = request(&owner, "bad", 0);
        *bad.pointer_mut(path).unwrap() = value;
        let before = state(&owner);
        let first = dispatch(&mut owner, &bad);
        assert_eq!(first["ok"], false);
        assert_eq!(first["error"]["code"], code);
        assert_eq!(dispatch(&mut owner, &bad), first);
        assert_eq!(state(&owner), before);
        let corrected = request(&owner, "bad", 0);
        assert_eq!(
            dispatch(&mut owner, &corrected)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(state(&owner), before);
    }
    let mut owner = owner();
    let mut cancelled = request(&owner, "cancelled", 0);
    cancelled["cancelledBeforeDispatch"] = json!(true);
    let before = state(&owner);
    let first = dispatch(&mut owner, &cancelled);
    assert_eq!(first["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(dispatch(&mut owner, &cancelled), first);
    assert_eq!(state(&owner), before);
    cancelled
        .as_object_mut()
        .unwrap()
        .remove("cancelledBeforeDispatch");
    assert_eq!(
        dispatch(&mut owner, &cancelled)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&owner), before);
    let mut missing = request(&owner, "missing-revision", 0);
    missing.as_object_mut().unwrap().remove("expectedRevision");
    let first = dispatch(&mut owner, &missing);
    assert_eq!(first["error"]["code"], "invalid_request");
    assert_eq!(dispatch(&mut owner, &missing), first);
    assert_eq!(state(&owner), before);
    missing["expectedRevision"] = json!(0);
    assert_eq!(
        dispatch(&mut owner, &missing)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&owner), before);
}

#[test]
fn identity_bounds_and_unsupported_routes_cannot_commit_or_borrow_opacity() {
    let mut owner = owner();
    let before = state(&owner);
    for (path, value, code) in [
        ("/instanceId", json!("other-instance"), "wrong_instance"),
        ("/documentId", json!("other-document"), "wrong_document"),
        ("/apiVersion", json!(1), "invalid_request"),
        (
            "/expectedRevision",
            json!(9_007_199_254_740_992_u64),
            "invalid_request",
        ),
        ("/payload", json!([]), "invalid_request"),
    ] {
        let mut request = request(&owner, "preflight", 0);
        *request.pointer_mut(path).unwrap() = value;
        assert_eq!(dispatch(&mut owner, &request)["error"]["code"], code);
        assert_eq!(state(&owner), before);
    }
    let mut invalid = request(&owner, "valid", 0);
    invalid["requestId"] = json!("bad id");
    let typed = serde_json::from_value(invalid).unwrap();
    assert!(owner.try_dispatch_object_fill(&typed).is_err());
    let mut oversized = request(&owner, "big", 0);
    oversized["payload"]["fill"]["unused"] = json!("🟠".repeat(1100));
    assert!(serde_json::to_vec(&oversized).unwrap().len() > 4096);
    assert_eq!(
        dispatch(&mut owner, &oversized)["error"]["code"],
        "invalid_request"
    );
    let mut opacity = request(&owner, "wrong-route", 0);
    opacity["operation"] = json!("command.document.apply");
    let typed = serde_json::from_value(opacity).unwrap();
    assert!(owner.try_dispatch_object_fill(&typed).unwrap().is_none());
    assert_eq!(state(&owner), before);
}

#[test]
fn complete_utf8_request_budget_accepts_exact_limit_and_refuses_one_more() {
    let provisional = owner();
    let mut req = request(&provisional, "exact-limit", 0);
    req["payload"]["stableTarget"]["strokeId"] = json!("");
    let base = serde_json::to_vec(&req).unwrap().len();
    // Reserve header slack; incarnation counter width can vary across tests.
    let count = 4096 - base - 64;
    let id = format!("{}{}", "🟠".repeat(count / 4), "x".repeat(count % 4));
    let mut doc = document();
    doc["objects"][0]["target"]["strokeId"] = json!(id);
    let mut exact = NativeObjectHistory::new(
        "instance-A",
        object_codec::decode_project(doc.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    req["documentId"] = json!(exact.document_id());
    req["payload"]["stableTarget"]["strokeId"] = doc["objects"][0]["target"]["strokeId"].clone();
    let padding = 4096 - serde_json::to_vec(&req).unwrap().len();
    let request_id = format!("exact-limit{}", "x".repeat(padding));
    assert!(request_id.len() <= 128);
    req["requestId"] = json!(request_id);
    assert_eq!(serde_json::to_vec(&req).unwrap().len(), 4096);
    assert_eq!(dispatch(&mut exact, &req)["contentRevision"], 1);
    let before = state(&exact);
    req["payload"]["stableTarget"]["strokeId"] = json!(format!("{}x", id));
    assert_eq!(serde_json::to_vec(&req).unwrap().len(), 4097);
    assert_eq!(
        dispatch(&mut exact, &req)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&exact), before);
}
