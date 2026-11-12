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
fn same_colour_neighbour_is_absorbed_through_the_real_union_in_one_history_entry() {
    // Wall square 0..60 x 0..40 (stroked, unfilled) holds an unstroked same-colour (#336699ff) island
    // 30..50 x 10..30. A click at (10,10) traces the wall interior minus the island; the island touches
    // that region, so the baseline fillMergeSameColor unites them into ONE solid 0..60 x 0..40 record
    // and removes the island.
    let wall = f::vector(
        "L0",
        "wall",
        0,
        &f::rect(0.0, 0.0, 60.0, 40.0),
        &f::VectorSpec {
            fill: None,
            stroke: Some(f::BLUE),
            ..Default::default()
        },
    );
    let island = f::vector(
        "L0",
        "island",
        0,
        &f::rect(30.0, 10.0, 20.0, 20.0),
        &f::VectorSpec {
            fill: Some([0.2, 0.4, 0.6, 1.0]),
            ..Default::default()
        },
    );
    let mut owner = f::owner_of(&f::document(2, vec![wall, island], None, None));
    let before = f::bytes(&owner);
    let mut click = payload();
    click["point"] = json!([10, 10]);
    paint(&mut owner, &click);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let state = f::state(&owner);
    let objects = state["objects"].as_array().unwrap();
    let ids: Vec<&Value> = objects.iter().map(|r| &r["target"]["strokeId"]).collect();
    assert_eq!(ids.len(), 2, "wall + one merged fill, got {ids:?}");
    assert!(
        ids.iter().all(|id| *id != "island"),
        "absorbed island removed: {ids:?}"
    );
    assert!(ids.contains(&&json!("wall")));
    let merged = fill(&state);
    let axis = |k: &str| -> (f64, f64) {
        let v: Vec<f64> = merged["geometry"]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["point"][k].as_f64().unwrap())
            .collect();
        (
            v.iter().cloned().fold(f64::INFINITY, f64::min),
            v.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    assert_eq!((axis("x"), axis("y")), ((0.0, 60.0), (0.0, 40.0)));
    assert_eq!(
        merged["vectorExt"]["fillTrace"]["seeds"],
        json!([[10.0, 10.0]])
    );
    let edited = f::bytes(&owner);
    assert_eq!(f::bytes(&f::owner_of(&state)), edited);
    owner.undo(1).unwrap();
    assert_eq!(f::bytes(&owner), before);
    owner.redo(2).unwrap();
    assert_eq!(f::bytes(&owner), edited);
}

#[test]
fn a_cancelled_fill_trace_request_publishes_nothing_and_keeps_redo() {
    // Same envelope control as apply/erase/flatten: prime one redo entry, then dispatch the real fill.trace
    // request with cancelledBeforeDispatch; bytes, revision and both stacks must be untouched.
    let mut owner = f::owner_of(&document());
    paint(&mut owner, &payload());
    owner.undo(1).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let frozen = |o: &NativeObjectHistory| (f::bytes(o), o.history_depths(), o.content_revision());
    let before = frozen(&owner);
    let mut req = json!({"apiVersion":2,"requestId":"fill-cancel","instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.fill.trace","payload":payload()});
    req["cancelledBeforeDispatch"] = json!(true);
    let reply = f::dispatch(&mut owner, &req);
    assert_eq!(
        (reply["ok"].clone(), reply["error"]["code"].clone()),
        (json!(false), json!("cancelled_before_dispatch"))
    );
    assert_eq!(frozen(&owner), before);
    // the same request without the cancel flag is accepted, so the refusal was the cancel's doing
    req["cancelledBeforeDispatch"] = json!(false);
    req["requestId"] = json!("fill-ok");
    let accepted = f::dispatch(&mut owner, &req);
    assert_eq!(accepted["ok"], true, "{accepted}");
    assert_eq!(owner.history_depths().0, 1);
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

#[test]
fn a_same_colour_fill_brush_shape_the_trace_did_not_use_is_kept_apart() {
    // The same wall and island as the absorbed case above, but the island is a Fill Brush shape (serP's
    // isFillShape): fillMergeSameColor(layer, fill, false) skips it unless its stroke id is one of the fill's walls,
    // and the traced walls are only ["wall"]. Two fills stay side by side.
    let wall = f::vector("L0", "wall", 0, &f::rect(0.0, 0.0, 60.0, 40.0),
        &f::VectorSpec { fill: None, stroke: Some(f::BLUE), ..Default::default() });
    let mut island = f::vector("L0", "island", 0, &f::rect(30.0, 10.0, 20.0, 20.0),
        &f::VectorSpec { fill: Some([0.2, 0.4, 0.6, 1.0]), ..Default::default() });
    island["vectorExt"] = json!({"isFillShape": true});
    let mut owner = f::owner_of(&f::document(2, vec![wall, island], None, None));
    let mut click = payload();
    click["point"] = json!([10, 10]);
    paint(&mut owner, &click);
    let state = f::state(&owner);
    let objects = state["objects"].as_array().unwrap();
    let ids: Vec<&Value> = objects.iter().map(|r| &r["target"]["strokeId"]).collect();
    assert_eq!(ids.len(), 3, "wall, the kept Fill Brush shape and the new fill: {ids:?}");
    let kept = objects.iter().find(|r| r["target"]["strokeId"] == "island").unwrap();
    assert_eq!(kept["vectorExt"], json!({"isFillShape": true}), "the shape keeps its flag");
    assert_eq!(fill(&state)["vectorExt"]["fillTrace"]["walls"], json!(["wall"]));
}
