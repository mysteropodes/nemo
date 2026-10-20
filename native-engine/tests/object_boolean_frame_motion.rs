//! Actual production Flatten: native tracks -> geometry -> one native history entry.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn document(property: Value) -> Value {
    let records: Vec<Value> = [("a", 0.), ("b", 20.)].into_iter().map(|(id,x)| {
        let segments: Vec<_> = [[x,0.],[x+10.,0.],[x+10.,10.],[x,10.]].into_iter().map(|p|
            json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})).collect();
        json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","layerUid":"L0","strokeId":id,"frameScope":{"kind":"authored","frame":1}},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}})
    }).collect();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":[{"layerUid":"L0","groups":{"contextId":"scene-root","entries":[{"gid":"g","order":["a","b"],"combineMode":"unite"}],
            "members":[{"strokeId":"a","groupId":"g"},{"strokeId":"b","groupId":"g"}]}}],
        "objects":records,"propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},"properties":[property]}]})
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("flatten-motion",decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
fn payload() -> Vec<u8> {
    json!({"command":"object.boolean.flatten","contextId":"scene-root","layerUid":"L0","frame":1,"groupId":"g"}).to_string().into_bytes()
}

#[test]
fn actual_keyed_member_motion_is_baked_then_undo_and_reopen_preserve_it() {
    let mut o = owner(&document(json!({"property":"position","track":{"keys":[
        {"frame":0,"v":[0,0]}, {"frame":2,"v":[40,0]}]}})));
    let before = bytes(&o);
    let candidate = o.prepare_boolean_flatten_json(&payload()).unwrap();
    assert_eq!(bytes(&o), before, "preparation cannot publish");
    o.commit_boolean_edit(candidate).unwrap();
    assert_eq!(o.history_depths(), (1,0));
    let after = bytes(&o);
    let value: Value = serde_json::from_slice(&after).unwrap();
    assert_eq!(value["objects"].as_array().unwrap().len(), 1, "sampled +20 translation overlaps the second square exactly");
    for segment in value["objects"][0]["geometry"]["segments"].as_array().unwrap() {
        assert!(segment["point"]["x"].as_f64().unwrap() >= 20.);
        assert!(segment["point"]["x"].as_f64().unwrap() <= 30.);
    }
    let reopened = owner(&value);
    assert_eq!(bytes(&reopened), after);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), after);
}

#[test]
fn unsupported_or_expression_motion_leaves_source_revision_and_history_untouched() {
    for property in [
        json!({"property":"position","static":[20,0],"expression":{"enabled":true,"code":"[20,0]"}}),
        json!({"property":"rotationX","static":[30]})
    ] {
        let o = owner(&document(property));
        let before = bytes(&o);
        assert!(matches!(o.prepare_boolean_flatten_json(&payload()), Err(DispatchErrorCode::Unavailable)));
        assert_eq!(bytes(&o), before);
        assert_eq!(o.content_revision(), 0);
        assert_eq!(o.history_depths(), (0,0));
    }
}

#[test]
fn a_real_intervening_publication_rejects_the_sealed_motion_candidate() {
    let mut o = owner(&document(json!({"property":"position","static":[20,0]})));
    let stale = o.prepare_boolean_flatten_json(&payload()).unwrap();
    let current = o.prepare_boolean_flatten_json(&payload()).unwrap();
    o.commit_boolean_edit(current).unwrap();
    let after = bytes(&o);
    assert_eq!(o.commit_boolean_edit(stale), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(bytes(&o), after);
    assert_eq!(o.history_depths(), (1,0));
}
