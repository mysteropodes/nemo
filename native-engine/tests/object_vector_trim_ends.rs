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
fn trim_is_refused_on_other_commands_and_with_a_bad_limit() {
    let line = path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)]);
    let base = document(1, walls(), None);
    for (name, payload) in [
        ("zero", create_trim(line.clone(), json!(0))),
        ("negative", create_trim(line.clone(), json!(-1))),
        ("text", create_trim(line.clone(), json!("24"))),
        ("null", create_trim(line.clone(), Value::Null)),
        ("replace", json!({"command":"object.vector.replace","record":path("L1",0,"w-10",&[(1.0,1.0),(2.0,2.0)]),"trimEnds":24})),
        ("delete", json!({"command":"object.vector.delete","stableTarget":target("L1",0,"w-10"),"trimEnds":24})),
    ] {
        let mut owner = owner_of(&base);
        let refused = send(&mut owner, name, payload);
        assert_eq!(refused["ok"], false, "{name}: {refused}");
        assert_eq!(owner.content_revision(), 0, "{name}");
    }
    // Controls: each refused payload is admitted without its trimEnds member.
    for (name, payload) in [
        ("replace", json!({"command":"object.vector.replace","record":path("L1",0,"w-10",&[(1.0,1.0),(2.0,2.0)])})),
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

// A pressure ribbon (tools.js trimStrokeEnds isVB @1065e9d8): oracle cases generated by running the baseline
// trimStrokeEnds/rebuildVectorBrushOutline on Paper.js (fixture provenance field).
const RIBBON_ORACLE: &str = include_str!("fixtures/object-vector-trim-ribbon/oracle.json");

#[cfg(not(feature = "application"))]
#[test]
fn pressure_trim_without_application_refuses_without_publication_or_history() {
    let oracle: Value = serde_json::from_str(RIBBON_ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let mut owner = owner_of(&document(1, walls(), None));
    assert_unavailable(
        &mut owner,
        "pressure-without-application",
        create_trim(ribbon_of(case, None), case["max"].clone()),
    );
}
fn xy(a: &Value) -> Value {
    json!({"x":a[0],"y":a[1]})
}
fn seg_of(s: &Value) -> Value {
    json!({"point":xy(&s["point"]),"handleIn":xy(&s["handleIn"]),"handleOut":xy(&s["handleOut"])})
}
fn ribbon_of(case: &Value, linked: Option<&str>) -> Value {
    let input = &case["input"];
    // A pressure ribbon is a filled outline with no real stroke (validate_vector_ext); oracle "C" is one.
    let mut r = record_at("C", "L1", 0, "");
    r["geometry"]["closed"] = json!(true);
    r["geometry"]["segments"] = input["outline"].as_array().unwrap().iter().map(seg_of).collect();
    r["vector"]["strokeWidth"] = case["strokeWidth"].clone();
    let centre: Vec<Value> = input["centerSegments"].as_array().unwrap().iter().map(|s| {
        let mut v = seg_of(s);
        v["width"] = s["width"].clone();
        v
    }).collect();
    let mut pressure = json!({ "centerSegments": centre });
    if !input["widthProfile"].is_null() {
        pressure["widthProfile"] = input["widthProfile"].clone();
    }
    r["vectorExt"] = json!({ "pressure": pressure });
    if let Some(id) = linked {
        r["vectorExt"]["linkedFillId"] = json!(id);
    }
    r
}
#[cfg(feature = "application")]
fn flat(segments: &Value) -> Vec<f64> {
    segments.as_array().unwrap().iter().flat_map(|s| {
        ["point", "handleIn", "handleOut"].into_iter().flat_map(move |k| {
            let v = &s[k];
            if v.is_array() { [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()] } else { [v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()] }
        })
    }).collect()
}
#[cfg(feature = "application")]
fn worst(actual: &[f64], expected: &[f64], what: &str) -> f64 {
    assert_eq!(actual.len(), expected.len(), "{what}: {} values, expected {}", actual.len(), expected.len());
    actual.iter().zip(expected).map(|(a, e)| (a - e).abs()).fold(0., f64::max)
}

#[test]
#[cfg(feature = "application")]
fn a_pressure_ribbon_trims_its_centreline_and_rebuilds_its_outline_profile_and_linked_fill_as_the_baseline() {
    let oracle: Value = serde_json::from_str(RIBBON_ORACLE).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let walls: Vec<Value> = case["walls"].as_array().unwrap().iter().enumerate().map(|(i, w)| {
            path("L1", 0, &format!("w-{i}"), &[(w[0][0].as_f64().unwrap(), w[0][1].as_f64().unwrap()), (w[1][0].as_f64().unwrap(), w[1][1].as_f64().unwrap())])
        }).collect();
        let linked = (!case["input"]["fill"].is_null()).then_some("lf-1");
        let ribbon = ribbon_of(case, linked);
        let payload = match linked {
            Some(id) => {
                let mut fill = record_at("C", "L1", 0, "");
                fill["geometry"]["closed"] = json!(true);
                fill["geometry"]["segments"] = case["input"]["fill"].as_array().unwrap().iter().map(seg_of).collect();
                fill["vectorExt"] = json!({"isLinkedFillCompanion":true,"linkedFillId":id});
                json!({"command":"object.vector.createMany","records":[fill, ribbon],"trimEnds":case["max"]})
            }
            None => create_trim(ribbon, case["max"].clone()),
        };
        let (response, created) = trimmed(payload, walls);
        assert_eq!(response["ok"], true, "{name}: {response}");
        let made = created.iter().find(|r| !r["vectorExt"]["pressure"].is_null()).unwrap();
        let expected = &case["expected"];
        let centre = &made["vectorExt"]["pressure"]["centerSegments"];
        let widths: Vec<f64> = centre.as_array().unwrap().iter().map(|c| c["width"].as_f64().unwrap()).collect();
        let expected_widths: Vec<f64> = expected["centerSegments"].as_array().unwrap().iter().map(|c| c["width"].as_f64().unwrap()).collect();
        let errors = [
            worst(&flat(centre), &flat(&expected["centerSegments"]), "centre"),
            worst(&widths, &expected_widths, "widths"),
            worst(&flat(&made["geometry"]["segments"]), &flat(&expected["outline"]), "outline"),
        ];
        eprintln!("{name}: worst centre {:e} widths {:e} outline {:e}", errors[0], errors[1], errors[2]);
        assert!(errors.iter().all(|&e| e < 1e-6), "{name}: {errors:?}");
        assert_eq!(made["geometry"]["closed"], true, "{name}");
        if expected["trimmed"] == true {
            let profile: Vec<f64> = made["vectorExt"]["pressure"]["widthProfile"].as_array().unwrap().iter()
                .flat_map(|s| [s["t"].as_f64().unwrap(), s["width"].as_f64().unwrap()]).collect();
            let expected_profile: Vec<f64> = expected["widthProfile"].as_array().unwrap().iter()
                .flat_map(|s| [s["t"].as_f64().unwrap(), s["width"].as_f64().unwrap()]).collect();
            assert!(worst(&profile, &expected_profile, "profile") < 1e-9, "{name}: profile");
        }
        if linked.is_some() {
            let fill = created.iter().find(|r| r["vectorExt"]["isLinkedFillCompanion"] == true).unwrap();
            assert!(worst(&flat(&fill["geometry"]["segments"]), &flat(&expected["fill"]), "fill") < 1e-6, "{name}: linked fill");
        }
    }
}

#[test]
fn a_ribbon_without_a_centreline_stays_unavailable() {
    let oracle: Value = serde_json::from_str(RIBBON_ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let ribbon = ribbon_of(case, None);
    let mut bare = ribbon.clone();
    bare["vectorExt"]["pressure"] = json!({"widthProfile":[{"t":0,"width":1},{"t":1,"width":1}]});
    for (name, payload) in [
        ("bare", create_trim(bare, json!(20))),
        ("plain-copy", json!({"command":"object.vector.createMany","records":[ribbon, path("L1", 0, "", &[(0.0, 50.0), (100.0, 50.0)])],"trimEnds":20})),
    ] {
        let mut owner = owner_of(&document(1, walls(), None));
        let refused = send(&mut owner, name, payload);
        assert_eq!(refused["ok"], false, "{name}: {refused}");
        assert_eq!(owner.content_revision(), 0, "{name}");
    }
}

#[test]
#[cfg(feature = "application")]
fn a_ribbon_and_its_linked_fill_stamped_on_another_frame_take_the_drawn_cells_trim() {
    // multiframe-draw stamps the already trimmed ribbon and its fill: frame 5's own wall (x=60) never cuts the stamp.
    let oracle: Value = serde_json::from_str(RIBBON_ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let at = |mut record: Value, frame: u32| {
        record["target"]["frameScope"]["frame"] = json!(frame);
        record
    };
    let ribbon = ribbon_of(case, Some("lf-1"));
    let mut fill = record_at("C", "L1", 0, "");
    fill["geometry"]["closed"] = json!(true);
    fill["geometry"]["segments"] = case["input"]["fill"].as_array().unwrap().iter().map(seg_of).collect();
    fill["vectorExt"] = json!({"isLinkedFillCompanion":true,"linkedFillId":"lf-1"});
    let ops = json!([
        {"op":"create","record":fill.clone()},{"op":"create","record":ribbon.clone()},
        {"op":"create","record":at(fill, 5),"keyframe":"copy"},{"op":"create","record":at(ribbon, 5),"keyframe":"copy"}]);
    let (response, created) = trimmed(json!({"command":"object.vector.batch","trimEnds":case["max"],"ops":ops}), walls());
    assert_eq!(response["result"]["historyEntriesAdded"], 1, "{response}");
    let expected = &case["expected"];
    for frame in [0, 5] {
        let cell: Vec<&Value> = created.iter().filter(|r| r["target"]["frameScope"]["frame"] == frame).collect();
        let ribbon = cell.iter().find(|r| !r["vectorExt"]["pressure"].is_null()).unwrap();
        let fill = cell.iter().find(|r| r["vectorExt"]["isLinkedFillCompanion"] == true).unwrap();
        assert!(worst(&flat(&ribbon["geometry"]["segments"]), &flat(&expected["outline"]), "outline") < 1e-6, "frame {frame}");
        assert!(worst(&flat(&ribbon["vectorExt"]["pressure"]["centerSegments"]), &flat(&expected["centerSegments"]), "centre") < 1e-6, "frame {frame}");
        assert!(worst(&flat(&fill["geometry"]["segments"]), &flat(&expected["fill"]), "fill") < 1e-6, "frame {frame}");
    }
}

// symmetry-bridge.js onStrokeCommitted after trimStrokeEnds (@1065e9d8): the copies of the trimmed ribbon.
const SYMMETRY_ORACLE: &str = include_str!("fixtures/object-vector-trim-ribbon/symmetry-oracle.json");
fn ribbon_entry(entry: &Value, width: &Value, link: Option<&str>) -> Value {
    let mut r = record_at("C", "L1", 0, "");
    r["geometry"]["closed"] = json!(true);
    r["geometry"]["segments"] = entry["outline"].as_array().unwrap().iter().map(seg_of).collect();
    r["vector"]["strokeWidth"] = width.clone();
    let centre: Vec<Value> = entry["centerSegments"].as_array().unwrap().iter().map(|s| {
        let mut v = seg_of(s);
        v["width"] = s["width"].clone();
        v
    }).collect();
    r["vectorExt"] = json!({"pressure":{"centerSegments":centre,"widthProfile":entry["widthProfile"]}});
    if let Some(id) = link {
        r["vectorExt"]["linkedFillId"] = json!(id);
    }
    r
}

#[test]
#[cfg(feature = "application")]
fn symmetry_copies_of_a_trimmed_ribbon_take_the_drawn_ribbons_cut_and_rebuild_as_the_baseline() {
    let oracle: Value = serde_json::from_str(SYMMETRY_ORACLE).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["trimmed"], true, "{name}");
        let walls: Vec<Value> = case["walls"].as_array().unwrap().iter().enumerate().map(|(i, w)| {
            path("L1", 0, &format!("w-{i}"), &[(w[0][0].as_f64().unwrap(), w[0][1].as_f64().unwrap()), (w[1][0].as_f64().unwrap(), w[1][1].as_f64().unwrap())])
        }).collect();
        // The native Draw createMany: each cell bottom to top, a linked fill directly below its ribbon.
        let mut records = Vec::new();
        for (k, entry) in case["input"].as_array().unwrap().iter().enumerate() {
            let link = format!("lf-{k}");
            let linked = !entry["fill"].is_null();
            if linked {
                let mut fill = record_at("C", "L1", 0, "");
                fill["geometry"]["closed"] = json!(true);
                fill["geometry"]["segments"] = entry["fill"].as_array().unwrap().iter().map(seg_of).collect();
                fill["vectorExt"] = json!({"isLinkedFillCompanion":true,"linkedFillId":link});
                records.push(fill);
            }
            records.push(ribbon_entry(entry, &case["strokeWidth"], linked.then_some(link.as_str())));
        }
        let payload = json!({"command":"object.vector.createMany","records":records,"trimEnds":case["max"]});
        let (response, created) = trimmed(payload, walls);
        assert_eq!(response["ok"], true, "{name}: {response}");
        let ribbons: Vec<&Value> = created.iter().filter(|r| !r["vectorExt"]["pressure"].is_null()).collect();
        let expected = case["expected"].as_array().unwrap();
        assert_eq!(ribbons.len(), expected.len(), "{name}");
        let mut worst_seen: f64 = 0.;
        for (k, (made, want)) in ribbons.iter().zip(expected).enumerate() {
            let centre = &made["vectorExt"]["pressure"]["centerSegments"];
            let widths: Vec<f64> = centre.as_array().unwrap().iter().map(|c| c["width"].as_f64().unwrap()).collect();
            let want_widths: Vec<f64> = want["centerSegments"].as_array().unwrap().iter().map(|c| c["width"].as_f64().unwrap()).collect();
            let errors = [
                worst(&flat(centre), &flat(&want["centerSegments"]), "centre"),
                worst(&widths, &want_widths, "widths"),
                worst(&flat(&made["geometry"]["segments"]), &flat(&want["outline"]), "outline"),
            ];
            assert!(errors.iter().all(|&e| e < 1e-6), "{name} cell {k}: {errors:?}");
            worst_seen = errors.iter().copied().fold(worst_seen, f64::max);
            if !want["fill"].is_null() {
                let link = made["vectorExt"]["linkedFillId"].as_str().unwrap();
                let fill = created.iter().find(|r| r["vectorExt"]["isLinkedFillCompanion"] == true && r["vectorExt"]["linkedFillId"] == link).unwrap();
                assert!(worst(&flat(&fill["geometry"]["segments"]), &flat(&want["fill"]), "fill") < 1e-6, "{name} cell {k}: linked fill");
            }
        }
        eprintln!("{name}: worst {worst_seen:e}");
    }
}
