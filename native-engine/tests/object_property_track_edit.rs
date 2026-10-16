//! Actual owner, history, codec and receipt controls for track edits.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};
fn target(layer: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer})
}
fn sel(layer: &str, property: &str, frame: Value) -> Value {
    json!({"target":target(layer),"property":property,"frame":frame})
}
fn edit(selections: Vec<Value>, action: Value) -> Value {
    json!({"command":"object.property.track.edit","selections":selections,"action":action})
}
fn preset(name: &str) -> Value {
    json!({"kind":"preset","preset":name})
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
    "layers":[{"layerUid":"A","name":"first"},{"layerUid":"B","visible":false}],"objects":[],
    "propertyHolders":[
      {"target":target("A"),"properties":[
        {"property":"opacity","static":[17],"track":{"keys":[
          {"frame":0,"v":[0],"hold":false,"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hIn":[3,4],"hOut":[5,6]},
          {"frame":4,"v":[80],"curvePoints":[]},
          {"frame":12,"v":[0],"curvePoints":[]},
          {"frame":24,"v":[240]}]}},
        {"property":"rotation","track":{"keys":[{"frame":0,"v":[0],"hold":true},{"frame":12,"v":[90]}]}}]},
      {"target":target("B"),"properties":[{"property":"opacity","track":{"keys":[{"frame":0,"v":[7]},{"frame":12,"v":[9]}]}}]}
    ]})
}
fn from(value: Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "d12-owner",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(o)).unwrap()
}
fn track(o: &NativeObjectHistory, h: usize, r: usize) -> Value {
    wire(o)["propertyHolders"][h]["properties"][r]["track"].clone()
}
fn apply(o: &mut NativeObjectHistory, payload: Value) -> Result<u64, DispatchErrorCode> {
    let candidate = o
        .owner
        .prepare_property_track_edit_json(payload.to_string().as_bytes())?;
    o.commit_property_edit(candidate)
}
#[test]
fn stable_selection_groups_multi_track_preset_into_one_real_history_commit() {
    let initial = fixture();
    let mut o = from(initial.clone());
    let retained = o.acquire_snapshot(0).unwrap();
    let retained_bytes = encode_storage_project(retained.document()).unwrap();
    let p = edit(
        vec![
            sel("A", "rotation", json!(12)),
            sel("A", "opacity", json!(12)),
            sel("A", "opacity", json!(4)),
        ],
        preset("easeIn"),
    );
    let candidate = o
        .owner
        .prepare_property_track_edit_json(p.to_string().as_bytes())
        .unwrap();
    assert_eq!(bytes(&o), retained_bytes);
    assert_eq!(o.history_depths(), (0, 0));
    assert_eq!(o.commit_property_edit(candidate).unwrap(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    let got = wire(&o);
    assert_eq!(got["layers"], initial["layers"]);
    assert_eq!(got["objects"], initial["objects"]);
    assert_eq!(got["propertyHolders"][1], initial["propertyHolders"][1]);
    assert_eq!(
        got["propertyHolders"][0]["properties"][0]["static"],
        json!([17])
    );
    let keys = track(&o, 0, 0)["keys"].clone();
    assert_eq!(
        keys[0]["curvePoints"],
        json!([{"x":0,"y":0},{"x":0.25,"y":0.25},{"x":0.5,"y":0.5},{"x":0.75,"y":0.91},{"x":1.0,"y":1.0}])
    );
    assert_eq!(keys[0]["hIn"], json!([3, 4]));
    assert_eq!(keys[0]["hOut"], json!([5, 6]));
    assert!(keys[0]["hold"].as_bool() == Some(false));
    assert_eq!(keys[2]["curvePoints"], json!([]));
    assert_eq!(keys[2]["hold"], json!(false));
    assert_eq!(
        track(&o, 0, 1)["keys"][0]["hold"],
        json!(true),
        "previous Hold survives an incoming preset"
    );
    assert_eq!(
        encode_storage_project(retained.document()).unwrap(),
        retained_bytes
    );
    let saved = bytes(&o);
    assert_eq!(
        encode_storage_project(&decode_storage_project(&saved).unwrap()).unwrap(),
        saved
    );
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), initial);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(bytes(&o), saved);
}
#[test]
fn selected_first_ease_in_changes_presence_even_without_reported_segment_count() {
    let mut value = fixture();
    value["propertyHolders"][0]["properties"][0]["track"]["keys"][0]
        .as_object_mut()
        .unwrap()
        .remove("hold");
    let mut o = from(value);
    let before = track(&o, 0, 0)["keys"][0]["curvePoints"].clone();
    assert_eq!(
        apply(
            &mut o,
            edit(vec![sel("A", "opacity", json!(0))], preset("easeIn"))
        )
        .unwrap(),
        1
    );
    assert_eq!(track(&o, 0, 0)["keys"][0]["hold"], json!(false));
    assert_eq!(track(&o, 0, 0)["keys"][0]["curvePoints"], before);
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn equal_numeric_selector_and_preset_preserve_bytes_revision_and_real_redo() {
    let mut o = from(fixture());
    apply(
        &mut o,
        edit(vec![sel("A", "opacity", json!(0))], preset("hold")),
    )
    .unwrap();
    o.undo(1).unwrap();
    let before = bytes(&o);
    assert_eq!(
        apply(
            &mut o,
            edit(vec![sel("A", "opacity", json!(0.0))], preset("linear"))
        )
        .unwrap(),
        2
    );
    assert_eq!(bytes(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(track(&o, 0, 0)["keys"][0]["hold"], json!(true));
}
#[test]
fn influence_uses_selected_incoming_segment_and_does_not_accept_values() {
    let mut o = from(fixture());
    apply(
        &mut o,
        edit(
            vec![sel("A", "opacity", json!(12))],
            json!({"kind":"influence","side":"in","percent":50}),
        ),
    )
    .unwrap();
    assert_eq!(
        track(&o, 0, 0)["keys"][1]["curvePoints"],
        json!([
      {"x":0.0,"y":0.0},{"x":0.25,"y":0.25},{"x":0.5,"y":0.5},{"x":0.75,"y":0.875},{"x":1.0,"y":1.0}])
    );
    assert_eq!(track(&o, 0, 0)["keys"][1]["hold"], json!(false));
    assert_eq!(
        track(&o, 0, 0)["keys"][2],
        fixture()["propertyHolders"][0]["properties"][0]["track"]["keys"][2]
    );
    let before = bytes(&o);
    let mut bad = edit(vec![sel("A", "opacity", json!(12))], preset("linear"));
    bad["values"] = json!([99]);
    assert_eq!(apply(&mut o, bad), Err(DispatchErrorCode::InvalidRequest));
    assert_eq!(bytes(&o), before);
}
#[cfg(feature = "evaluation")]
#[test]
fn subdivide_samples_original_native_tracks_and_copies_selected_left_curve() {
    let initial = fixture();
    let mut o = from(initial.clone());
    // Deliberately unordered selection; unselected key 4 is the sampling left key
    // for frame 6 but selected key 0 is the copied-curve source.
    apply(
        &mut o,
        edit(
            vec![
                sel("A", "opacity", json!(24)),
                sel("A", "opacity", json!(0)),
                sel("A", "opacity", json!(12)),
                sel("A", "rotation", json!(0)),
                sel("A", "rotation", json!(12)),
            ],
            json!({"kind":"subdivide"}),
        ),
    )
    .unwrap();
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    let got = track(&o, 0, 0);
    let keys = got["keys"].as_array().unwrap();
    assert_eq!(
        keys.iter()
            .map(|k| k["frame"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![0., 4., 6., 12., 18., 24.]
    );
    assert_eq!(
        keys[2],
        json!({"frame":6.0,"v":[60.0],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hIn":[0,0],"hOut":[0,0]})
    );
    assert_eq!(
        keys[4],
        json!({"frame":18.0,"v":[120.0],"curvePoints":[],"hIn":[0,0],"hOut":[0,0]})
    );
    let r = track(&o, 0, 1);
    let middle = &r["keys"][1];
    assert_eq!(
        middle["v"][0].as_f64(),
        Some(0.),
        "raw held segment sampled by existing native owner"
    );
    assert!(middle.get("hold").is_none());
    assert_eq!(
        middle["curvePoints"],
        json!([{"x":0.0,"y":0.0},{"x":0.25,"y":0.156},{"x":0.5,"y":0.5},{"x":0.75,"y":0.844},{"x":1.0,"y":1.0}])
    );
    assert_eq!(
        wire(&o)["propertyHolders"][1],
        initial["propertyHolders"][1]
    );
    let saved = bytes(&o);
    assert_eq!(
        encode_storage_project(&decode_storage_project(&saved).unwrap()).unwrap(),
        saved
    );
}
#[test]
fn subdivision_collision_is_noop_and_active_expression_bake_is_atomic_refusal() {
    let mut o = from(fixture());
    let before = bytes(&o);
    // Existing midpoint12 collides: selected endpoints 0,24 produce no insertion.
    assert_eq!(
        apply(
            &mut o,
            edit(
                vec![
                    sel("A", "opacity", json!(0)),
                    sel("A", "opacity", json!(24))
                ],
                json!({"kind":"subdivide"})
            )
        )
        .unwrap(),
        0
    );
    assert_eq!(bytes(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let mut value = fixture();
    value["propertyHolders"][0]["properties"][1]["expression"] =
        json!({"enabled":true,"code":"42"});
    let mut o = from(value);
    let before = bytes(&o);
    let p = edit(
        vec![
            sel("A", "opacity", json!(0)),
            sel("A", "opacity", json!(12)),
            sel("A", "rotation", json!(0)),
            sel("A", "rotation", json!(12)),
        ],
        json!({"kind":"subdivide"}),
    );
    assert_eq!(apply(&mut o, p), Err(DispatchErrorCode::Unavailable));
    assert_eq!(bytes(&o), before);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    assert!(
        apply(
            &mut o,
            edit(vec![sel("A", "rotation", json!(0))], preset("easeOut"))
        )
        .is_ok(),
        "curve edits are not expression baking"
    );
}
#[test]
fn every_original_selector_validates_before_any_track_changes() {
    let mut o = from(fixture());
    let before = bytes(&o);
    for last in [
        sel("missing", "opacity", json!(0)),
        sel("A", "opacity", json!(99)),
        sel("A", "unknown", json!(0)),
        sel("A", "order", json!(0)),
        sel("A", "timeRemap", json!(0)),
        sel("A", "timeLinkInOffset", json!(0)),
        json!({"target":{"contextId":"other","layerUid":"A"},"property":"opacity","frame":0}),
        json!({"target":{"contextId":"scene-root","layerUid":"A","strokeId":"missing"},"property":"opacity","frame":0}),
    ] {
        assert!(apply(
            &mut o,
            edit(vec![sel("A", "rotation", json!(0)), last], preset("hold"))
        )
        .is_err());
        assert_eq!(bytes(&o), before);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
    let duplicate = edit(
        vec![
            sel("A", "opacity", json!(0)),
            sel("A", "opacity", json!(0.0)),
        ],
        preset("hold"),
    );
    assert_eq!(
        apply(&mut o, duplicate),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(bytes(&o), before);
}
#[test]
fn malformed_and_unbounded_inputs_refuse_without_history() {
    let mut o = from(fixture());
    let before = bytes(&o);
    let valid = edit(vec![sel("A", "opacity", json!(0))], preset("linear"));
    let mut cases = vec![
        edit(vec![], preset("hold")),
        edit(vec![sel("A", "opacity", json!(0)); 33], preset("hold")),
    ];
    for action in [
        json!({"kind":"preset","preset":"arbitrary"}),
        json!({"kind":"influence","side":"both","percent":50}),
        json!({"kind":"influence","side":"out","percent":"50"}),
        json!({"kind":"subdivide","values":[0]}),
        json!({"kind":"subdivide","keyIndex":0}),
        json!(["preset", "hold"]),
        json!(["influence", "out", 50]),
        json!(["subdivide"]),
    ] {
        cases.push(edit(vec![sel("A", "opacity", json!(0))], action));
    }
    let mut bad = valid.clone();
    bad["selections"][0]["frame"] = Value::Null;
    cases.push(bad);
    let mut bad = valid.clone();
    bad["selections"][0]["keyIndex"] = json!(0);
    cases.push(bad);
    for p in cases {
        assert_eq!(apply(&mut o, p), Err(DispatchErrorCode::InvalidRequest));
        assert_eq!(bytes(&o), before);
    }
    for action in [
        json!(["setValue", 0, [42]]),
        json!(["setKey", 0, [42], null]),
        json!(["removeKey", 0]),
        json!(["setAnimated", 0, false]),
    ] {
        let p = json!({"command":"object.property.edit","target":target("A"),
            "property":"opacity","action":action});
        assert_eq!(
            o.owner
                .prepare_property_edit_json(p.to_string().as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
        assert_eq!(bytes(&o), before);
    }
    let duplicate_action = r#"{"command":"object.property.track.edit","selections":[{"target":{"contextId":"scene-root","layerUid":"A"},"property":"opacity","frame":0}],"action":{"kind":"preset","preset":"hold","preset":"ease"}}"#;
    assert_eq!(
        o.owner
            .prepare_property_track_edit_json(duplicate_action.as_bytes())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    let raw = r#"{"command":"object.property.track.edit","selections":[{"target":{"contextId":"scene-root","layerUid":"A"},"property":"opacity","frame":0,"frame":4}],"action":{"kind":"preset","preset":"hold"}}"#;
    assert!(o
        .owner
        .prepare_property_track_edit_json(raw.as_bytes())
        .is_err());
    let mut huge = valid;
    huge["extra"] = json!("x".repeat(4096));
    assert!(apply(&mut o, huge).is_err());
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
}
#[test]
fn existing_sealed_property_token_guards_cancel_stale_and_foreign_commit() {
    let mut o = from(fixture());
    let before = bytes(&o);
    let p = edit(vec![sel("A", "opacity", json!(0))], preset("hold")).to_string();
    let dropped = o
        .owner
        .prepare_property_track_edit_json(p.as_bytes())
        .unwrap();
    drop(dropped);
    assert_eq!(bytes(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let first = o
        .owner
        .prepare_property_track_edit_json(p.as_bytes())
        .unwrap();
    let stale = first.clone();
    let foreign = from(fixture())
        .owner
        .prepare_property_track_edit_json(p.as_bytes())
        .unwrap();
    assert_eq!(o.commit_property_edit(first).unwrap(), 1);
    assert_eq!(
        o.commit_property_edit(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        o.commit_property_edit(foreign),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(o.history_depths(), (1, 0));
    let different = NativeObjectHistory::new(
        "different-instance",
        decode_storage_project(fixture().to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let foreign = different
        .owner
        .prepare_property_track_edit_json(p.as_bytes())
        .unwrap();
    assert_eq!(
        o.commit_property_edit(foreign),
        Err(DispatchErrorCode::WrongInstance)
    );
}
fn request(o: &NativeObjectHistory, id: &str, payload: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),
      "expectedRevision":o.content_revision(),"operation":"command.document.object.property.track.edit","payload":payload})).unwrap()
}
#[test]
fn existing_receipt_route_replays_exactly_and_refuses_changed_body_stale_cancelled() {
    // Requires the explicit dispatch wiring described in the handoff, no new receipt store.
    let mut o = from(fixture());
    let p = edit(vec![sel("A", "opacity", json!(0))], preset("hold"));
    let r = request(&o, "d12-replay", p.clone());
    let first = o.try_dispatch_object_command(&r).unwrap().unwrap();
    assert!(first.ok);
    let replay = o.try_dispatch_object_command(&r).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    let mut changed = r.clone();
    changed.payload["action"] = preset("ease");
    assert!(!o.try_dispatch_object_command(&changed).unwrap().unwrap().ok);
    let mut stale = r.clone();
    stale.request_id = "d12-stale".into();
    assert!(!o.try_dispatch_object_command(&stale).unwrap().unwrap().ok);
    let mut cancelled = request(&o, "d12-cancel", p);
    cancelled.cancelled_before_dispatch = true;
    assert!(
        !o.try_dispatch_object_command(&cancelled)
            .unwrap()
            .unwrap()
            .ok
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
}
