//! Public shape admission does not invent metadata, dimensions or native availability.
use crate::contract::{NativeApplicationRequest, MAX_SAFE_REVISION};
use crate::native_contract::property_edit::{request, result, revision_matches, OP};
use serde_json::{json, Value};
fn payload(action: Value) -> Value {
    json!({"command":"object.property.edit","target":{"contextId":"data:context 雪","layerUid":"blob:layer / α","strokeId":"data:stroke"},"property":"blob:property","action":action})
}
fn envelope(payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"edit","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":OP,"payload":payload}),
    )
    .unwrap()
}
#[test]
fn property_edit_four_actions_accept_finite_frames_dimensions_and_opaque_selectors() {
    for frame in [-1.25, 0.0, 1.5, 4294967296.0] {
        let mut actions = vec![json!({"kind":"removeKey","frame":frame})];
        for animated in [false, true] {
            actions.push(json!({"kind":"setAnimated","frame":frame,"animated":animated}));
        }
        for kind in ["setValue", "setKey"] {
            for values in [
                json!([0]),
                json!([-10, 2.5]),
                json!([1, 2, 3]),
                json!([0, 1, 2, 3]),
            ] {
                actions.push(json!({"kind":kind,"frame":frame,"values":values}));
            }
        }
        for action in actions {
            let p = payload(action);
            assert!(request(&p));
            assert!(envelope(p).validate().is_ok());
        }
    }
    let mut no_stroke = payload(json!({"kind":"removeKey","frame":0}));
    no_stroke["target"]
        .as_object_mut()
        .unwrap()
        .remove("strokeId");
    assert!(envelope(no_stroke).validate().is_ok());
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .unwrap();
    assert_eq!(descriptor["id"], "native.property-edit");
    assert_eq!(descriptor["authority"]["fallback"], "none");
}
#[test]
fn property_edit_payload_target_and_all_action_variants_are_closed_map_only_shapes() {
    for action in [
        json!({"kind":"setValue","frame":0,"values":[1]}),
        json!({"kind":"setKey","frame":-0.5,"values":[1]}),
        json!({"kind":"removeKey","frame":0}),
        json!({"kind":"setAnimated","frame":0,"animated":false}),
    ] {
        let valid = payload(action);
        for pointer in ["", "/target", "/action"] {
            for key in valid.pointer(pointer).unwrap().as_object().unwrap().keys() {
                let mut bad = valid.clone();
                bad.pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                // Only strokeId is optional; null is always refused.
                assert_eq!(request(&bad), pointer == "/target" && key == "strokeId");
                let mut bad = valid.clone();
                bad.pointer_mut(pointer).unwrap()[key] = Value::Null;
                assert!(!request(&bad), "null {pointer}/{key}");
            }
            for replacement in [json!([]), json!(false), json!("object"), Value::Null] {
                let mut bad = valid.clone();
                *bad.pointer_mut(pointer).unwrap() = replacement;
                assert!(!request(&bad));
            }
            let mut bad = valid.clone();
            bad.pointer_mut(pointer).unwrap()["extra"] = json!(0);
            assert!(!request(&bad));
            assert!(envelope(bad).validate().is_err());
        }
        for key in ["contextId", "layerUid", "strokeId"] {
            let mut bad = valid.clone();
            bad["target"][key] = json!("");
            assert!(!request(&bad));
        }
        let mut bad = valid.clone();
        bad["property"] = json!("");
        assert!(!request(&bad));
        let mut bad = valid.clone();
        bad["command"] = json!("object.property.track.edit");
        assert!(!request(&bad));
        for frame in [Value::Null, json!("0"), json!(false), json!([])] {
            let mut bad = valid.clone();
            bad["action"]["frame"] = frame;
            assert!(!request(&bad));
        }
        let mut missing = envelope(valid);
        missing.expected_revision = None;
        assert!(missing.validate().is_err());
    }
    for action in [
        json!({"kind":"setValue","frame":0,"values":[]}),
        json!({"kind":"setKey","frame":0,"values":[1,2,3,4,5]}),
        json!({"kind":"setKey","frame":0,"values":[null]}),
        json!({"kind":"setValue","frame":0,"values":[true]}),
        json!({"kind":"setValue","frame":0,"values":["1"]}),
        json!({"kind":"setKey","frame":0,"values":{}}),
        json!({"kind":"setAnimated","frame":0,"animated":0}),
        json!({"kind":"setAnimated","frame":0,"animated":"true"}),
        json!({"kind":"setKey","frame":0,"values":[1],"curvePoints":[]}),
        json!({"kind":"removeKey","frame":0,"values":[1]}),
        json!({"kind":"setAnimated","frame":0,"animated":true,"values":[1]}),
        json!(["setKey", 0, [1]]),
        json!({"kind":"preset","preset":"linear"}),
    ] {
        assert!(!request(&payload(action)));
    }
    let mut extra = envelope(payload(json!({"kind":"removeKey","frame":0})));
    extra.payload["imageData"] = json!("data:forbidden");
    assert!(extra.validate().is_err());
}
#[test]
fn property_edit_receipts_bind_exact_history_and_safe_revision_without_overflow() {
    let yes = json!({"applied":true,"historyEntriesAdded":1});
    let no = json!({"applied":false,"historyEntriesAdded":0});
    assert!(revision_matches(Some(7), 8, &yes));
    assert!(revision_matches(Some(7), 7, &no));
    assert!(revision_matches(
        Some(MAX_SAFE_REVISION),
        MAX_SAFE_REVISION,
        &no
    ));
    for (before, after, value) in [
        (Some(7), 7, yes.clone()),
        (Some(7), 8, no),
        (None, 8, yes.clone()),
        (Some(u64::MAX), 0, yes.clone()),
        (Some(MAX_SAFE_REVISION), MAX_SAFE_REVISION + 1, yes),
    ] {
        assert!(!revision_matches(before, after, &value));
    }
    for bad in [
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":1,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":2}),
        json!({"applied":true}),
        json!({"applied":true,"historyEntriesAdded":1,"extra":0}),
        json!([true, 1]),
    ] {
        assert!(!result(&bad));
    }
}

#[test]
fn property_bake_admits_only_the_intent_not_samples_or_replacement_keys() {
    let p = payload(json!({"kind":"bake"}));
    assert!(request(&p));
    assert!(envelope(p).validate().is_ok());
    for action in [
        json!({"kind":"bake","frame":0}),
        json!({"kind":"bake","values":[1,2]}),
        json!({"kind":"bake","samples":[[1,2]]}),
        json!({"kind":"bake","tolerance":100}),
        json!(["bake"]),
    ] {
        assert!(!request(&payload(action.clone())));
        assert!(envelope(payload(action)).validate().is_err());
    }
}
