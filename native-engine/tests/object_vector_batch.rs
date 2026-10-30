//! `object.vector.batch` (#2361, #2362): ordered creates, replaces and deletes on one layer as one revision and one undo
//! entry, through the single-command kernels; a refused step refuses the whole batch.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn op_create(record: Value) -> Value {
    json!({"op":"create","record":record})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn ids_at(owner: &NativeObjectHistory, layer: &str, frame: u64) -> Vec<String> {
    objects(&state(owner))
        .iter()
        .filter(|r| r["target"]["layerUid"] == layer && r["target"]["frameScope"]["frame"].as_u64() == Some(frame))
        .map(|r| r["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_cut_is_one_revision_and_one_undo_entry() {
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, seeds(), None));
        let before = state(&owner);
        let response = send(&mut owner, "cut", batch(vec![
            json!({"op":"delete","stableTarget":target("L1", 0, "s-a")}),
            op_create(record_at("B", "L1", 0, "")),
            op_create(record_at("C", "L1", 0, "")),
        ]));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["contentRevision"], 1);
        assert_eq!(response["result"], json!({"applied":true,"historyEntriesAdded":1,"operation":"batch",
            "strokeId":"s-a","strokeIds":["s-a","native-stroke:1:0","native-stroke:1:1"]}));
        assert_eq!(ids_at(&owner, "L1", 0), ["native-stroke:1:0", "native-stroke:1:1"].map(String::from).to_vec());
        let after = state(&owner);
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(state(&owner), before, "v{version}: one undo restores every step");
        owner.redo(2).unwrap();
        assert_eq!(state(&owner), after);
        assert_eq!(state(&owner_of(&after)), after, "the saved document reopens unchanged");
    }
}

#[test]
fn replaces_and_copy_keyed_creates_across_frames_are_one_entry() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a"), record_at("C", "L1", 0, "s-c")], None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&doc);
    let mut moved = record_at("A", "L1", 0, "s-a");
    moved["geometry"]["segments"][0]["point"]["x"] = json!(-7);
    let response = send(&mut owner, "sculpt", batch(vec![json!({"op":"replace","record":moved})]));
    assert_eq!(response["result"]["operation"], "batch", "{response}");
    assert_eq!(objects(&state(&owner)).iter().find(|r| r["target"]["strokeId"] == "s-a").unwrap()["geometry"]["segments"][0]["point"]["x"], -7);
    let mut payload = batch(vec![op_create(record_at("B", "L1", 2, "")), op_create(record_at("B", "L1", 5, ""))]);
    payload["keyframe"] = json!("copy");
    let response = send(&mut owner, "multi", payload);
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(state(&owner)["layers"][0]["timing"]["keys"], json!([{"frame":0},{"frame":2},{"frame":5}]));
    for frame in [2, 5] {
        let cell = ids_at(&owner, "L1", frame);
        assert_eq!(cell.len(), 3, "frame {frame}: the two shown strokes copied, then the new one");
        assert_eq!(&cell[..2], &["s-a".to_string(), "s-c".to_string()]);
    }
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn any_refused_step_refuses_the_whole_batch() {
    let many: Vec<Value> = (0..65).map(|_| op_create(record_at("B", "L1", 0, ""))).collect();
    for payload in [
        batch(vec![]),
        batch(many),
        batch(vec![op_create(record_at("B", "L1", 0, "")), op_create(record_at("C", "L2", 0, ""))]),
        batch(vec![json!({"op":"move","record":record_at("B", "L1", 0, "")})]),
        batch(vec![op_create(record_at("B", "L1", 0, "")), json!({"op":"delete","stableTarget":target("L1", 0, "missing")})]),
        batch(vec![json!({"op":"delete","stableTarget":target("L1", 0, "s-a"),"placement":"behind"})]),
        { let mut p = batch(vec![op_create(record_at("B", "L1", 0, "")), json!({"op":"delete","stableTarget":target("L1", 0, "s-a")})]); p["trimEnds"] = json!(4); p },
        { let mut p = batch(vec![op_create(record_at("B", "L1", 0, ""))]); p["placement"] = json!("behind"); p },
        { let mut p = create(record_at("B", "L1", 0, "")); p["ops"] = json!([]); p },
    ] {
        let mut owner = owner_of(&document(1, seeds(), None));
        let before = state(&owner);
        let response = send(&mut owner, "bad", payload.clone());
        assert_eq!(response["ok"], false, "{payload}");
        assert_eq!((state(&owner), owner.history_depths()), (before, (0, 0)));
    }
}

#[test]
fn a_copy_keyed_cut_on_a_held_cell_keys_that_cell_and_leaves_the_key() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a"), record_at("C", "L1", 0, "s-c")], None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    let mut head = record_at("A", "L1", 3, "s-a");
    head["geometry"]["segments"][0]["point"]["x"] = json!(-7);
    let ops = vec![json!({"op":"replace","record":head}), json!({"op":"delete","stableTarget":target("L1", 3, "s-c")}),
        op_create(record_at("B", "L1", 3, ""))];
    let held = send(&mut owner, "held", batch(ops.clone()));
    assert_eq!(held["ok"], false, "without the copy key a held cell has no record to edit: {held}");
    let mut payload = batch(ops);
    payload["keyframe"] = json!("copy");
    let response = send(&mut owner, "knife", payload);
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(ids_at(&owner, "L1", 0), ["s-a", "s-c"].map(String::from).to_vec(), "the held key is untouched");
    assert_eq!(ids_at(&owner, "L1", 3), ["s-a", "native-stroke:1:0"].map(String::from).to_vec());
    let snapshot = state(&owner);
    let cut = objects(&snapshot);
    let at = |frame: u64| cut.iter().find(|r| r["target"]["strokeId"] == "s-a" && r["target"]["frameScope"]["frame"].as_u64() == Some(frame)).unwrap()["geometry"]["segments"][0]["point"]["x"].clone();
    assert_eq!((at(0) != json!(-7), at(3)), (true, json!(-7)));
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before, "one undo removes the key and the cut");
}

#[test]
fn a_create_keyed_with_its_own_copy_keys_only_its_cell_with_what_it_showed() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a")], None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&doc);
    let mut stamped = op_create(record_at("B", "L1", 4, ""));
    stamped["keyframe"] = json!("copy");
    let response = send(&mut owner, "multiframe", batch(vec![op_create(record_at("B", "L1", 2, "")), stamped]));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(ids_at(&owner, "L1", 2), ["native-stroke:1:0"].map(String::from).to_vec(), "the drawn frame takes Draw's empty key");
    assert_eq!(ids_at(&owner, "L1", 4), ["s-a", "native-stroke:1:1"].map(String::from).to_vec(), "a stamped frame keeps what it showed");
    let mut wrong = json!({"op":"delete","stableTarget":target("L1", 0, "s-a")});
    wrong["keyframe"] = json!("copy");
    assert_eq!(send(&mut owner, "wrong", batch(vec![wrong]))["ok"], false, "only a create carries its own key");
}

#[test]
fn a_copy_step_stamps_an_earlier_create_on_another_frame_as_that_create_would() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a")], None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let (mut by_record, mut by_copy) = (owner_of(&doc), owner_of(&doc));
    let mut stamped = op_create(record_at("B", "L1", 4, ""));
    stamped["keyframe"] = json!("copy");
    let sent = send(&mut by_record, "records", batch(vec![op_create(record_at("B", "L1", 2, "")), stamped]));
    let copied = send(&mut by_copy, "copy", batch(vec![op_create(record_at("B", "L1", 2, "")),
        json!({"op":"copy","fromStep":0,"frame":4,"keyframe":"copy"})]));
    assert_eq!(copied["ok"], true, "{copied}");
    assert_eq!(copied["result"], sent["result"], "the same receipt as the resent record");
    assert_eq!(state(&by_copy), state(&by_record), "the same document as the resent record");
    assert_eq!(ids_at(&by_copy, "L1", 4), ["s-a", "native-stroke:1:1"].map(String::from).to_vec());
    let drawn = || op_create(record_at("B", "L1", 2, ""));
    for (name, ops) in [
        ("forward", vec![drawn(), json!({"op":"copy","fromStep":2,"frame":4}), drawn()]),
        ("self", vec![drawn(), json!({"op":"copy","fromStep":1,"frame":4})]),
        ("no-frame", vec![drawn(), json!({"op":"copy","fromStep":0})]),
        ("negative-frame", vec![drawn(), json!({"op":"copy","fromStep":0,"frame":-1})]),
        ("with-record", vec![drawn(), json!({"op":"copy","fromStep":0,"frame":4,"record":record_at("B", "L1", 4, "")})]),
        ("create-with-fromstep", vec![drawn(), json!({"op":"create","record":record_at("B", "L1", 4, ""),"fromStep":0})]),
    ] {
        assert_eq!(send(&mut by_copy, name, batch(ops))["ok"], false, "{name} is refused");
    }
    let delete_then_copy = batch(vec![json!({"op":"delete","stableTarget":target("L1", 0, "s-a")}), json!({"op":"copy","fromStep":0,"frame":4})]);
    assert_eq!(send(&mut by_copy, "from-delete", delete_then_copy)["ok"], false, "only a create's record is copied");
}

#[test]
fn reshape_and_clone_keep_the_native_style_and_take_only_new_geometry() {
    let doc = document(2, vec![record_at("A", "L1", 0, "s-a")], None);
    let (mut by_steps, mut by_records) = (owner_of(&doc), owner_of(&doc));
    let source = objects(&state(&by_steps))[0].clone();
    let shape = |dx: f64| {
        let mut g = source["geometry"].clone();
        for s in g["segments"].as_array_mut().unwrap() { s["point"]["x"] = json!(s["point"]["x"].as_f64().unwrap() + dx); }
        g
    };
    let response = send(&mut by_steps, "cut", batch(vec![
        json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"geometry":shape(5.)}),
        json!({"op":"clone","stableTarget":target("L1", 0, "s-a"),"frame":0,"geometry":shape(50.)}),
    ]));
    assert_eq!(response["ok"], true, "{response}");
    let (mut head, mut tail) = (source.clone(), source.clone());
    head["geometry"] = shape(5.);
    tail["geometry"] = shape(50.);
    tail["target"]["strokeId"] = json!("");
    let sent = send(&mut by_records, "records", batch(vec![json!({"op":"replace","record":head}), op_create(tail)]));
    assert_eq!(response["result"], sent["result"], "the same receipt as sending both records");
    assert_eq!(state(&by_steps), state(&by_records), "the same document: style from the native record, geometry from the step");
    let mut other_space = shape(1.);
    other_space["coordinateSpace"] = json!("layer-local");
    let mut one_point = shape(1.);
    one_point["segments"].as_array_mut().unwrap().truncate(1);
    for (name, op) in [
        ("missing", json!({"op":"reshape","stableTarget":target("L1", 0, "s-x"),"geometry":shape(1.)})),
        ("reshape-placement", json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"geometry":shape(1.),"placement":"front"})),
        ("bad-frame", json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"geometry":shape(1.),"frame":-1})),
        ("clone-no-frame", json!({"op":"clone","stableTarget":target("L1", 0, "s-a"),"geometry":shape(1.)})),
        ("no-geometry", json!({"op":"reshape","stableTarget":target("L1", 0, "s-a")})),
        ("other-space", json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"geometry":other_space})),
        ("one-point", json!({"op":"clone","stableTarget":target("L1", 0, "s-a"),"frame":0,"geometry":one_point})),
        ("create-geometry", json!({"op":"create","record":record_at("B", "L1", 0, ""),"geometry":shape(1.)})),
    ] {
        assert_eq!(send(&mut by_steps, name, batch(vec![op]))["ok"], false, "{name} is refused");
    }
}

#[test]
fn a_reshape_on_a_held_cell_keys_that_cell_and_leaves_the_key() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a")], None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&doc);
    let key = objects(&state(&owner))[0].clone();
    let mut moved = key["geometry"].clone();
    moved["segments"][0]["point"]["x"] = json!(moved["segments"][0]["point"]["x"].as_f64().unwrap() + 7.);
    let mut payload = batch(vec![
        json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"geometry":moved,"frame":4}),
        json!({"op":"clone","stableTarget":target("L1", 0, "s-a"),"geometry":moved,"frame":4}),
    ]);
    payload["keyframe"] = json!("copy");
    let response = send(&mut owner, "held-cut", payload);
    assert_eq!(response["ok"], true, "{response}");
    let snapshot = state(&owner);
    let after = objects(&snapshot);
    let at = |frame: u64, id: &str| after.iter().find(|r| r["target"]["frameScope"]["frame"].as_u64() == Some(frame) && r["target"]["strokeId"] == id).cloned();
    assert_eq!(at(0, "s-a").unwrap()["geometry"], key["geometry"], "the held key is untouched");
    assert_eq!(at(4, "s-a").unwrap()["geometry"], moved, "the held cell's copy takes the new geometry under its own id");
    assert_eq!(ids_at(&owner, "L1", 4).len(), 2, "and the clone lands beside it");
}

#[test]
fn a_geometry_less_clone_stamps_any_stroke_shifted_and_renames_its_linked_fill_pair() {
    use super::create_many::pressure_record;
    let mut owner = owner_of(&document(1, seeds(), None));
    let companion = pressure_record([0.2, 0.4, 0.6], json!({"linkedFillId":"lf-pose","isLinkedFillCompanion":true}));
    let ribbon = pressure_record([0.1, 0.1, 0.1], json!({"linkedFillId":"lf-pose","pressure":{"centerSegments":[
        {"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
        {"point":{"x":40,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4}],
        "widthProfile":[{"t":0,"width":2},{"t":1,"width":4}]}}));
    let made = send(&mut owner, "pair", json!({"command":"object.vector.createMany","records":[companion, ribbon]}));
    assert_eq!(made["ok"], true, "{made}");
    let before = state(&owner);
    let stamp = |id: &str| json!({"op":"clone","stableTarget":target("L1", 0, id),"frame":0,"offset":[5,7]});
    let response = send(&mut owner, "pose", batch(vec![stamp("native-stroke:1:0"), stamp("native-stroke:1:1")]));
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    let pair: Vec<&Value> = objects(&after).iter().filter(|r| r["vectorExt"]["linkedFillId"] == "native-fill:1:0").collect();
    assert_eq!(pair.len(), 2, "the stamped ribbon and fill are one pair under a fresh link beside the source pair");
    assert_eq!(objects(&after).iter().filter(|r| r["vectorExt"]["linkedFillId"] == "lf-pose").count(), 2, "the source pair is untouched");
    let source = objects(&before).iter().find(|r| r["target"]["strokeId"] == "native-stroke:1:1").unwrap().clone();
    let copy = pair.iter().find(|r| r["vectorExt"]["pressure"].is_object()).unwrap();
    for (a, b) in source["geometry"]["segments"].as_array().unwrap().iter().zip(copy["geometry"]["segments"].as_array().unwrap()) {
        assert_eq!((b["point"]["x"].as_f64().unwrap() - a["point"]["x"].as_f64().unwrap(), b["point"]["y"].as_f64().unwrap() - a["point"]["y"].as_f64().unwrap()), (5.0, 7.0));
    }
    assert_eq!(copy["vectorExt"]["pressure"], source["vectorExt"]["pressure"], "the stored centreline is copied as the baseline stamp did");
    owner.undo(2).unwrap();
    assert_eq!(state(&owner), before, "one undo removes the stamp");
    for (name, ops) in [
        ("ribbon-alone", vec![stamp("native-stroke:1:1")]),
        ("offset-and-geometry", vec![json!({"op":"clone","stableTarget":target("L1", 0, "native-stroke:1:1"),"frame":0,"offset":[1,1],"geometry":source["geometry"].clone()})]),
        ("reshape-offset", vec![json!({"op":"reshape","stableTarget":target("L1", 0, "native-stroke:1:1"),"geometry":source["geometry"].clone(),"offset":[1,1]})]),
        ("no-frame", vec![json!({"op":"clone","stableTarget":target("L1", 0, "native-stroke:1:1")})]),
        ("missing", vec![json!({"op":"clone","stableTarget":target("L1", 0, "native-stroke:9:9"),"frame":0})]),
    ] {
        assert_eq!(send(&mut owner, name, batch(ops))["ok"], false, "{name} is refused");
    }
}

#[test]
fn a_centred_reshape_rebuilds_a_ribbon_and_resyncs_its_linked_fill_in_one_step() {
    use super::create_many::pressure_record;
    use crate::object_document::pressure_geometry::{profiled_outline, SegOut};
    let mut owner = owner_of(&document(1, seeds(), None));
    let companion = pressure_record([0.2, 0.4, 0.6], json!({"linkedFillId":"lf-sculpt","isLinkedFillCompanion":true}));
    let ribbon = pressure_record([0.1, 0.1, 0.1], json!({"linkedFillId":"lf-sculpt","pressure":{"centerSegments":[
        {"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
        {"point":{"x":40,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4}],
        "widthProfile":[{"t":0,"width":2},{"t":1,"width":4}]}}));
    let plain = record_at("A", "L1", 0, "");
    let made = send(&mut owner, "pair", json!({"command":"object.vector.createMany","records":[companion, ribbon, plain]}));
    assert_eq!(made["ok"], true, "{made}");
    let before = state(&owner);
    // Sculpt densified the centreline under the brush and pushed the new anchor down.
    let anchor = |x: f64, y: f64, w: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":-3,"y":0},"handleOut":{"x":3,"y":0},"width":w});
    let center = json!([anchor(0., 10., 2.), anchor(20., 18., 3.), anchor(40., 10., 4.)]);
    let sculpt = |c: Value| json!({"op":"reshape","stableTarget":target("L1", 0, "native-stroke:1:1"),"center":c});
    let response = send(&mut owner, "sculpt", batch(vec![sculpt(center.clone())]));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["strokeIds"], json!(["native-stroke:1:1"]), "the fill re-sync is not a step of its own");
    let after = state(&owner);
    let at = |id: &str| objects(&after).iter().find(|r| r["target"]["strokeId"] == id).unwrap().clone();
    let (ribbon, fill) = (at("native-stroke:1:1"), at("native-stroke:1:0"));
    assert_eq!(ribbon["vectorExt"]["pressure"]["centerSegments"], center);
    assert_eq!(ribbon["vectorExt"]["pressure"]["widthProfile"], json!([{"t":0,"width":2},{"t":1,"width":4}]), "the profile is kept");
    let f = |v: &Value| [v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()];
    let segs: Vec<SegOut> = center.as_array().unwrap().iter()
        .map(|a| SegOut { point: f(&a["point"]), handle_in: f(&a["handleIn"]), handle_out: f(&a["handleOut"]) }).collect();
    let outline = profiled_outline(&segs, &[(0., 2.), (1., 4.)]).unwrap();
    let painted = ribbon["geometry"]["segments"].as_array().unwrap();
    assert_eq!(painted.len(), outline.len(), "the ribbon is the shared producer's outline");
    for (p, o) in painted.iter().zip(&outline) {
        assert_eq!((f(&p["point"]), f(&p["handleIn"]), f(&p["handleOut"])), (o.point, o.handle_in, o.handle_out));
    }
    let sides: Vec<Value> = center.as_array().unwrap().iter()
        .map(|a| json!({"point":a["point"],"handleIn":a["handleIn"],"handleOut":a["handleOut"]})).collect();
    assert_eq!(fill["geometry"]["segments"], json!(sides), "the linked fill follows the same centreline");
    assert_eq!(fill["geometry"]["closed"], true);
    owner.undo(2).unwrap();
    assert_eq!(state(&owner), before, "one undo restores the ribbon and its fill");
    let geometry = objects(&before).iter().find(|r| r["target"]["strokeId"] == "native-stroke:1:2").unwrap()["geometry"].clone();
    for (name, op) in [
        ("both", json!({"op":"reshape","stableTarget":target("L1", 0, "native-stroke:1:1"),"center":center,"geometry":geometry})),
        ("fill", json!({"op":"reshape","stableTarget":target("L1", 0, "native-stroke:1:0"),"center":center})),
        ("plain", json!({"op":"reshape","stableTarget":target("L1", 0, "native-stroke:1:2"),"center":center})),
        ("one-anchor", sculpt(json!([anchor(0., 10., 2.)]))),
        ("clone", json!({"op":"clone","stableTarget":target("L1", 0, "native-stroke:1:1"),"frame":0,"center":center})),
        ("no-width", sculpt(json!([{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}, anchor(9., 9., 1.)]))),
    ] {
        assert_eq!(send(&mut owner, name, batch(vec![op]))["ok"], false, "{name} is refused");
    }
}

/// #2372 Select: a `reshape` with a record-space `matrix` moves one stored record and what belongs to it.
mod select_transform {
    use super::*;

    const MOVE: [f64; 6] = [2.0, 0.0, 0.0, 2.0, 10.0, 5.0];
    fn reshape(id: &str, m: [f64; 6]) -> Value {
        json!({"op":"reshape","stableTarget":target("L1", 0, id),"matrix":m})
    }
    fn closed(mut record: Value, id: &str, pts: [(f64, f64); 4], ext: Value) -> Value {
        record["target"]["strokeId"] = json!(id);
        record["geometry"]["closed"] = json!(true);
        record["geometry"]["segments"] = json!(pts.map(|(x, y)| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        if !ext.is_null() {
            record["vectorExt"] = ext;
        }
        record
    }
    const SQUARE: [(f64, f64); 4] = [(0., 0.), (100., 0.), (100., 60.), (0., 60.)];
    fn at(owner: &NativeObjectHistory, id: &str) -> Value {
        objects(&state(owner)).iter().find(|r| r["target"]["strokeId"] == id).unwrap().clone()
    }
    fn xy(v: &Value) -> [f64; 2] {
        [v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()]
    }
    fn points(record: &Value) -> Vec<[f64; 2]> {
        record["geometry"]["segments"].as_array().unwrap().iter().map(|s| xy(&s["point"])).collect()
    }

    #[test]
    fn a_plain_stroke_is_mapped_by_the_matrix_in_one_entry() {
        let doc = document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a", SQUARE, Value::Null)], None);
        let mut owner = owner_of(&doc);
        let before = state(&owner);
        let response = send(&mut owner, "move", batch(vec![reshape("s-a", MOVE)]));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(points(&at(&owner, "s-a")), vec![[10., 5.], [210., 5.], [210., 125.], [10., 125.]]);
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(state(&owner), before);
    }

    #[test]
    fn a_textured_ribbon_moves_with_its_centreline_linked_fill_and_dabs() {
        use super::super::create_many::pressure_record;
        use crate::object_document::pressure_geometry::{profiled_outline, SegOut};
        let center = json!([{"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
            {"point":{"x":40,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4}]);
        let mut owner = owner_of(&document(1, seeds(), None));
        let companion = pressure_record([0.2, 0.4, 0.6], json!({"linkedFillId":"lf","isLinkedFillCompanion":true}));
        let ribbon = pressure_record([0.1, 0.1, 0.1], json!({"linkedFillId":"lf","brushTexture":{"brushGroupId":"g","brushTexturePreset":"chalk-round","preTextureOpacity":1},
            "pressure":{"centerSegments":center,"widthProfile":[{"t":0,"width":2},{"t":1,"width":4}]}}));
        let dab = closed(record_at("A", "L1", 0, ""), "", SQUARE, json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}}));
        let other = closed(record_at("A", "L1", 0, ""), "", SQUARE, json!({"brushTexture":{"brushGroupId":"h","isBrushTextureCopy":true}}));
        let made = send(&mut owner, "set", json!({"command":"object.vector.createMany","records":[companion, ribbon, dab, other]}));
        assert_eq!(made["ok"], true, "{made}");
        let before = state(&owner);
        let response = send(&mut owner, "move", batch(vec![reshape("native-stroke:1:1", MOVE)]));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["result"]["strokeIds"], json!(["native-stroke:1:1"]), "one receipt id per requested step; the fill and copies are not steps");
        let (ribbon, fill, dab, other) = (at(&owner, "native-stroke:1:1"), at(&owner, "native-stroke:1:0"), at(&owner, "native-stroke:1:2"), at(&owner, "native-stroke:1:3"));
        let moved = ribbon["vectorExt"]["pressure"]["centerSegments"].as_array().unwrap().clone();
        assert_eq!((xy(&moved[0]["point"]), xy(&moved[1]["point"])), ([10., 25.], [90., 25.]), "the centreline is mapped");
        assert_eq!((moved[0]["width"].as_f64(), moved[1]["width"].as_f64()), (Some(4.), Some(8.)), "widths follow the mean axis scale");
        let segs: Vec<SegOut> = moved.iter().map(|a| SegOut { point: xy(&a["point"]), handle_in: xy(&a["handleIn"]), handle_out: xy(&a["handleOut"]) }).collect();
        let outline = profiled_outline(&segs, &[(0., 2.), (1., 4.)]).unwrap();
        let painted = ribbon["geometry"]["segments"].as_array().unwrap();
        assert_eq!(painted.len(), outline.len(), "the outline is rebuilt from the mapped centreline");
        assert_eq!(xy(&painted[0]["point"]), outline[0].point);
        assert_eq!(fill["geometry"]["segments"].as_array().unwrap().iter().map(|s| xy(&s["point"])).collect::<Vec<_>>(), vec![[10., 25.], [90., 25.]]);
        assert_eq!(points(&dab), vec![[10., 5.], [210., 5.], [210., 125.], [10., 125.]], "the anchor's copy follows");
        assert_eq!(points(&other), vec![[0., 0.], [100., 0.], [100., 60.], [0., 60.]], "another group's copy stays");
        assert_eq!(owner.history_depths(), (2, 0));
        owner.undo(2).unwrap();
        assert_eq!(state(&owner), before, "one undo restores all of them");
    }

    #[test]
    fn a_gradient_fill_shape_takes_its_end_points() {
        let ext = json!({"isFillShape":true,"fillGradient":{"kind":"linear","from":[0,0],"to":[100,0],"stops":[{"offset":0,"color":"#ff0000ff"},{"offset":1,"color":"#0000ffff"}]}});
        let mut owner = owner_of(&document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a", SQUARE, ext)], None));
        let response = send(&mut owner, "move", batch(vec![reshape("s-a", MOVE)]));
        assert_eq!(response["ok"], true, "{response}");
        let shape = at(&owner, "s-a");
        assert_eq!((shape["vectorExt"]["fillGradient"]["from"].clone(), shape["vectorExt"]["fillGradient"]["to"].clone()), (json!([10., 5.]), json!([210., 5.])));
        assert_eq!(points(&shape)[2], [210., 125.]);
    }

    #[cfg(feature = "evaluation")]
    #[test]
    fn a_dynamic_shape_keeps_its_box_and_takes_a_turn_as_box_angle() {
        let ext = json!({"paramShape":{"kind":"rect","box":{"x1":0,"y1":0,"x2":100,"y2":60}}});
        let mut owner = owner_of(&document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a", SQUARE, ext)], None));
        assert_eq!(send(&mut owner, "move", batch(vec![reshape("s-a", [1., 0., 0., 1., 10., 5.])]))["ok"], true);
        let shape = at(&owner, "s-a");
        assert_eq!(shape["vectorExt"]["paramShape"]["box"], json!({"x1":10.,"y1":5.,"x2":110.,"y2":65.}), "the frame follows a move");
        assert_eq!(points(&shape)[0], [10., 5.]);
        assert_eq!(send(&mut owner, "scale", batch(vec![reshape("s-a", [2., 0., 0., 1., 0., 0.])]))["ok"], true);
        assert_eq!(at(&owner, "s-a")["vectorExt"]["paramShape"]["box"], json!({"x1":20.,"y1":5.,"x2":220.,"y2":65.}), "the frame follows a resize");
        let quarter = send(&mut owner, "turn", batch(vec![reshape("s-a", [0., 1., -1., 0., 0., 0.])]));
        assert_eq!(quarter["ok"], true, "{quarter}");
        assert_eq!(at(&owner, "s-a")["vectorExt"]["boxAngle"].as_f64(), Some(90.), "a turn adds to the stored angle");
        assert_eq!(send(&mut owner, "skew", batch(vec![reshape("s-a", [1., 0., 0.5, 1., 0., 0.])]))["ok"], false, "a skew of a dynamic shape is refused");
        assert_eq!(owner.history_depths(), (3, 0));
    }

    #[test]
    fn a_matrix_that_cannot_be_applied_refuses_the_whole_batch() {
        let bitmap = json!({"bitmapBrush":{"bitmapBrushSpec":{"tip":"hard","size":8,"spacing":15,"scatter":0,"opacity":1,"color":"#0000ffff","pressure":false,"seed":17},"brushGroupId":"g","preTextureOpacity":1}});
        let dab = json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}});
        let doc = document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a", SQUARE, Value::Null), closed(record_at("A", "L1", 0, ""), "s-d", SQUARE, dab),
            { let mut b = closed(record_at("A", "L1", 0, ""), "s-b", SQUARE, bitmap); b["vector"]["opacity"] = json!(0); b }], None);
        let mut owner = owner_of(&doc);
        let before = state(&owner);
        for (name, ops) in [
            ("singular", vec![reshape("s-a", [1., 2., 2., 4., 0., 0.])]),
            ("missing", vec![reshape("s-x", MOVE)]),
            ("copy", vec![reshape("s-d", MOVE)]),
            ("bitmap", vec![reshape("s-b", MOVE)]),
            ("also-geometry", vec![json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"matrix":MOVE,"center":[]})]),
            ("clone", vec![json!({"op":"clone","stableTarget":target("L1", 0, "s-a"),"frame":0,"matrix":MOVE})]),
            ("create", vec![json!({"op":"create","record":record_at("B", "L1", 0, ""),"matrix":MOVE})]),
            ("second-step", vec![reshape("s-a", MOVE), reshape("s-x", MOVE)]),
        ] {
            assert_eq!(send(&mut owner, name, batch(ops))["ok"], false, "{name} is refused");
        }
        assert_eq!(state(&owner), before, "nothing was written");
    }
}
