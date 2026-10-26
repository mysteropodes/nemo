//! #2361: Trim Stroke Ends on native Draw creates (`trimStrokeEnds`, tools.js @1065e9d8): a new open stroke's
//! dangling head/tail is cut back to its first/last crossing with the other paths of its layer and frame
//! when that overshoot is at most `trimEnds`; symmetry copies in the same createMany receive the same cuts.
use super::support::*;
use serde_json::{json, Value};

fn path(layer: &str, frame: u32, stroke: &str, points: &[(f64, f64)]) -> Value {
    let mut record = record_at("A", layer, frame, stroke);
    record["geometry"]["closed"] = json!(false);
    record["geometry"]["segments"] = points
        .iter()
        .map(|(x, y)| json!({"point":{"x":x,"y":y},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}}))
        .collect();
    record
}
fn walls() -> Vec<Value> {
    vec![
        path("L1", 0, "w-10", &[(10.0, 0.0), (10.0, 100.0)]),
        path("L1", 0, "w-95", &[(95.0, 0.0), (95.0, 100.0)]),
        path("L2", 0, "w-other-layer", &[(50.0, 0.0), (50.0, 100.0)]),
        path("L1", 5, "w-other-frame", &[(60.0, 0.0), (60.0, 100.0)]),
    ]
}
fn trimmed(payload: Value, objects: Vec<Value>) -> (Value, Vec<Value>) {
    let mut owner = owner_of(&document(1, objects, None));
    let response = send(&mut owner, "trim", payload);
    let after = state(&owner);
    let created = objects_of(&after);
    (response, created)
}
fn objects_of(state: &Value) -> Vec<Value> {
    objects(state)
        .iter()
        .filter(|o| o["target"]["strokeId"].as_str().unwrap().starts_with("native-stroke:"))
        .cloned()
        .collect()
}
fn points(record: &Value) -> Vec<(f64, f64)> {
    record["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["point"]["x"].as_f64().unwrap(), s["point"]["y"].as_f64().unwrap()))
        .collect()
}
fn near(actual: &[(f64, f64)], expected: &[(f64, f64)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a.0 - e.0).abs() < 1e-6 && (a.1 - e.1).abs() < 1e-6, "{actual:?} != {expected:?}");
    }
}
fn create_trim(record: Value, max: Value) -> Value {
    json!({"command":"object.vector.create","record":record,"trimEnds":max})
}

#[test]
fn both_overshoots_within_the_limit_are_cut_to_the_first_and_last_crossing_of_the_same_layer_and_frame() {
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let (response, created) = trimmed(create_trim(line, json!(24)), walls());
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["historyEntriesAdded"], 1);
    near(&points(&created[0]), &[(10.0, 50.0), (95.0, 50.0)]);
    // A straight curve divides without handles, as Paper's divideAtTime does for a handle-less curve.
    for segment in created[0]["geometry"]["segments"].as_array().unwrap() {
        assert_eq!(segment["handleIn"], json!({"x":0.0,"y":0.0}));
        assert_eq!(segment["handleOut"], json!({"x":0.0,"y":0.0}));
    }
}

#[test]
fn an_overshoot_beyond_the_limit_is_kept_and_no_crossing_or_limit_leaves_the_stroke_whole() {
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let (_, created) = trimmed(create_trim(line.clone(), json!(8)), walls());
    near(&points(&created[0]), &[(0.0, 50.0), (95.0, 50.0)]);
    let (_, created) = trimmed(create_trim(line.clone(), json!(24)), vec![]);
    near(&points(&created[0]), &[(0.0, 50.0), (100.0, 50.0)]);
    let (_, created) = trimmed(create(line), walls());
    near(&points(&created[0]), &[(0.0, 50.0), (100.0, 50.0)]);
}

#[test]
fn the_guard_keeps_a_stroke_that_would_be_mostly_eaten_and_a_closed_stroke_is_never_trimmed() {
    let short = path("L1", 0, "", &[(8.0, 50.0), (12.5, 50.0)]);
    let (_, created) = trimmed(create_trim(short, json!(24)), walls());
    near(&points(&created[0]), &[(8.0, 50.0), (12.5, 50.0)]);
    let mut closed = path("L1", 0, "", &[(0.0, 40.0), (100.0, 40.0), (100.0, 60.0), (0.0, 60.0)]);
    closed["geometry"]["closed"] = json!(true);
    let (_, created) = trimmed(create_trim(closed.clone(), json!(24)), walls());
    assert_eq!(created[0]["geometry"], closed["geometry"]);
}

#[test]
fn symmetry_copies_in_one_create_many_receive_the_same_arc_length_cuts() {
    let original = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let mirror = path("L1", 0, "", &[(100.0, 50.0), (0.0, 50.0)]);
    let payload = json!({"command":"object.vector.createMany","records":[original, mirror],"trimEnds":24});
    let (response, created) = trimmed(payload, walls());
    assert_eq!(response["result"]["historyEntriesAdded"], 1, "{response}");
    near(&points(&created[0]), &[(10.0, 50.0), (95.0, 50.0)]);
    near(&points(&created[1]), &[(90.0, 50.0), (5.0, 50.0)]);
}

#[test]
fn a_curved_stroke_keeps_its_shape_between_the_cuts() {
    let mut curve = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    curve["geometry"]["segments"][0]["handleOut"] = json!({"x":30.0,"y":-40.0});
    curve["geometry"]["segments"][1]["handleIn"] = json!({"x":-30.0,"y":-40.0});
    let (_, created) = trimmed(create_trim(curve, json!(40)), walls());
    let s = created[0]["geometry"]["segments"].as_array().unwrap();
    assert_eq!(s.len(), 2, "{s:?}");
    // Both cut anchors sit on the wall lines and carry the divided handles.
    assert!((s[0]["point"]["x"].as_f64().unwrap() - 10.0).abs() < 1e-6, "{s:?}");
    assert!((s[1]["point"]["x"].as_f64().unwrap() - 95.0).abs() < 1e-6, "{s:?}");
    assert_ne!(s[0]["handleIn"], json!({"x":0.0,"y":0.0}));
    assert_ne!(s[1]["handleOut"], json!({"x":0.0,"y":0.0}));
}

#[test]
fn trim_is_refused_on_other_commands_with_a_bad_limit_and_for_a_vector_brush_stroke() {
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let base = document(1, walls(), None);
    // A pressure ribbon is a filled outline with no real stroke (validate_vector_ext); oracle "C" is one.
    let mut brush = record_at("C", "L1", 0, "");
    brush["vectorExt"] = json!({"pressure":{"centerSegments":[
        {"point":{"x":0,"y":50},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":0.5},
        {"point":{"x":100,"y":50},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":3}],
        "widthProfile":[{"t":0,"width":0.5},{"t":1,"width":3}]}});
    for (name, payload) in [
        ("zero", create_trim(line.clone(), json!(0))),
        ("negative", create_trim(line.clone(), json!(-1))),
        ("text", create_trim(line.clone(), json!("24"))),
        ("null", create_trim(line.clone(), Value::Null)),
        ("replace", json!({"command":"object.vector.replace","record":path("L1",0,"w-10",&[(1.0,1.0),(2.0,2.0)]),"trimEnds":24})),
        ("delete", json!({"command":"object.vector.delete","stableTarget":target("L1",0,"w-10"),"trimEnds":24})),
        ("brush", create_trim(brush.clone(), json!(24))),
    ] {
        let mut owner = owner_of(&base);
        let refused = send(&mut owner, name, payload);
        assert_eq!(refused["ok"], false, "{name}: {refused}");
        assert_eq!(owner.content_revision(), 0, "{name}");
    }
    // Controls: each refused payload is admitted without its trimEnds member.
    for (name, payload) in [
        ("replace", json!({"command":"object.vector.replace","record":path("L1",0,"w-10",&[(1.0,1.0),(2.0,2.0)])})),
        ("brush", create(brush)),
    ] {
        let mut owner = owner_of(&base);
        let admitted = send(&mut owner, name, payload);
        assert_eq!(admitted["ok"], true, "{name}: {admitted}");
    }
}

#[test]
fn a_vector_brush_wall_is_crossed_at_its_centreline_not_its_outline() {
    // fillWallPath reads a vector brush's centre segments; this outline lies far from the new stroke.
    let mut ribbon = record_at("C", "L1", 0, "w-ribbon");
    ribbon["geometry"]["segments"] = json!([
        {"point":{"x":200.0,"y":0.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}},
        {"point":{"x":210.0,"y":0.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}},
        {"point":{"x":210.0,"y":100.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}},
        {"point":{"x":200.0,"y":100.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}}]);
    ribbon["vectorExt"] = json!({"pressure":{"centerSegments":[
        {"point":{"x":12.0,"y":0.0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
        {"point":{"x":12.0,"y":100.0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2}],
        "widthProfile":[{"t":0,"width":2},{"t":1,"width":2}]}});
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let (response, created) = trimmed(create_trim(line, json!(24)), vec![ribbon]);
    assert_eq!(response["ok"], true, "{response}");
    // One crossing: the head (12) is cut; the tail (88) exceeds the limit and stays.
    near(&points(&created[0]), &[(12.0, 50.0), (100.0, 50.0)]);
}

#[test]
fn tracing_over_an_existing_stroke_is_bounded_and_still_commits_once() {
    // Collinear overlap is the subdivision worst case; many identical walls must not stall the create.
    let mut objects = Vec::new();
    for i in 0..64 {
        objects.push(path("L1", 0, &format!("w-{i}"), &[(0.0, 50.0), (25.0, 50.0), (50.0, 50.0), (75.0, 50.0), (100.0, 50.0)]));
    }
    let line = path("L1", 0, "", &[(0.0, 50.0), (25.0, 50.0), (50.0, 50.0), (75.0, 50.0), (100.0, 50.0)]);
    let started = std::time::Instant::now();
    let (response, created) = trimmed(create_trim(line, json!(24)), objects);
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(created.len(), 1);
    // An exhausted search may have missed the latest crossing, so nothing is trimmed.
    near(&points(&created[0]), &[(0.0, 50.0), (25.0, 50.0), (50.0, 50.0), (75.0, 50.0), (100.0, 50.0)]);
    assert!(started.elapsed().as_secs_f64() < 20.0, "{:?}", started.elapsed());
}

fn outline(label: &str, stroke: &str, x0: f64, x1: f64) -> Value {
    let mut record = record_at(label, "L1", 0, stroke);
    record["geometry"]["closed"] = json!(true);
    record["geometry"]["segments"] = [(x0, 0.0), (x1, 0.0), (x1, 100.0), (x0, 100.0)]
        .iter()
        .map(|(x, y)| json!({"point":{"x":x,"y":y},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}}))
        .collect();
    record
}

#[test]
fn a_linked_fill_companion_and_a_wall_without_paint_are_not_walls() {
    // The backdrop crosses the stroke at x=94/96 but is a linked-fill companion; the baseline filter skips it.
    let mut backdrop = outline("C", "w-backdrop", 94.0, 96.0);
    backdrop["vectorExt"] = json!({"linkedFillId":"lf","isLinkedFillCompanion":true});
    let mut ribbon = outline("C", "w-ribbon", 300.0, 310.0);
    ribbon["vectorExt"] = json!({"linkedFillId":"lf","isLinkedFillCompanion":false,"pressure":{"centerSegments":[
        {"point":{"x":305.0,"y":0.0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
        {"point":{"x":305.0,"y":100.0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2}],
        "widthProfile":[{"t":0,"width":2},{"t":1,"width":2}]}});
    // A path with neither stroke nor fill (nor a brush) paints nothing and is no wall either.
    let mut bare = path("L1", 0, "w-bare", &[(90.0, 0.0), (90.0, 100.0)]);
    bare["vector"]["stroke"] = Value::Null;
    bare["vector"]["hasRealStroke"] = json!(false);
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let walls = vec![path("L1", 0, "w-10", &[(10.0, 0.0), (10.0, 100.0)]), backdrop, ribbon, bare];
    let (response, created) = trimmed(create_trim(line, json!(24)), walls);
    assert_eq!(response["ok"], true, "{response}");
    near(&points(&created[0]), &[(10.0, 50.0), (100.0, 50.0)]);
}

#[test]
fn a_brush_texture_dab_is_not_a_wall() {
    // trimStrokeEnds skips `isBrushTextureCopy` children; a dab crossing at x=95 must not cut the tail.
    let mut dab = path("L1", 0, "w-dab", &[(95.0, 0.0), (95.0, 100.0)]);
    dab["vectorExt"] = json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}});
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let walls = vec![path("L1", 0, "w-10", &[(10.0, 0.0), (10.0, 100.0)]), dab];
    let (response, created) = trimmed(create_trim(line, json!(24)), walls);
    assert_eq!(response["ok"], true, "{response}");
    near(&points(&created[0]), &[(10.0, 50.0), (100.0, 50.0)]);
}

#[test]
fn a_multi_frame_batch_cuts_the_drawn_stroke_in_its_own_cell_and_every_stamp_takes_the_same_cut() {
    // The baseline trims the drawn path before the Labs multi-frame hook stamps it: the frame-5 stamp is the frame-0
    // cut, never re-trimmed against frame 5's own wall at x=60.
    let drawn = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let stamp = path("L1", 5, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let payload = json!({"command":"object.vector.batch","trimEnds":24,
        "ops":[{"op":"create","record":drawn},{"op":"create","record":stamp,"keyframe":"copy"}]});
    let (response, created) = trimmed(payload, walls());
    assert_eq!(response["result"]["historyEntriesAdded"], 1, "{response}");
    assert_eq!(created.len(), 2);
    near(&points(&created[0]), &[(10.0, 50.0), (95.0, 50.0)]);
    near(&points(&created[1]), &[(10.0, 50.0), (95.0, 50.0)]);
}
