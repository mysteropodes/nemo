//! nemo-test-harness: test-application => object_property_expression_public_tests
//! #2376 public expression authoring edit actions and the expressionSource read view over the sealed producer.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::PropertyHolderTarget;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Code, OpacityRequest};
use serde_json::{json, Value};

fn target() -> Value {
    json!({"contextId":"scene-root","layerUid":"L1"})
}
fn typed_target(value: Value) -> PropertyHolderTarget {
    serde_json::from_value(value).unwrap()
}
fn document(version: u32) -> Value {
    support::document(
        version,
        vec![support::record_at("A", "L1", 0, "s-a")],
        Some(json!([
            {"target":target(),"properties":[
                {"property":"opacity","static":[42],"track":{"keys":[{"frame":0,"v":[20],"hIn":[1,2],"hOut":[3,4],"hold":true},{"frame":10,"v":[80]}]}},
                {"property":"rotation","static":[17]}
            ]},
            {"target":{"contextId":"scene-root","layerUid":"L2"},"properties":[{"property":"opacity","static":[91]}]},
            {"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"},"properties":[{"property":"position","static":[3,4]}]}
        ])),
    )
}
fn make_owner(version: u32) -> NativeObjectHistory {
    support::owner_of(&document(version))
}
fn snapshot(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(snapshot(owner).document()).unwrap()
}
fn state(owner: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (
        owner.content_revision(),
        owner.history_depths(),
        bytes(owner),
    )
}
fn expression(code: &str, enabled: bool) -> Value {
    json!({"enabled":enabled,"code":code})
}
fn without_source(mut document: Value) -> Value {
    for holder in document["propertyHolders"].as_array_mut().unwrap() {
        for property in holder["properties"].as_array_mut().unwrap() {
            property.as_object_mut().unwrap().remove("expression");
        }
    }
    document
}
fn public_edit(owner: &mut NativeObjectHistory, target: Value, property: &str, action: Value) -> Value {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!(
        "public-expression-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let q: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),"documentId":owner.document_id(),"expectedRevision":owner.content_revision(),"operation":"command.document.object.property.edit","payload":{"command":"object.property.edit","target":target,"property":property,"action":action}})).unwrap();
    serde_json::to_value(owner.try_dispatch_object_command(&q).unwrap().unwrap()).unwrap()
}
fn public_refusal(owner: &mut NativeObjectHistory, target: Value, property: &str, action: Value, code: &str) {
    let before = state(owner);
    let response = public_edit(owner, target, property, action.clone());
    assert_eq!(response["ok"], false, "{action} {response}");
    assert_eq!(response["error"]["code"], code, "{response}");
    assert_eq!(state(owner), before);
}
fn public_read(owner: &NativeObjectHistory, view: Value) -> Result<Value, Code> {
    let pin = snapshot(owner);
    let read = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),"documentSnapshotId":pin.snapshot_id(),"view":view});
    pin.query_property_read_json(&serde_json::to_vec(&read).unwrap())
}
fn source_view(target: Value, property: &str) -> Value {
    json!({"kind":"expressionSource","target":target,"property":property})
}
fn expression_of(owner: &NativeObjectHistory) -> Value {
    public_read(owner, source_view(target(), "opacity")).unwrap()["expression"].clone()
}

#[test]
fn public_expression_actions_author_code_enable_and_remove_as_one_history_entry_each() {
    // The frozen inline editor commits the code and the enable checkbox independently.
    let unicode = "// café 🧭\r\nconst 値 = 'é';\nreturn value;\n";
    for version in [1, 2] {
        let mut owner = make_owner(version);
        let before = support::state(&owner);
        assert_eq!(expression_of(&owner), Value::Null);
        // A new expression starts disabled; an empty code stays present-but-empty.
        let r = public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":""}));
        assert_eq!(r["result"], json!({"applied":true,"historyEntriesAdded":1}), "{r}");
        assert_eq!(expression_of(&owner), expression("", false));
        public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpressionEnabled","enabled":true}));
        assert_eq!(expression_of(&owner), expression("", true));
        // Code replacement keeps the enabled flag and every byte.
        public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":unicode}));
        assert_eq!(expression_of(&owner), expression(unicode, true));
        public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpressionEnabled","enabled":false}));
        assert_eq!(expression_of(&owner), expression(unicode, false));
        assert_eq!(owner.history_depths(), (4, 0));
        assert_eq!(owner.content_revision(), 4);
        assert_eq!(without_source(support::state(&owner)), before, "static/track/foreign holders survive");
        let authored = bytes(&owner);
        let reopened = NativeObjectHistory::new("expression-public-reopen", decode_storage_project(&authored).unwrap()).unwrap();
        assert_eq!(expression_of(&reopened), expression(unicode, false));
        // Remove clears only the expression; undo/redo walk the same entries.
        public_edit(&mut owner, target(), "opacity", json!({"kind":"removeExpression"}));
        assert_eq!(expression_of(&owner), Value::Null);
        assert_eq!(support::state(&owner), before);
        for _ in 0..5 {
            owner.undo(owner.content_revision()).unwrap();
        }
        assert_eq!(support::state(&owner), before);
        assert_eq!(owner.history_depths(), (0, 5));
        for _ in 0..4 {
            owner.redo(owner.content_revision()).unwrap();
        }
        assert_eq!(bytes(&owner), authored);
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(expression_of(&owner), Value::Null);
    }
}

#[test]
fn public_expression_noops_add_no_revision_and_preserve_redo() {
    let mut owner = make_owner(2);
    let original = state(&owner);
    // Nothing to disable and nothing to remove: no revision, no history entry.
    for action in [json!({"kind":"removeExpression"}), json!({"kind":"setExpressionEnabled","enabled":false})] {
        let r = public_edit(&mut owner, target(), "opacity", action);
        assert_eq!(r["result"], json!({"applied":false,"historyEntriesAdded":0}), "{r}");
        assert_eq!(state(&owner), original);
    }
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":"return 1;"}));
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":"return 2;"}));
    owner.undo(owner.content_revision()).unwrap();
    let before = state(&owner);
    assert_eq!(before.1, (1, 1));
    // Re-applying the identical source is a no-op that must not drop the redo entry.
    let r = public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":"return 1;"}));
    assert_eq!(r["result"], json!({"applied":false,"historyEntriesAdded":0}), "{r}");
    assert_eq!(state(&owner), before);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(expression_of(&owner), expression("return 2;", false));
}

#[test]
fn public_expression_actions_target_elements_and_refuse_wrong_holders_and_selectors() {
    let mut owner = make_owner(2);
    let element = json!({"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"});
    public_edit(&mut owner, element.clone(), "position", json!({"kind":"setExpression","code":"return [1, 2];"}));
    assert_eq!(public_read(&owner, source_view(element.clone(), "position")).unwrap()["expression"], expression("return [1, 2];", false));
    assert_eq!(expression_of(&owner), Value::Null, "the layer holder is untouched");
    let absent = json!({"contextId":"scene-root","layerUid":"absent"});
    for (t, p, code) in [
        (absent.clone(), "opacity", "not_found"),
        (target(), "notAProperty", "unavailable"),
        (target(), "order", "unavailable"),
        (target(), "timeRemap", "unavailable"),
    ] {
        public_refusal(&mut owner, t, p, json!({"kind":"setExpression","code":"x"}), code);
        public_refusal(&mut owner, absent.clone(), p, json!({"kind":"removeExpression"}), if matches!(p, "order" | "timeRemap") { code } else { "not_found" });
    }
    assert_eq!(public_read(&owner, source_view(target(), "timeRemap")), Err(Code::Unavailable));
    assert_eq!(public_read(&owner, source_view(target(), "notAProperty")), Err(Code::Unavailable));
    assert_eq!(public_read(&owner, source_view(absent, "opacity")), Err(Code::NotFound));
}

#[test]
fn public_expression_actions_refuse_stale_unknown_and_oversized_input_without_mutation() {
    let mut owner = make_owner(2);
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":"return 1;"}));
    let stale = format!(r#"{{"apiVersion":2,"requestId":"stale","instanceId":"{}","documentId":"{}","expectedRevision":0,"operation":"command.document.object.property.edit","payload":{{"command":"object.property.edit","target":{},"property":"opacity","action":{{"kind":"removeExpression"}}}}}}"#,
        owner.instance_id(), owner.document_id(), target());
    let before = state(&owner);
    let q: OpacityRequest = serde_json::from_str(&stale).unwrap();
    let response = serde_json::to_value(owner.try_dispatch_object_command(&q).unwrap().unwrap()).unwrap();
    assert_eq!(response["error"]["code"], "stale_revision", "{response}");
    assert_eq!(state(&owner), before);
    for action in [
        json!({"kind":"setExpression"}),
        json!({"kind":"setExpression","code":null}),
        json!({"kind":"setExpression","code":7}),
        json!({"kind":"setExpression","code":"x","enabled":true}),
        json!({"kind":"setExpression","code":"x","frame":0}),
        json!({"kind":"setExpression","expression":{"enabled":true,"code":"x"}}),
        json!({"kind":"setExpressionEnabled"}),
        json!({"kind":"setExpressionEnabled","enabled":"true"}),
        json!({"kind":"setExpressionEnabled","enabled":null}),
        json!({"kind":"setExpressionEnabled","enabled":true,"code":"x"}),
        json!({"kind":"removeExpression","code":null}),
        json!({"kind":"removeExpression","frame":0}),
        json!(["setExpression","x"]),
    ] {
        public_refusal(&mut owner, target(), "opacity", action, "invalid_request");
    }
    // Over the 4096-byte request and code ceiling: refused whole, never truncated.
    for code in ["x".repeat(4097), "é".repeat(2049), "é".repeat(2000)] {
        public_refusal(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":code}), "invalid_request");
    }
    assert_eq!(expression_of(&owner), expression("return 1;", false));
}

#[test]
fn public_expression_source_read_is_exact_pinned_bounded_and_never_truncated() {
    let mut owner = make_owner(2);
    let empty = public_read(&owner, source_view(target(), "opacity")).unwrap();
    assert_eq!(empty["kind"], "expressionSource");
    assert_eq!(empty["expression"], Value::Null);
    assert_eq!(empty["dimension"], 1);
    assert!(empty.get("frame").is_none() && empty.get("evaluation").is_none(), "storage read only: {empty}");
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":"return ("}));
    let pinned = snapshot(&owner);
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpressionEnabled","enabled":true}));
    assert_eq!(pinned.property_expression_source(&typed_target(target()), "opacity").unwrap().unwrap().enabled, false);
    assert_eq!(expression_of(&owner), expression("return (", true), "invalid syntax stays authored data");
    // Within the request ceiling but past the 3072-byte result bound: refused, not trimmed.
    let big = "y".repeat(3500);
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":big}));
    assert_eq!(snapshot(&owner).property_expression_source(&typed_target(target()), "opacity").unwrap().unwrap().code.len(), 3500);
    assert_eq!(public_read(&owner, source_view(target(), "opacity")), Err(Code::Unavailable));
    let fits = "z".repeat(2500);
    public_edit(&mut owner, target(), "opacity", json!({"kind":"setExpression","code":fits}));
    assert_eq!(expression_of(&owner), expression(&"z".repeat(2500), true));
    for bad in [
        json!({"kind":"expressionSource","target":target(),"property":"opacity","frame":0}),
        json!({"kind":"expressionSource","target":target()}),
        json!({"kind":"expressionSource","target":target(),"property":""}),
    ] {
        assert_eq!(public_read(&owner, bad), Err(Code::InvalidRequest));
    }
}

