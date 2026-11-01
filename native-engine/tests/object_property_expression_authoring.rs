//! nemo-test-harness: test-application => object_property_expression_authoring_tests
//! #2376 private native expression authoring: exact source, one owner/history and the sealed producer.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use crate::commands::ObjectFillOwner;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::PropertyHolderTarget;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Code;
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
fn payload(expression: Value) -> Value {
    json!({"target":target(),"property":"opacity","expression":expression})
}
fn apply(owner: &mut NativeObjectHistory, p: Value) -> Result<u64, Code> {
    let candidate = ObjectFillOwner::prepare_property_expression_edit_json(
        snapshot(owner),
        &serde_json::to_vec(&p).unwrap(),
    )?;
    owner.commit_property_edit(candidate)
}
fn source(snapshot: &ObjectSnapshot, target: Value, property: &str) -> Result<Option<Value>, Code> {
    snapshot
        .property_expression_source(&typed_target(target), property)
        .map(|v| v.map(|v| serde_json::to_value(v).unwrap()))
}
fn without_source(mut document: Value) -> Value {
    for holder in document["propertyHolders"].as_array_mut().unwrap() {
        for property in holder["properties"].as_array_mut().unwrap() {
            property.as_object_mut().unwrap().remove("expression");
        }
    }
    document
}
fn assert_refused(owner: &mut NativeObjectHistory, raw: &[u8], expected: Code) {
    let before = state(owner);
    assert_eq!(
        ObjectFillOwner::prepare_property_expression_edit_json(snapshot(owner), raw).unwrap_err(),
        expected
    );
    assert_eq!(state(owner), before);
}

#[test]
fn expression_authoring_distinguishes_empty_from_remove_and_preserves_exact_source_and_other_fields(
) {
    // The frozen inline editor stores an empty string; removing uses a distinct null intent.
    let unicode = "// café 🧭\r\nconst 値 = 'é';\nreturn value;\n";
    for version in [1, 2] {
        let mut owner = make_owner(version);
        let before = support::state(&owner);
        let original_bytes = bytes(&owner);
        let retained = snapshot(&owner);
        assert_eq!(source(&retained, target(), "opacity").unwrap(), None);
        assert_eq!(apply(&mut owner, payload(expression("", true))).unwrap(), 1);
        assert_eq!(
            source(&snapshot(&owner), target(), "opacity").unwrap(),
            Some(expression("", true))
        );
        assert_eq!(owner.history_depths(), (1, 0));
        assert_eq!(
            apply(&mut owner, payload(expression(unicode, false))).unwrap(),
            2
        );
        assert_eq!(
            source(&snapshot(&owner), target(), "opacity").unwrap(),
            Some(expression(unicode, false))
        );
        assert_eq!(
            without_source(support::state(&owner)),
            before,
            "static/track/control/foreign holder bytes survive"
        );
        let after = bytes(&owner);
        owner.undo(2).unwrap();
        assert_eq!(
            source(&snapshot(&owner), target(), "opacity").unwrap(),
            Some(expression("", true))
        );
        owner.redo(3).unwrap();
        assert_eq!(bytes(&owner), after);
        let reopened =
            NativeObjectHistory::new("expression-reopen", decode_storage_project(&after).unwrap())
                .unwrap();
        assert_eq!(bytes(&reopened), after);
        assert_eq!(
            source(&snapshot(&reopened), target(), "opacity").unwrap(),
            Some(expression(unicode, false))
        );
        assert_eq!(apply(&mut owner, payload(Value::Null)).unwrap(), 5);
        assert_eq!(
            source(&snapshot(&owner), target(), "opacity").unwrap(),
            None
        );
        assert_eq!(
            support::state(&owner),
            before,
            "remove changes only the expression field"
        );
        owner.undo(5).unwrap();
        assert_eq!(bytes(&owner), after);
        assert_eq!(source(&retained, target(), "opacity").unwrap(), None);
        assert_eq!(
            encode_storage_project(retained.document()).unwrap(),
            original_bytes
        );
    }
}

#[test]
fn expression_authoring_noops_preserve_snapshot_revision_and_redo() {
    let mut owner = make_owner(2);
    let original = state(&owner);
    assert_eq!(apply(&mut owner, payload(Value::Null)).unwrap(), 0);
    assert_eq!(state(&owner), original);
    apply(&mut owner, payload(expression("return 1;", true))).unwrap();
    apply(&mut owner, payload(expression("return 2;", false))).unwrap();
    owner.undo(2).unwrap();
    let before = state(&owner);
    let snapshot_id = snapshot(&owner).snapshot_id().to_owned();
    assert_eq!(before.1, (1, 1));
    assert_eq!(
        apply(&mut owner, payload(expression("return 1;", true))).unwrap(),
        3
    );
    assert_eq!(state(&owner), before);
    assert_eq!(snapshot(&owner).snapshot_id(), snapshot_id);
    owner.redo(3).unwrap();
    assert_eq!(
        source(&snapshot(&owner), target(), "opacity").unwrap(),
        Some(expression("return 2;", false))
    );
}

#[test]
fn expression_authoring_targeted_element_and_order_sources_are_detached_and_exact() {
    for version in [1, 2] {
        let mut owner = make_owner(version);
        let t = json!({"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"});
        let before = support::state(&owner);
        let p = json!({"target":t,"property":"position","expression":expression("return [1, 2];",false)});
        apply(&mut owner, p).unwrap();
        assert_eq!(
            source(&snapshot(&owner), t.clone(), "position").unwrap(),
            Some(expression("return [1, 2];", false))
        );
        assert_eq!(without_source(support::state(&owner)), before);
        let pin = snapshot(&owner);
        assert!(source(&pin, target(), "timeRemap").is_err());
        assert!(source(&pin, target(), "notAProperty").is_err());
        assert!(source(
            &pin,
            json!({"contextId":"scene-root","layerUid":"absent"}),
            "opacity"
        )
        .is_err());
    }
    let mut authored_order = document(2);
    authored_order["compositionOrder"]["contexts"][0]["layers"][0]["order"] =
        json!({"static":[17],"expression":expression("return 7;",true)});
    let mut owner = support::owner_of(&authored_order);
    let before = state(&owner);
    let retained = snapshot(&owner);
    assert_eq!(
        source(&retained, target(), "order").unwrap(),
        Some(expression("return 7;", true))
    );
    // Order has its own writer. The private generic producer must not widen it.
    for expression in [expression("return 9;", false), Value::Null] {
        assert_eq!(
            apply(
                &mut owner,
                json!({"target":target(),"property":"order","expression":expression})
            ),
            Err(Code::Unavailable)
        );
        assert_eq!(state(&owner), before);
    }
    let mut detached = retained
        .property_expression_source(&typed_target(target()), "order")
        .unwrap()
        .unwrap();
    detached.enabled = false;
    detached.code.push_str(" // changed by reader");
    assert_eq!(
        source(&retained, target(), "order").unwrap(),
        Some(expression("return 7;", true))
    );
}

#[test]
fn expression_authoring_prepared_origin_fences_stale_wrong_document_and_wrong_instance_without_mutation(
) {
    let mut owner = make_owner(2);
    let raw = serde_json::to_vec(&payload(expression("return 4;", true))).unwrap();
    let original = snapshot(&owner);
    let first =
        ObjectFillOwner::prepare_property_expression_edit_json(original.clone(), &raw).unwrap();
    let stale = first.clone();
    owner.commit_property_edit(first).unwrap();
    let before = state(&owner);
    assert_eq!(owner.commit_property_edit(stale), Err(Code::StaleRevision));
    assert_eq!(state(&owner), before);
    // A retained old snapshot remains readable, but cannot publish over a new revision.
    let old = ObjectFillOwner::prepare_property_expression_edit_json(original, &raw).unwrap();
    assert_eq!(owner.commit_property_edit(old), Err(Code::StaleRevision));
    assert_eq!(state(&owner), before);
    let same_instance = make_owner(2);
    let foreign =
        ObjectFillOwner::prepare_property_expression_edit_json(snapshot(&same_instance), &raw)
            .unwrap();
    assert_eq!(
        owner.commit_property_edit(foreign),
        Err(Code::WrongDocument)
    );
    assert_eq!(state(&owner), before);
    let different = NativeObjectHistory::new(
        "other-instance",
        decode_storage_project(&serde_json::to_vec(&document(2)).unwrap()).unwrap(),
    )
    .unwrap();
    let foreign =
        ObjectFillOwner::prepare_property_expression_edit_json(snapshot(&different), &raw).unwrap();
    assert_eq!(
        owner.commit_property_edit(foreign),
        Err(Code::WrongInstance)
    );
    assert_eq!(state(&owner), before);
}

#[test]
fn expression_authoring_rejects_missing_extra_duplicate_and_malformed_fields_atomically() {
    let mut owner = make_owner(2);
    apply(&mut owner, payload(expression("return 1;", true))).unwrap();
    owner.undo(1).unwrap();
    for raw in [
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity"}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":null,"extra":1}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":null,"expression":null}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1","layerUid":"L2"},"property":"opacity","expression":null}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"enabled":true,"enabled":false,"code":"x"}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"enabled":true,"code":"x","code":"y"}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"enabled":true}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"code":"x"}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"enabled":true,"code":"x","extra":0}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"enabled":null,"code":"x"}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":{"enabled":true,"code":null}}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":false}"#,
        r#"{"target":{"contextId":"scene-root","layerUid":"L1"},"property":"opacity","expression":[]}"#,
    ] {
        assert_refused(&mut owner, raw.as_bytes(), Code::InvalidRequest);
    }
    assert_refused(&mut owner, &[0xff], Code::InvalidRequest);
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn expression_authoring_bounds_code_by_utf8_bytes_and_private_envelope_without_partial_publication()
{
    let mut owner = make_owner(2);
    let exact = "é".repeat(2048);
    assert_eq!(exact.len(), 4096);
    assert_eq!(
        apply(&mut owner, payload(expression(&exact, false))).unwrap(),
        1
    );
    assert_eq!(
        source(&snapshot(&owner), target(), "opacity").unwrap(),
        Some(expression(&exact, false))
    );
    let too_long = format!("{exact}x");
    assert_refused(
        &mut owner,
        &serde_json::to_vec(&payload(expression(&too_long, false))).unwrap(),
        Code::InvalidRequest,
    );
    let p = serde_json::to_vec(&payload(expression("return 1;", true))).unwrap();
    let mut bounded = p.clone();
    bounded.resize(32768, b' ');
    assert!(
        ObjectFillOwner::prepare_property_expression_edit_json(snapshot(&owner), &bounded).is_ok()
    );
    bounded.push(b' ');
    assert_refused(&mut owner, &bounded, Code::InvalidRequest);
}

#[test]
fn expression_authoring_invalid_syntax_retains_source_and_evaluates_with_existing_raw_fallback() {
    use crate::evaluation::property_expression_runtime::Diagnostic;
    use crate::object_expression_runtime::{evaluate_property_for_owner, ExpressionLimits};
    let mut owner = make_owner(2);
    apply(&mut owner, payload(expression("return (", true))).unwrap();
    let pin = snapshot(&owner);
    let before = state(&owner);
    let result = evaluate_property_for_owner(
        &owner,
        &pin,
        &typed_target(target()),
        "opacity",
        0.,
        0,
        ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap(),
    )
    .unwrap();
    assert_eq!(result.value, vec![20.]);
    assert!(matches!(result.diagnostic, Some(Diagnostic::Syntax { .. })));
    assert_eq!(
        source(&pin, target(), "opacity").unwrap(),
        Some(expression("return (", true))
    );
    assert_eq!(state(&owner), before);
}
