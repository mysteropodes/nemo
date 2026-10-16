//! PRIVATE UNRUN: literal flag-only deltas, sole-owner fences and receipts.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn fixture(version: u32) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","layerUid":"L0","strokeId":"ink",
        "frameScope":{"kind":"authored","frame":0}});
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":8,"layers":[{"layerUid":"L0","folderId":"g"},
        {"layerUid":"L1","locked":false,"shy":false}],"objects":[record],
        "layerFolders":{"g":{"name":"Keep group","collapsed":false}},
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"opacity","static":[50]}]}]});
    if version == 2 {
        drawing["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":0,"order":{}},
                {"layerUid":"L1","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"L0","strokeId":"ink","naturalPosition":0,"order":{}}]}]}]});
    }
    drawing
}
fn owner(instance: &str, value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        instance,
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    (
        o.content_revision(),
        o.history_depths(),
        head(o).snapshot_id().into(),
        bytes(o),
    )
}
fn payload(layer: &str, flag: &str, value: bool) -> Value {
    json!({"command":"object.layer.policy.edit","layerUid":layer,"flag":flag,"value":value})
}
fn prepare(o: &NativeObjectHistory, layer: &str, flag: &str, value: bool) -> PreparedLayerPolicy {
    o.prepare_layer_policy_json(payload(layer, flag, value).to_string().as_bytes())
        .unwrap()
}
fn request(o: &NativeObjectHistory, id: &str, flag: &str, value: bool) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":"command.document.object.layer.policy.edit","payload":payload("L0",flag,value)})).unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, r: &OpacityRequest) -> Value {
    serde_json::to_value(o.try_dispatch_object_command(r).unwrap().unwrap()).unwrap()
}
fn fill() -> Value {
    json!({"command":"object.fill.set","stableTarget":{"contextId":"scene-root","layerUid":"L0",
        "strokeId":"ink","frameScope":{"kind":"authored","frame":0}},
        "fill":{"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1}})
}
fn changed_redo(o: &mut NativeObjectHistory) {
    let t = prepare(o, "L0", "shy", true);
    o.commit_layer_policy(t).unwrap();
    o.undo(o.content_revision()).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn layer_policy_preparation_is_detached_and_each_flag_changes_only_its_literal_field() {
    for version in [1, 2] {
        for flag in ["locked", "shy"] {
            let source = fixture(version);
            let mut o = owner("policy", &source);
            let zero = head(&o);
            let before = state(&o);
            let t = prepare(&o, "L0", flag, true);
            assert_eq!(state(&o), before);
            assert_eq!(o.commit_layer_policy(t), Ok(1));
            assert_eq!(o.history_depths(), (1, 0));
            let mut expected = source.clone();
            expected["layers"][0][flag] = json!(true);
            assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), expected);
            assert_eq!(serde_json::to_value(zero.document()).unwrap(), source);
            assert_eq!(
                decode_storage_project(&bytes(&o)).unwrap(),
                *head(&o).document()
            );
        }
    }
}
#[test]
fn absent_and_explicit_false_noops_preserve_presence_snapshot_revision_and_redo() {
    let mut o = owner("policy", &fixture(2));
    changed_redo(&mut o);
    for layer in ["L0", "L1"] {
        for flag in ["locked", "shy"] {
            let before = state(&o);
            let t = prepare(&o, layer, flag, false);
            assert_eq!(o.commit_layer_policy(t), Ok(before.0));
            assert_eq!(state(&o), before);
        }
    }
}
#[test]
fn true_to_false_is_one_commit_and_roundtrips_explicit_false_through_undo_redo() {
    for flag in ["locked", "shy"] {
        let mut source = fixture(2);
        source["layers"][0][flag] = json!(true);
        let mut o = owner("policy", &source);
        let t = prepare(&o, "L0", flag, false);
        assert_eq!(o.commit_layer_policy(t), Ok(1));
        let mut expected = source.clone();
        expected["layers"][0][flag] = json!(false);
        assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), expected);
        assert_eq!(o.undo(1), Ok(2));
        assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), source);
        assert_eq!(o.redo(2), Ok(3));
        assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), expected);
        assert_eq!(
            decode_storage_project(&bytes(&o)).unwrap(),
            *head(&o).document()
        );
    }
}
#[test]
fn changed_flag_clears_redo_and_other_flag_never_changes() {
    let mut o = owner("policy", &fixture(2));
    changed_redo(&mut o);
    let t = prepare(&o, "L0", "locked", true);
    let revision = o.content_revision();
    assert_eq!(o.commit_layer_policy(t), Ok(revision + 1));
    assert_eq!(o.history_depths(), (1, 0));
    assert!(
        serde_json::to_value(head(&o).document()).unwrap()["layers"][0]
            .get("shy")
            .is_none()
    );
}
#[test]
fn raw_policy_maps_reject_duplicate_unknown_missing_null_positional_and_wrong_types_atomically() {
    let mut o = owner("policy", &fixture(2));
    changed_redo(&mut o);
    let before = state(&o);
    let valid = payload("L0", "locked", true).to_string();
    let mut invalid = vec![
        "null".to_string(),
        "[]".into(),
        "[\"object.layer.policy.edit\",\"L0\",\"locked\",true]".into(),
    ];
    for key in ["command", "layerUid", "flag", "value"] {
        let value: Value = serde_json::from_str(&valid).unwrap();
        invalid.push(format!(
            "{{{},{}:{}}}",
            &valid[1..valid.len() - 1],
            json!(key),
            value[key]
        ));
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(key);
        invalid.push(missing.to_string());
        let mut null = value.clone();
        null[key] = Value::Null;
        invalid.push(null.to_string());
    }
    for (key, value) in [
        ("extra", json!(true)),
        ("command", json!("object.layer.visibility.set")),
        ("layerUid", json!("")),
        ("flag", json!("solo")),
        ("flag", json!({"locked":null})),
        ("flag", json!(false)),
        ("value", json!(1)),
        ("value", json!("true")),
        ("layerUid", json!(3)),
    ] {
        let mut wrong: Value = serde_json::from_str(&valid).unwrap();
        wrong[key] = value;
        invalid.push(wrong.to_string());
    }
    invalid.push(format!("{}{}", valid, " ".repeat(4097)));
    for raw in invalid {
        assert_eq!(
            o.prepare_layer_policy_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{raw}"
        );
        assert_eq!(state(&o), before);
    }
    assert_eq!(
        o.prepare_layer_policy_json(payload("missing", "locked", true).to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::NotFound
    );
    assert_eq!(state(&o), before);
}
#[test]
fn raw_policy_payload_accepts_exact_4096_bytes_and_refuses_4097_without_mutation() {
    let mut o = owner("policy", &fixture(2));
    let before = state(&o);
    let mut raw = payload("L0", "locked", false).to_string();
    raw.push_str(&" ".repeat(4096 - raw.len()));
    let token = o.prepare_layer_policy_json(raw.as_bytes()).unwrap();
    assert_eq!(o.commit_layer_policy(token), Ok(0));
    assert_eq!(state(&o), before);
    raw.push(' ');
    assert_eq!(
        o.prepare_layer_policy_json(raw.as_bytes()).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(state(&o), before);
}

#[test]
fn foreign_instance_and_document_candidates_cannot_publish_into_another_owner() {
    let value = fixture(2);
    let first = owner("one", &value);
    let mut second = owner("two", &value);
    let before = state(&second);
    assert_eq!(
        second.commit_layer_policy(prepare(&first, "L0", "locked", true)),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(state(&second), before);
    let first = owner("two", &value);
    assert_ne!(first.document_id(), second.document_id());
    assert_eq!(
        second.commit_layer_policy(prepare(&first, "L0", "shy", true)),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(state(&second), before);
}
#[test]
fn old_changed_and_noop_tokens_refuse_after_a_commit_even_if_undo_restores_the_same_bytes() {
    let mut o = owner("policy", &fixture(2));
    let changed = prepare(&o, "L0", "locked", true);
    let noop = prepare(&o, "L0", "shy", false);
    let t = prepare(&o, "L0", "shy", true);
    o.commit_layer_policy(t).unwrap();
    o.undo(1).unwrap();
    let before = state(&o);
    assert_eq!(
        o.commit_layer_policy(changed),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&o), before);
    assert_eq!(
        o.commit_layer_policy(noop),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&o), before);
}
#[test]
fn staged_policy_receipt_replay_is_exact_and_does_not_repeat_history() {
    let mut o = owner("policy", &fixture(2));
    let r = request(&o, "lock-on", "locked", true);
    let first = dispatch(&mut o, &r);
    assert_eq!(first["ok"], true);
    assert_eq!(first["result"]["applied"], true);
    assert_eq!(first["result"]["historyEntriesAdded"], 1);
    let before = state(&o);
    assert_eq!(dispatch(&mut o, &r), first);
    assert_eq!(state(&o), before);
    let mut conflict = r.clone();
    conflict.payload["value"] = json!(false);
    assert_eq!(
        dispatch(&mut o, &conflict)["error"]["code"],
        "InvalidRequest"
    );
    assert_eq!(state(&o), before);
    let new = request(&o, "same-lock", "locked", true);
    let noop = dispatch(&mut o, &new);
    assert_eq!(noop["ok"], true);
    assert_eq!(noop["result"]["applied"], false);
    assert_eq!(noop["result"]["historyEntriesAdded"], 0);
    assert_eq!(state(&o), before);
    let shy = request(&o, "shy-on", "shy", true);
    assert_eq!(dispatch(&mut o, &shy)["ok"], true);
    let after_shy = state(&o);
    assert_eq!(dispatch(&mut o, &r), first);
    assert_eq!(state(&o), after_shy);
}
#[test]
fn staged_policy_cancel_stale_missing_revision_wrong_scope_and_malformed_payload_keep_redo() {
    let mut o = owner("policy", &fixture(2));
    changed_redo(&mut o);
    let before = state(&o);
    let mut cancelled = request(&o, "cancel", "locked", true);
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "CancelledBeforeDispatch"
    );
    assert_eq!(state(&o), before);
    let mut stale = request(&o, "stale", "locked", true);
    stale.expected_revision = Some(0);
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "StaleRevision");
    assert_eq!(state(&o), before);
    let mut missing = request(&o, "missing-revision", "locked", true);
    missing.expected_revision = None;
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "InvalidRequest"
    );
    assert_eq!(state(&o), before);
    for (id, code, instance) in [
        ("wrong-instance", "WrongInstance", true),
        ("wrong-document", "WrongDocument", false),
    ] {
        let mut wrong = request(&o, id, "shy", true);
        if instance {
            wrong.instance_id = "foreign".into();
        } else {
            wrong.document_id = "foreign".into();
        }
        assert_eq!(dispatch(&mut o, &wrong)["error"]["code"], code);
        assert_eq!(state(&o), before);
    }
    let mut invalid = request(&o, "bad-flag", "shy", true);
    invalid.payload["flag"] = json!("solo");
    assert_eq!(
        dispatch(&mut o, &invalid)["error"]["code"],
        "InvalidRequest"
    );
    assert_eq!(state(&o), before);
}
#[test]
fn locking_then_unlocking_controls_only_the_accepted_drawing_fill_guard() {
    let mut o = owner("policy", &fixture(2));
    let t = prepare(&o, "L0", "locked", true);
    o.commit_layer_policy(t).unwrap();
    let before = state(&o);
    assert_eq!(
        o.prepare_fill_json(fill().to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(state(&o), before);
    let property = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","action":{"kind":"setValue","frame":0,"values":[75]}});
    let t = o
        .prepare_property_edit_json(property.to_string().as_bytes())
        .unwrap();
    o.commit_property_edit(t).unwrap();
    assert!(head(&o).document().layers()[0].locked());
    let t = prepare(&o, "L0", "shy", true);
    o.commit_layer_policy(t).unwrap();
    let t = prepare(&o, "L0", "locked", false);
    o.commit_layer_policy(t).unwrap();
    let t = o.prepare_fill_json(fill().to_string().as_bytes()).unwrap();
    let revision = o.content_revision();
    assert_eq!(o.commit_fill(t), Ok(revision + 1));
    let saved: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    assert_eq!(saved["layers"][0]["locked"], false);
    assert_eq!(saved["layers"][0]["shy"], true);
}
#[test]
fn policy_undo_redo_is_routed_by_the_existing_shared_history_dispatch() {
    let mut o = owner("policy", &fixture(2));
    let source = bytes(&o);
    let r = request(&o, "lock", "locked", true);
    assert_eq!(dispatch(&mut o, &r)["ok"], true);
    let changed = bytes(&o);
    for (id, op, want) in [("undo", "undo", source), ("redo", "redo", changed)] {
        let mut r = request(&o, id, "locked", true);
        r.operation = format!("command.document.object.{op}");
        r.payload = json!({"command":format!("object.{op}")});
        assert_eq!(dispatch(&mut o, &r)["ok"], true);
        assert_eq!(bytes(&o), want);
    }
}
#[cfg(feature = "project_media_codec")]
#[test]
fn authored_policy_flags_survive_exact_project_save_reopen_and_old_snapshot_pins() {
    let mut o = NativeObjectHistory::new_blank_project("project", 640, 480, 24.).unwrap();
    let zero = head(&o);
    let layer = zero.document().layers()[0].layer_uid().to_owned();
    for flag in ["locked", "shy"] {
        let t = prepare(&o, &layer, flag, true);
        o.commit_layer_policy(t).unwrap();
    }
    let current = head(&o);
    let saved = o
        .serialize_project_file(o.content_revision(), head(&o).snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened", &saved).unwrap();
    assert_eq!(
        reopened
            .serialize_project_file(0, head(&reopened).snapshot_id())
            .unwrap(),
        saved
    );
    assert_eq!(head(&reopened).document(), current.document());
    assert!(!zero.document().layers()[0].locked());
    assert_eq!(
        zero.document().layers()[0]
            .shy
            .value()
            .copied()
            .unwrap_or(false),
        false
    );
    assert!(current.document().layers()[0].locked());
    assert_eq!(current.document().layers()[0].shy.value(), Some(&true));
    assert_eq!(o.undo(2), Ok(3));
    assert_eq!(
        head(&o).document().layers()[0]
            .shy
            .value()
            .copied()
            .unwrap_or(false),
        false
    );
    assert_eq!(o.redo(3), Ok(4));
    assert_eq!(
        o.serialize_project_file(4, head(&o).snapshot_id()).unwrap(),
        saved
    );
}
