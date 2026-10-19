//! Production paint-bucket/linked-wall transactions through the real native history.
//! Expected geometry, authored tags and revision counts are literal independent facts.
use crate::history::object_history::{object_boolean_ops_tests as f, NativeObjectHistory};
use crate::object_codec::decode_storage_project;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn document() -> Value {
    let wall = f::vector(
        "L0",
        "wall",
        0,
        &f::rect(0.0, 0.0, 40.0, 40.0),
        &f::VectorSpec {
            fill: None,
            stroke: Some(f::BLUE),
            ..Default::default()
        },
    );
    f::document(2, vec![wall], None, None)
}
fn payload() -> Value {
    json!({"command":"object.fill.trace","layerUid":"L0","contextId":"scene-root","frame":0,
        "point":[20,20],"color":"#336699ff","opacity":0.5,"remove":false})
}
fn paint(owner: &mut NativeObjectHistory, p: &Value) {
    let candidate = owner
        .prepare_fill_trace_json(p.to_string().as_bytes())
        .unwrap();
    owner.commit_boolean_edit(candidate).unwrap();
}
fn fill(state: &Value) -> &Value {
    state["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["vectorExt"]["fillTrace"].is_object())
        .unwrap()
}

#[test]
fn one_native_fill_commit_preserves_tags_history_and_fresh_codec_reopen() {
    let mut owner = f::owner_of(&document());
    let before = f::bytes(&owner);
    paint(&mut owner, &payload());
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    let state = f::state(&owner);
    let record = fill(&state);
    assert_eq!(record["target"]["strokeId"], "native-stroke:1:0");
    assert_eq!(
        record["vectorExt"]["fillTrace"],
        json!({"seed":[20.0,20.0],"walls":["wall"],"gapPx":0})
    );
    assert_eq!(record["vector"]["opacity"], 0.5);
    assert!(record["vector"]["stroke"].is_null());
    assert_eq!(record["geometry"]["segments"].as_array().unwrap().len(), 4);
    let edited = f::bytes(&owner);
    let reopened = f::owner_of(&state);
    assert_eq!(f::bytes(&reopened), edited);
    owner.undo(1).unwrap();
    assert_eq!(f::bytes(&owner), before);
    owner.redo(2).unwrap();
    assert_eq!(f::bytes(&owner), edited);
}

#[test]
fn replacing_a_stored_wall_regenerates_its_fill_in_the_same_undo_entry() {
    let mut owner = f::owner_of(&document());
    paint(&mut owner, &payload());
    let before = f::bytes(&owner);
    let state = f::state(&owner);
    let mut wall = state["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["target"]["strokeId"] == "wall")
        .unwrap()
        .clone();
    wall["geometry"]["segments"] = Value::Array(
        f::rect(0.0, 0.0, 60.0, 40.0)
            .into_iter()
            .map(f::corner)
            .collect(),
    );
    let candidate = owner
        .prepare_vector_edit_json(
            json!({"command":"object.vector.replace","record":wall})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    owner.commit_vector_edit(candidate).unwrap();
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (2, 0));
    let after = f::state(&owner);
    let filled = fill(&after);
    let xmax = filled["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["point"]["x"].as_f64().unwrap())
        .fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(xmax, 60.0);
    assert_eq!(filled["vectorExt"]["fillTrace"]["walls"], json!(["wall"]));
    assert_eq!(filled["target"]["strokeId"], "native-stroke:1:0");
    let after = f::bytes(&owner);
    owner.undo(2).unwrap();
    assert_eq!(f::bytes(&owner), before);
    owner.redo(3).unwrap();
    assert_eq!(f::bytes(&owner), after);
    let reopened = f::owner_of(&serde_json::from_slice::<Value>(&after).unwrap());
    assert_eq!(f::bytes(&reopened), after);
}

#[test]
fn stale_and_foreign_fill_candidates_publish_nothing_and_preserve_redo() {
    let mut owner = f::owner_of(&document());
    paint(&mut owner, &payload());
    let candidate = owner
        .prepare_fill_trace_json(payload().to_string().as_bytes())
        .unwrap();
    let mut red = payload();
    red["color"] = json!("#ff0000ff");
    paint(&mut owner, &red);
    owner.undo(2).unwrap();
    let before = f::bytes(&owner);
    let depths = owner.history_depths();
    assert_eq!(
        owner.commit_boolean_edit(candidate),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(f::bytes(&owner), before);
    assert_eq!(owner.history_depths(), depths);
    let other = NativeObjectHistory::new(
        "foreign-fill",
        decode_storage_project(document().to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let foreign = other
        .prepare_fill_trace_json(payload().to_string().as_bytes())
        .unwrap();
    assert_eq!(
        owner.commit_boolean_edit(foreign),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(f::bytes(&owner), before);
    assert_eq!(owner.history_depths(), depths);
}

#[test]
fn no_region_or_noop_recolour_preserves_bytes_revision_and_redo() {
    let mut owner = f::owner_of(&document());
    paint(&mut owner, &payload());
    let mut red = payload();
    red["color"] = json!("#ff0000ff");
    paint(&mut owner, &red);
    owner.undo(2).unwrap();
    let before = f::bytes(&owner);
    let depth = owner.history_depths();
    let revision = owner.content_revision();
    paint(&mut owner, &payload());
    let mut miss = payload();
    miss["point"] = json!([1000, 1000]);
    paint(&mut owner, &miss);
    assert_eq!(owner.content_revision(), revision);
    assert_eq!(owner.history_depths(), depth);
    assert_eq!(f::bytes(&owner), before);
}

#[test]
fn malformed_locked_or_wrong_scope_fill_requests_cannot_change_history() {
    let mut owner = f::owner_of(&document());
    let before = f::bytes(&owner);
    for (field, bad) in [
        ("point", json!([20])),
        ("point", json!([1e13, 0])),
        ("color", json!("blue")),
        ("opacity", json!(1.1)),
        ("remove", json!("true")),
        ("frame", json!(-1)),
        ("command", json!("object.fill.other")),
    ] {
        let mut p = payload();
        p[field] = bad;
        assert!(owner
            .prepare_fill_trace_json(p.to_string().as_bytes())
            .is_err());
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(f::bytes(&owner), before);
    }
    let mut locked = document();
    locked["layers"][0]["locked"] = json!(true);
    owner = f::owner_of(&locked);
    assert_eq!(
        owner
            .prepare_fill_trace_json(payload().to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn real_dispatch_replay_executes_the_fill_once() {
    let mut owner = f::owner_of(&document());
    let request = json!({"apiVersion":2,"requestId":"fill-once",
        "instanceId":owner.instance_id(),"documentId":owner.document_id(),"expectedRevision":0,
        "operation":"command.document.object.fill.trace","payload":payload()});
    let first = f::dispatch(&mut owner, &request);
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let after = f::bytes(&owner);
    let replay = f::dispatch(&mut owner, &request);
    assert_eq!(first, replay);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(f::bytes(&owner), after);
}

#[test]
fn removing_a_fill_keeps_its_identity_and_restores_it_with_one_undo() {
    let mut owner = f::owner_of(&document());
    paint(&mut owner, &payload());
    let before = f::bytes(&owner);
    let mut remove = payload();
    remove["remove"] = json!(true);
    paint(&mut owner, &remove);
    let state = f::state(&owner);
    let record = fill(&state);
    assert!(record["fill"].is_null());
    assert_eq!(record["target"]["strokeId"], "native-stroke:1:0");
    assert_eq!(owner.history_depths(), (2, 0));
    assert!(f::owner_of(&state).history_depths() == (0, 0));
    owner.undo(2).unwrap();
    assert_eq!(f::bytes(&owner), before);
}

fn held_document() -> Value {
    let mut d = document();
    d["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
        "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    d
}

#[test]
fn an_ordinary_held_cell_fill_promotes_and_paints_as_one_sealed_history_entry() {
    let d = held_document();
    let mut owner = f::owner_of(&d);
    let before = f::bytes(&owner);
    let mut p = payload();
    p["frame"] = json!(2);
    let candidate = owner
        .prepare_fill_trace_json(p.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        f::bytes(&owner),
        before,
        "preparation must not publish promotion"
    );
    owner.commit_boolean_edit(candidate).unwrap();
    let after = f::state(&owner);
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        after["layers"][0]["timing"]["keys"],
        json!([{"frame":0},{"frame":2}])
    );
    assert_eq!(
        f::frame_records(&after, "L0", 0),
        f::frame_records(&d, "L0", 0)
    );
    let records = f::frame_records(&after, "L0", 2);
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r["target"]["strokeId"] == "wall"));
    let painted = records
        .iter()
        .find(|r| r["vectorExt"]["fillTrace"].is_object())
        .unwrap();
    assert_eq!(painted["vectorExt"]["fillTrace"]["walls"], json!(["wall"]));
    let saved = f::bytes(&owner);
    assert_eq!(f::bytes(&f::owner_of(&after)), saved);
    owner.undo(1).unwrap();
    assert_eq!(f::bytes(&owner), before);
    owner.redo(2).unwrap();
    assert_eq!(f::bytes(&owner), saved);
}

#[test]
fn a_held_no_region_keeps_its_original_undoable_key_promotion() {
    let mut owner = f::owner_of(&held_document());
    let before = f::bytes(&owner);
    let mut p = payload();
    p["frame"] = json!(2);
    p["point"] = json!([1000, 1000]);
    paint(&mut owner, &p);
    let after = f::state(&owner);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        after["layers"][0]["timing"]["keys"],
        json!([{"frame":0},{"frame":2}])
    );
    assert_eq!(f::frame_records(&after, "L0", 2).len(), 1);
    owner.undo(1).unwrap();
    assert_eq!(f::bytes(&owner), before);
}
