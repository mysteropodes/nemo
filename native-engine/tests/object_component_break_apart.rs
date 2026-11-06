//! `object.component.breakApart` through the one history owner (#2256 D-comp-break-apart): the baseline
//! `convertComponentToLayer` bakes what the instance shows on every frame back into plain keys and detaches the layer.
//! One edit is one revision and one undo entry; the baked records equal the inner records when nothing places them, move
//! by exactly the instance matrix when one does, a held inner key collapses into one key plus holds, and every refusal
//! publishes nothing.
use super::fixture::{decode, document, layer, record};
use super::*;
use crate::object_codec::encode_storage_project;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

const OP: &str = "command.document.object.component.breakApart";

fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}
fn inner(stroke: &str, frame: u32) -> Value {
    let mut rec = record("xi", stroke, frame);
    rec["target"]["contextId"] = json!("sym_x");
    rec
}
fn base(instance_extra: Value, component_extra: Value) -> Value {
    let layers = [
        layer("ly_inst", "Inst", &[]),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    let mut instance = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"once",
        "symSpeed":1,"symPlacedAt":0,"symSingleFrame":0});
    for (key, value) in instance_extra.as_object().expect("object") {
        instance[key] = value.clone();
    }
    let mut component = json!({"symbolId":"sym_x","totalFrames":24,
        "layers":[{"layerUid":"xi","name":"Layer 1","visible":true,"locked":false,"timing":timing(&[0, 6])}],
        "objects":[inner("s0", 0), inner("s6", 6)]});
    for (key, value) in component_extra.as_object().expect("object") {
        component[key] = value.clone();
    }
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[component],"instances":[instance]});
    document(
        &layers,
        json!({"componentTables":{"components":components}}),
    )
}
fn payload(layer_uid: &str) -> Value {
    json!({"command":"object.component.breakApart","contextId":"scene-root","layerUid":layer_uid})
}
fn stored(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(snapshot.document()).unwrap()).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    encode_storage_project(snapshot.document()).unwrap()
}
fn send(
    owner: &mut NativeObjectHistory,
    payload: &Value,
) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
    owner.dispatch_component_setup(OP, payload)
}
fn records_of(doc: &Value, layer_uid: &str) -> Vec<Value> {
    doc["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["target"]["layerUid"] == layer_uid && r["target"]["contextId"] == "scene-root"
        })
        .cloned()
        .collect()
}
fn first_point(rec: &Value) -> [f64; 2] {
    let p = &rec["geometry"]["segments"][0]["point"];
    [p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()]
}
fn keys_of(doc: &Value, layer_uid: &str) -> Vec<u64> {
    let layer = doc["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == layer_uid)
        .unwrap();
    layer["timing"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect()
}

#[test]
fn the_instance_bakes_into_keys_where_the_content_changes_and_detaches() {
    let mut owner = NativeObjectHistory::new("break", decode(&base(json!({}), json!({})))).unwrap();
    let before = stored(&owner);
    assert!(records_of(&before, "ly_inst").is_empty());
    let reply = send(&mut owner, &payload("ly_inst")).unwrap();
    assert_eq!(
        reply,
        CompactResult::Command {
            applied: true,
            history_entries_added: 1
        }
    );
    assert_eq!(owner.history_depths(), (1, 0));
    let after = stored(&owner);
    // Frames 0..5 show inner key 0 and frames 6.. inner key 6: two keys, the held frames carry no record.
    assert_eq!(keys_of(&after, "ly_inst"), vec![0, 6]);
    let baked = records_of(&after, "ly_inst");
    assert_eq!(baked.len(), 2);
    let frames: Vec<(u64, &str)> = baked
        .iter()
        .map(|r| {
            (
                r["target"]["frameScope"]["frame"].as_u64().unwrap(),
                r["target"]["strokeId"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(frames, vec![(0, "s0"), (6, "s6")]);
    let source = inner("s0", 0);
    assert_eq!(first_point(&baked[0]), first_point(&source));
    assert_eq!(baked[0]["geometry"], source["geometry"]);
    // The instance record is gone, the layer is unlocked, the component stays for any other instance.
    let set = &after["componentTables"]["components"];
    assert!(set["instances"].as_array().unwrap().is_empty());
    assert_eq!(set["components"].as_array().unwrap().len(), 1);
    let host = after["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == "ly_inst")
        .unwrap();
    assert_eq!(host["locked"], json!(false));
    // The untouched layer is untouched.
    assert_eq!(
        records_of(&after, "ly_plain"),
        records_of(&before, "ly_plain")
    );
}

#[test]
fn undo_and_redo_restore_the_exact_bytes_and_the_result_reopens() {
    let mut owner = NativeObjectHistory::new("break", decode(&base(json!({}), json!({})))).unwrap();
    let original = bytes(&owner);
    send(&mut owner, &payload("ly_inst")).unwrap();
    let after = bytes(&owner);
    assert_ne!(after, original);
    owner.undo(1).unwrap();
    assert_eq!(bytes(&owner), original);
    owner.redo(2).unwrap();
    assert_eq!(bytes(&owner), after);
    let reopened =
        crate::object_codec::decode_storage_project(&after).expect("the baked document reopens");
    assert_eq!(encode_storage_project(&reopened).unwrap(), after);
}

#[test]
fn the_instance_matrix_moves_every_baked_point_exactly() {
    let mut owner = NativeObjectHistory::new(
        "break",
        decode(&base(json!({"symMatrix":[2,0,0,3,10,20]}), json!({}))),
    )
    .unwrap();
    send(&mut owner, &payload("ly_inst")).unwrap();
    let baked = records_of(&stored(&owner), "ly_inst");
    let [x, y] = first_point(&inner("s0", 0));
    assert_eq!(first_point(&baked[0]), [x * 2.0 + 10.0, y * 3.0 + 20.0]);
    // A handle takes the linear part only.
    let source = &inner("s0", 0)["geometry"]["segments"][0]["handleOut"];
    let moved = &baked[0]["geometry"]["segments"][0]["handleOut"];
    assert_eq!(
        moved["x"].as_f64().unwrap(),
        source["x"].as_f64().unwrap() * 2.0
    );
    assert_eq!(
        moved["y"].as_f64().unwrap(),
        source["y"].as_f64().unwrap() * 3.0
    );
}

#[test]
fn single_frame_playback_bakes_one_held_key() {
    let mut owner = NativeObjectHistory::new(
        "break",
        decode(&base(
            json!({"symPlayMode":"single","symSingleFrame":6}),
            json!({}),
        )),
    )
    .unwrap();
    send(&mut owner, &payload("ly_inst")).unwrap();
    let after = stored(&owner);
    assert_eq!(keys_of(&after, "ly_inst"), vec![0]);
    let baked = records_of(&after, "ly_inst");
    assert_eq!(baked.len(), 1);
    assert_eq!(baked[0]["target"]["strokeId"], "s6");
    assert_eq!(baked[0]["target"]["frameScope"]["frame"], 0);
}

#[test]
fn a_placed_offset_shifts_the_key_and_holds_the_first_inner_frame_before_it() {
    // Placed at 4: frames 0..9 show inner key 0 (the playback clamps before the placement), frame 10 shows inner 6.
    let mut owner =
        NativeObjectHistory::new("break", decode(&base(json!({"symPlacedAt":4}), json!({}))))
            .unwrap();
    send(&mut owner, &payload("ly_inst")).unwrap();
    let after = stored(&owner);
    assert_eq!(keys_of(&after, "ly_inst"), vec![0, 10]);
    let ids: Vec<(u64, String)> = records_of(&after, "ly_inst")
        .iter()
        .map(|r| {
            (
                r["target"]["frameScope"]["frame"].as_u64().unwrap(),
                r["target"]["strokeId"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(ids, vec![(0, "s0".to_owned()), (10, "s6".to_owned())]);
}

#[test]
fn a_held_blank_key_bakes_a_blank_key() {
    // The outer layer holds a blank override from frame 3: the baked layer is empty from there, and the blank is a key.
    let extra = json!({"frameKeys":[{"frame":3,"blankOverride":true}]});
    let mut doc = base(extra, json!({}));
    // The key must sit on a timing key of the instance layer.
    doc["layers"][0]["timing"]["keys"] = json!([{"frame":0},{"frame":3}]);
    let mut owner = NativeObjectHistory::new("break", decode(&doc)).unwrap();
    send(&mut owner, &payload("ly_inst")).unwrap();
    let after = stored(&owner);
    assert_eq!(keys_of(&after, "ly_inst"), vec![0, 3]);
    let frames: Vec<u64> = records_of(&after, "ly_inst")
        .iter()
        .map(|r| r["target"]["frameScope"]["frame"].as_u64().unwrap())
        .collect();
    assert_eq!(frames, vec![0]);
}

#[test]
fn refusals_publish_nothing() {
    let mut owner = NativeObjectHistory::new("break", decode(&base(json!({}), json!({})))).unwrap();
    let original = bytes(&owner);
    let cases: Vec<(Value, DispatchErrorCode)> = vec![
        (payload("ly_plain"), DispatchErrorCode::NotFound),
        (payload("missing"), DispatchErrorCode::NotFound),
        (payload("xi"), DispatchErrorCode::NotFound),
        (
            json!({"command":"object.component.breakApart","contextId":"sym_x","layerUid":"ly_inst"}),
            DispatchErrorCode::Unavailable,
        ),
        (
            json!({"command":"object.component.convert","contextId":"scene-root","layerUid":"ly_inst"}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.component.breakApart","contextId":"scene-root"}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.component.breakApart","contextId":"scene-root","layerUid":"ly_inst","extra":1}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.component.breakApart","contextId":"scene-root","layerUid":""}),
            DispatchErrorCode::InvalidRequest,
        ),
        (json!([1]), DispatchErrorCode::InvalidRequest),
    ];
    for (body, code) in cases {
        assert_eq!(send(&mut owner, &body).unwrap_err().0, code, "{body}");
        assert_eq!(bytes(&owner), original, "{body}");
        assert_eq!(owner.history_depths(), (0, 0), "{body}");
    }
}

#[test]
fn a_component_with_inner_motion_is_refused_whole_and_typed() {
    let mut doc = base(json!({}), json!({}));
    doc["propertyHolders"] = json!([{"target":{"contextId":"sym_x","layerUid":"xi"},"properties":[{"property":"opacity","static":[50]}]}]);
    let mut owner = NativeObjectHistory::new("break", decode(&doc)).unwrap();
    let original = bytes(&owner);
    let refusal = send(&mut owner, &payload("ly_inst")).unwrap_err();
    assert_eq!(refusal.0, DispatchErrorCode::Unavailable);
    assert!(
        refusal.1.starts_with("component-break-apart"),
        "{}",
        refusal.1
    );
    assert_eq!(bytes(&owner), original);
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn a_baseline_symbol_that_stored_empty_owned_tables_opens_and_survives_save_and_reload() {
    // enterSymbol wrote `sym.cameraKeys = []`, `sym.markers = []` and three empty maps on every entered symbol.
    let extra =
        json!({"cameraKeys":[],"markers":[],"motionArcs":{},"tweenOverrides":{},"tweenEasing":{}});
    let mut owner = NativeObjectHistory::new("break", decode(&base(json!({}), extra))).unwrap();
    let before = bytes(&owner);
    let reopened = crate::object_codec::decode_storage_project(&before)
        .expect("empty owned tables are admitted");
    assert_eq!(encode_storage_project(&reopened).unwrap(), before);
    let component = &stored(&owner)["componentTables"]["components"]["components"][0];
    assert_eq!(component["cameraKeys"], json!([]));
    assert_eq!(component["motionArcs"], json!({}));
    // And a break apart leaves the component's own tables exactly as they were.
    send(&mut owner, &payload("ly_inst")).unwrap();
    assert_eq!(
        &stored(&owner)["componentTables"]["components"]["components"][0],
        component
    );
}
