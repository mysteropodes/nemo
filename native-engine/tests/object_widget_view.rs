//! #2255: the widget presentation view, over widgets created and edited by the owner itself.
use super::object_widget_edit_rows::*;
use super::*;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};

const PROOF: (&str, u64, &str, u32) = ("native-work-4", 2, "scene-root", 0);

fn query(snapshot: &ObjectSnapshot, cursor: u64, limit: u64, frame: u32) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"widget-read",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"view":{"kind":"widget","contextId":"scene-root",
                "frame":frame,"workId":"native-work-4","viewGeneration":2,"cursor":cursor,"limit":limit}}}))
    .unwrap()
}
fn read_with(
    owner: &NativeObjectHistory,
    cursor: u64,
    limit: u64,
    presented: Result<(&str, u64, &str, u32), &str>,
) -> ResponseEnvelope {
    let snapshot = current(owner);
    let response = snapshot
        .dispatch_with_widget_view(query(&snapshot, cursor, limit, 0), presented)
        .unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response
}
fn read(owner: &NativeObjectHistory) -> Value {
    let response = read_with(owner, 0, 4, Ok(PROOF));
    assert!(response.ok, "{:?}", response.error);
    response.result.unwrap()
}
fn code_of(response: ResponseEnvelope) -> DispatchErrorCode {
    assert!(!response.ok && response.result.is_none());
    response.error.unwrap().code()
}
fn near(value: &Value, want: &[f64]) {
    let got: Vec<f64> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(got.len(), want.len(), "{value}");
    for (a, b) in got.iter().zip(want) {
        assert!((a - b).abs() < 1e-9, "{value} vs {want:?}");
    }
}
fn set_value(owner: &mut NativeObjectHistory, id: &str, property: &str, values: Value) {
    property_edit(
        owner,
        id,
        property,
        json!({"kind":"setValue","frame":0,"values":values}),
    );
}

#[test]
fn a_fresh_joystick_reads_its_pad_puck_rest_and_axes() {
    let owner = joystick(Some(json!({"size":[200,120]})));
    let result = read(&owner);
    assert_eq!(result["kind"], "widget");
    assert_eq!(
        (result["total"].clone(), result["nextCursor"].clone()),
        (json!(1), Value::Null)
    );
    let item = &result["items"][0];
    assert_eq!(item["layerUid"], UID);
    assert_eq!(
        (
            item["kind"].clone(),
            item["visible"].clone(),
            item["locked"].clone()
        ),
        (json!("joystick"), json!(true), json!(false))
    );
    near(&item["center"], &[960., 540.]);
    near(&item["size"], &[200., 120.]);
    near(&item["puck"], &[0., 0.]);
    near(&item["rest"], &[0., 0.]);
    assert_eq!(
        (item["angle"].as_f64(), item["unavailable"].clone()),
        (Some(0.0), Value::Null)
    );
    assert_eq!(item["x"]["key"], key(UID, "x").as_str());
    assert_eq!(
        (
            item["x"]["min"].as_f64(),
            item["x"]["max"].as_f64(),
            item["x"]["rest"].as_f64()
        ),
        (Some(-100.), Some(100.), Some(0.))
    );
    assert_eq!(
        (item["x"]["label"].clone(), item["x"]["value"].as_f64()),
        (json!("X"), Some(0.))
    );
    assert_eq!(item["y"]["label"], "Y");
}

#[test]
fn the_puck_follows_the_control_values_and_the_own_transform_places_the_pad() {
    let mut owner = joystick(Some(json!({"size":[200,200]})));
    set_value(&mut owner, "x", &key(UID, "x"), json!([50]));
    set_value(&mut owner, "y", &key(UID, "y"), json!([100]));
    set_value(&mut owner, "rot", "rotation", json!([30]));
    set_value(&mut owner, "pos", "position", json!([10, -20]));
    let item = read(&owner)["items"][0].clone();
    // x = 50 of -100..100 sits a quarter right of centre, y = 100 at the very top (Y is inverted).
    near(&item["puck"], &[50., -100.]);
    near(&item["center"], &[970., 520.]);
    assert_eq!(item["angle"].as_f64(), Some(30.0));
    assert_eq!(
        (item["x"]["value"].as_f64(), item["y"]["value"].as_f64()),
        (Some(50.), Some(100.))
    );
}

#[test]
fn a_slider_has_one_axis_a_flat_pad_and_no_y() {
    let mut owner = owner_of(&base(1));
    run_ok(
        &mut owner,
        "c",
        "create",
        create_payload("slider", "Slider 1", [100., 50.]),
    );
    let item = read(&owner)["items"][0].clone();
    assert_eq!(
        (item["kind"].clone(), item["y"].clone()),
        (json!("slider"), Value::Null)
    );
    near(&item["size"], &[200., 26.]);
    near(&item["center"], &[100., 50.]);
    assert_eq!(item["x"]["label"], "Value");
}

#[test]
fn a_null_parent_rotates_the_pad_about_its_stored_pivot() {
    let mut owner = owner_of(&base(2));
    run_ok(
        &mut owner,
        "c",
        "create",
        create_payload("joystick", "J", [960., 540.]),
    );
    run_ok(
        &mut owner,
        "s",
        "settings",
        settings_payload(UID, json!({"size":[200,200]})),
    );
    let mut wire = wire(&owner);
    wire["layers"][0]["isNullLayer"] = json!(true);
    wire["layers"][0]["nullPos"] = json!([100, 100]);
    wire["layers"][1]["parentLayerUid"] = json!("L0");
    wire["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(json!({"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"rotation","static":[90]}]}));
    let item = read(&owner_of(&wire))["items"][0].clone();
    assert_eq!(item["angle"].as_f64(), Some(90.0), "{item}");
    // (960,540) about (100,100) by 90 degrees: (860,440) becomes (-440,860).
    near(&item["center"], &[-340., 960.]);
    // The same parent without a Null pivot has bounds only the presented scene could know.
    wire["layers"][0]["isNullLayer"] = json!(false);
    let item = read(&owner_of(&wire))["items"][0].clone();
    assert_eq!(item["unavailable"], "parent_geometry");
    assert_eq!(
        (item["center"].clone(), item["puck"].clone()),
        (Value::Null, Value::Null)
    );
}

#[test]
fn a_transform_expression_is_a_typed_reason_not_a_guess() {
    let owner = joystick(None);
    let mut wire = wire(&owner);
    wire["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"position",
        "expression":{"enabled":true,"code":"[1, 2]"}}));
    let item = read(&owner_of(&wire))["items"][0].clone();
    assert_eq!(item["unavailable"], "expression", "{item}");
}

#[test]
fn pages_are_bounded_and_a_cursor_past_the_list_is_refused() {
    let mut owner = owner_of(&base(1));
    for n in 0..3 {
        run_ok(
            &mut owner,
            &format!("c{n}"),
            "create",
            create_payload("slider", "S", [10., 10.]),
        );
    }
    let first = read_with(&owner, 0, 2, Ok(PROOF)).result.unwrap();
    assert_eq!(
        (
            first["total"].clone(),
            first["nextCursor"].clone(),
            first["items"].as_array().unwrap().len()
        ),
        (json!(3), json!(2), 2)
    );
    let last = read_with(&owner, 2, 2, Ok(PROOF)).result.unwrap();
    assert_eq!(
        (
            last["nextCursor"].clone(),
            last["items"].as_array().unwrap().len()
        ),
        (Value::Null, 1)
    );
    assert_eq!(
        code_of(read_with(&owner, 4, 2, Ok(PROOF))),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(
        code_of(read_with(&owner, 0, 5, Ok(PROOF))),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(
        code_of(read_with(&owner, 0, 0, Ok(PROOF))),
        DispatchErrorCode::InvalidRequest
    );
}

#[test]
fn only_the_actual_presentation_may_read_and_a_changed_one_is_stale() {
    let owner = joystick(None);
    assert_eq!(
        code_of(read_with(&owner, 0, 1, Err("not presented"))),
        DispatchErrorCode::Unavailable
    );
    for changed in [
        ("native-work-5", 2, "scene-root", 0),
        ("native-work-4", 3, "scene-root", 0),
        ("native-work-4", 2, "other", 0),
        ("native-work-4", 2, "scene-root", 1),
    ] {
        assert_eq!(
            code_of(read_with(&owner, 0, 1, Ok(changed))),
            DispatchErrorCode::StaleRevision,
            "{changed:?}"
        );
    }
    let snapshot = current(&owner);
    let mut stale = query(&snapshot, 0, 1, 0);
    stale.payload["atRevision"] = json!(snapshot.content_revision() + 1);
    assert_eq!(
        code_of(
            snapshot
                .dispatch_with_widget_view(stale, Ok(PROOF))
                .unwrap()
        ),
        DispatchErrorCode::StaleRevision
    );
    let mut extra = query(&snapshot, 0, 1, 0);
    extra.payload["view"]["layerUid"] = json!(UID);
    assert_eq!(
        code_of(
            snapshot
                .dispatch_with_widget_view(extra, Ok(PROOF))
                .unwrap()
        ),
        DispatchErrorCode::InvalidRequest
    );
}

#[test]
fn a_read_changes_nothing_and_undo_moves_the_puck_back() {
    let mut owner = joystick(Some(json!({"size":[200,200]})));
    let before = (owner.content_revision(), owner.history_depths());
    let _ = read(&owner);
    assert_eq!((owner.content_revision(), owner.history_depths()), before);
    run_ok(
        &mut owner,
        "w",
        "write",
        write_payload(UID, json!(0), local([100., 0.])),
    );
    near(&read(&owner)["items"][0]["puck"], &[100., 0.]);
    owner.undo(owner.content_revision()).unwrap();
    near(&read(&owner)["items"][0]["puck"], &[0., 0.]);
}
