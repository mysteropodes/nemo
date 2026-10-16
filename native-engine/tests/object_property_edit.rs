//! Actual one-owner mutation, retention and receipt oracles for generic properties.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "property-edit",
        decode_storage_project(
            json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":20,
        "layers":[{"layerUid":"L"}],"objects":[]})
            .to_string()
            .as_bytes(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(
        &encode_storage_project(
            owner
                .acquire_snapshot(owner.content_revision())
                .unwrap()
                .document(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn payload(property: &str, action: Value) -> Value {
    json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},"property":property,"action":action})
}
fn apply(owner: &mut NativeObjectHistory, payload: Value) -> Result<u64, DispatchErrorCode> {
    let candidate = owner.prepare_property_edit_json(payload.to_string().as_bytes())?;
    owner.commit_property_edit(candidate)
}
fn set_value(frame: f64, value: f64) -> Value {
    json!({"kind":"setValue","frame":frame,"values":[value]})
}
fn set_key(frame: f64, value: f64) -> Value {
    json!({"kind":"setKey","frame":frame,"values":[value],"curvePoints":[]})
}
fn record(owner: &NativeObjectHistory) -> Value {
    wire(owner)["propertyHolders"][0]["properties"][0].clone()
}
#[test]
fn static_edit_is_atomic_persistent_and_undoable_without_mutating_old_snapshot() {
    let mut o = owner();
    let old = o.acquire_snapshot(0).unwrap();
    let old_bytes = encode_storage_project(old.document()).unwrap();
    assert_eq!(
        apply(&mut o, payload("opacity", set_value(0., 23.5))).unwrap(),
        1
    );
    assert_eq!(record(&o), json!({"property":"opacity","static":[23.5]}));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(encode_storage_project(old.document()).unwrap(), old_bytes);
    let saved = encode_storage_project(o.acquire_snapshot(1).unwrap().document()).unwrap();
    assert_eq!(
        encode_storage_project(&decode_storage_project(&saved).unwrap()).unwrap(),
        saved
    );
    assert_eq!(o.undo(1).unwrap(), 2);
    assert!(wire(&o).get("propertyHolders").is_none());
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(record(&o)["static"], json!([23.5]));
    let before = o.history_depths();
    assert_eq!(
        apply(&mut o, payload("opacity", set_value(0., 23.5))).unwrap(),
        3
    );
    assert_eq!(o.history_depths(), before);
}
#[test]
fn keys_are_sorted_replaced_in_place_and_preserve_authored_handles_hold_and_empty_track() {
    let mut o = owner();
    apply(&mut o, payload("opacity", set_key(4., 80.))).unwrap();
    apply(&mut o, payload("opacity", set_key(-0.5, 20.))).unwrap();
    assert_eq!(record(&o)["track"]["keys"][0]["frame"], json!(-0.5));
    assert_eq!(record(&o)["track"]["keys"][1]["frame"], json!(4.));
    apply(&mut o, payload("opacity", set_value(4., 75.))).unwrap();
    assert_eq!(record(&o)["track"]["keys"].as_array().unwrap().len(), 2);
    assert_eq!(record(&o)["track"]["keys"][1]["v"], json!([75.]));
    assert_eq!(record(&o)["track"]["keys"][1]["curvePoints"], json!([]));
    assert_eq!(record(&o)["track"]["keys"][1]["hIn"], json!([0, 0]));
    apply(
        &mut o,
        payload("opacity", json!({"kind":"removeKey","frame":-0.5})),
    )
    .unwrap();
    apply(
        &mut o,
        payload("opacity", json!({"kind":"removeKey","frame":4.})),
    )
    .unwrap();
    assert_eq!(record(&o)["track"], json!({"keys":[]}));
    let revision = o.content_revision();
    apply(
        &mut o,
        payload("opacity", json!({"kind":"removeKey","frame":999})),
    )
    .unwrap();
    assert_eq!(o.content_revision(), revision);
}
#[cfg(feature = "evaluation")]
#[test]
fn stopwatch_seeds_and_freezes_native_sample_with_one_history_entry_each() {
    let mut o = owner();
    apply(
        &mut o,
        payload(
            "opacity",
            json!({"kind":"setAnimated","frame":-0.5,"animated":true}),
        ),
    )
    .unwrap();
    assert_eq!(record(&o)["track"]["keys"][0]["v"][0].as_f64(), Some(100.));
    // Replace the outgoing default curve explicitly with linear and add endpoint.
    apply(&mut o, payload("opacity", set_key(-0.5, 20.))).unwrap();
    apply(&mut o, payload("opacity", set_key(3.5, 80.))).unwrap();
    let before = o.content_revision();
    let history = o.history_depths().0;
    apply(
        &mut o,
        payload(
            "opacity",
            json!({"kind":"setAnimated","frame":1.5,"animated":false}),
        ),
    )
    .unwrap();
    assert_eq!(o.content_revision(), before + 1);
    assert_eq!(o.history_depths(), (history + 1, 0));
    assert_eq!(record(&o)["track"], json!({"keys":[]}));
    assert_eq!(record(&o)["static"][0].as_f64(), Some(50.));
    o.undo(o.content_revision()).unwrap();
    assert_eq!(record(&o)["track"]["keys"].as_array().unwrap().len(), 2);
}
#[test]
fn invalid_edits_and_unavailable_families_do_not_allocate_history_or_revisions() {
    let mut o = owner();
    let original = wire(&o);
    for bad in [
        payload("position", set_value(0., 1.)),
        payload("unknown", set_value(0., 1.)),
        payload("order", set_value(0., 1.)),
        payload("timeRemap", set_key(0., 1.)),
        payload("timeLinkInOffset", set_key(0., 1.)),
        payload(
            "opacity",
            json!({"kind":"setKey","frame":0,"values":[1],"curvePoints":[{"x":0.7,"y":0},{"x":0.2,"y":1}]}),
        ),
    ] {
        assert!(apply(&mut o, bad).is_err());
        assert_eq!(wire(&o), original);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
    let mut missing = payload("opacity", set_value(0., 1.));
    missing["target"]["layerUid"] = json!("missing");
    assert_eq!(apply(&mut o, missing), Err(DispatchErrorCode::NotFound));
    let raw = r#"{"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},"property":"opacity","property":"rotation","action":{"kind":"setValue","frame":0,"values":[1]}}"#;
    assert_eq!(
        o.prepare_property_edit_json(raw.as_bytes()).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
}
#[test]
fn prepare_is_side_effect_free_and_stale_or_foreign_tokens_cannot_commit() {
    let mut o = owner();
    let bytes = payload("opacity", set_value(0., 25.)).to_string();
    let first = o.prepare_property_edit_json(bytes.as_bytes()).unwrap();
    let stale = first.clone();
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    o.commit_property_edit(first).unwrap();
    assert_eq!(
        o.commit_property_edit(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    let foreign = owner()
        .prepare_property_edit_json(bytes.as_bytes())
        .unwrap();
    assert_eq!(
        o.commit_property_edit(foreign),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(o.history_depths(), (1, 0));
}
fn request(o: &NativeObjectHistory, id: &str, payload: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),
        "expectedRevision":o.content_revision(),"operation":"command.document.object.property.edit","payload":payload})).unwrap()
}
#[test]
fn dispatch_replays_exact_receipt_and_rejects_changed_body_stale_and_cancelled_without_writes() {
    let mut o = owner();
    let r = request(
        &o,
        "property-request",
        payload("opacity", set_value(0., 25.)),
    );
    let first = o.try_dispatch_object_command(&r).unwrap().unwrap();
    assert!(first.ok);
    let replay = o.try_dispatch_object_command(&r).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    let mut changed = r.clone();
    changed.payload = payload("opacity", set_value(0., 26.));
    assert!(!o.try_dispatch_object_command(&changed).unwrap().unwrap().ok);
    let mut stale = r.clone();
    stale.request_id = "stale".into();
    assert!(!o.try_dispatch_object_command(&stale).unwrap().unwrap().ok);
    let mut cancelled = request(&o, "cancelled", payload("opacity", set_value(0., 26.)));
    cancelled.cancelled_before_dispatch = true;
    assert!(
        !o.try_dispatch_object_command(&cancelled)
            .unwrap()
            .unwrap()
            .ok
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
}

#[test]
fn replacing_existing_key_preserves_hold_and_handles_when_curve_is_not_supplied() {
    let mut value = wire(&owner());
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L"},
      "properties":[{"property":"position","track":{"keys":[
        {"frame":2,"v":[1,2],"hold":true,"curvePoints":[],"hIn":[3,4],"hOut":[5,6]}]}}]}]);
    let mut o = NativeObjectHistory::new(
        "property-edit",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    apply(
        &mut o,
        payload(
            "position",
            json!({"kind":"setValue","frame":2,"values":[8,9]}),
        ),
    )
    .unwrap();
    assert_eq!(
        record(&o)["track"]["keys"][0],
        json!({"frame":2,"v":[8,9],"hold":true,
        "curvePoints":[],"hIn":[3,4],"hOut":[5,6]})
    );
}
#[test]
fn absent_stopwatch_off_and_missing_key_removal_preserve_absence_and_redo() {
    let mut o = owner();
    let original = wire(&o);
    apply(
        &mut o,
        payload(
            "position",
            json!({"kind":"setAnimated","frame":0,"animated":false}),
        ),
    )
    .unwrap();
    apply(
        &mut o,
        payload("position", json!({"kind":"removeKey","frame":0})),
    )
    .unwrap();
    assert_eq!(wire(&o), original);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    apply(&mut o, payload("opacity", set_value(0., 25.))).unwrap();
    o.undo(1).unwrap();
    apply(
        &mut o,
        payload("position", json!({"kind":"removeKey","frame":0})),
    )
    .unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(o.content_revision(), 2);
}
#[cfg(feature = "evaluation")]
#[test]
fn active_expression_freeze_requires_native_evaluator_and_preserves_authored_state() {
    let mut value = wire(&owner());
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L"},
      "properties":[{"property":"opacity","expression":{"enabled":true,"code":"42"},
        "track":{"keys":[{"frame":0,"v":[50]}]}}]}]);
    let mut o = NativeObjectHistory::new(
        "property-edit",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        apply(
            &mut o,
            payload(
                "opacity",
                json!({"kind":"setAnimated","frame":0,"animated":false})
            )
        ),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(wire(&o), value);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
}
