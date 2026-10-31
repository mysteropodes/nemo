//! #2361: a mirror's stop-at-axis clip runs AFTER Trim Stroke Ends, as the baseline does (draw-bridge.js trims the drawn path,
//! then symmetry-bridge.js onStrokeCommitted clips it at the axis, `clipPathAtAxis` @1065e9d8): the cut is measured on the whole
//! stroke, never on one already clipped at the axis. `trimClip` names the axis and the (drawn, mirror) pairs of a create.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn path(stroke: &str, points: &[(f64, f64)]) -> Value {
    let mut record = record_at("A", "L1", 0, stroke);
    record["geometry"]["closed"] = json!(false);
    record["geometry"]["segments"] = points
        .iter()
        .map(|(x, y)| json!({"point":{"x":x,"y":y},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}}))
        .collect();
    record
}
fn wall(stroke: &str, a: (f64, f64), b: (f64, f64)) -> Value {
    path(stroke, &[a, b])
}
fn points(record: &Value) -> Vec<(f64, f64)> {
    record["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["point"]["x"].as_f64().unwrap(),
                s["point"]["y"].as_f64().unwrap(),
            )
        })
        .collect()
}
fn near(actual: &[(f64, f64)], expected: &[(f64, f64)], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: {actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a.0 - e.0).abs() < 1e-6 && (a.1 - e.1).abs() < 1e-6,
            "{what}: {actual:?} != {expected:?}"
        );
    }
}
fn created(owner: &NativeObjectHistory) -> Vec<Value> {
    objects(&state(owner))
        .iter()
        .filter(|o| {
            o["target"]["strokeId"]
                .as_str()
                .unwrap()
                .starts_with("native-stroke:")
        })
        .cloned()
        .collect()
}
fn run(payload: Value, walls: Vec<Value>) -> (Value, Vec<Value>, u64) {
    let mut owner = owner_of(&document(1, walls, None));
    let response = send(&mut owner, "clip", payload);
    let made = created(&owner);
    (response, made, owner.content_revision())
}
/// The baseline's reflect (symmetry-bridge.js reflectPoint) across the line through (x1, y1) and (x2, y2).
fn reflect(p: (f64, f64), axis: [f64; 4]) -> (f64, f64) {
    let (len_x, len_y) = (axis[2] - axis[0], axis[3] - axis[1]);
    let len = len_x.hypot(len_y);
    let (ux, uy) = (len_x / len, len_y / len);
    let t = (p.0 - axis[0]) * ux + (p.1 - axis[1]) * uy;
    (2. * (axis[0] + t * ux) - p.0, 2. * (axis[1] + t * uy) - p.1)
}
fn mirror_of(record: &Value, axis: [f64; 4]) -> Value {
    let pts: Vec<(f64, f64)> = points(record)
        .into_iter()
        .map(|p| reflect(p, axis))
        .collect();
    path("", &pts)
}
fn many(drawn: Value, axis: [f64; 4], max: f64) -> Value {
    let mirror = mirror_of(&drawn, axis);
    json!({"command":"object.vector.createMany","records":[drawn, mirror],"trimEnds":max,"trimClip":{"axis":axis,"mirrors":[[0,1]]}})
}
/// Trim walls: vertical lines at x = 10 and x = 40 cut the head; the stroke's far end meets no wall.
fn vertical_walls() -> Vec<Value> {
    vec![
        wall("w-10", (10., 0.), (10., 100.)),
        wall("w-40", (40., 0.), (40., 100.)),
    ]
}

#[test]
fn the_cut_is_measured_on_the_whole_stroke_then_each_half_is_clipped_at_a_vertical_axis() {
    // Whole stroke 0..100 along y=80: head to the wall at x=10 (10 <= 24), tail crossing at x=40 is 60 from the end (> 24),
    // so only the head is cut. Clipping first (the old order) left a stroke ending at the axis whose "tail" 15 from x=40
    // WAS cut: the kept half lost 15 at the axis.
    let axis = [60., 0., 60., 100.];
    let drawn = path(
        "",
        &[(0., 80.), (30., 80.), (55., 80.), (80., 80.), (100., 80.)],
    );
    let (response, made, revision) = run(many(drawn, axis, 24.), vertical_walls());
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        (revision, response["result"]["historyEntriesAdded"].as_u64()),
        (1, Some(1))
    );
    near(
        &points(&made[0]),
        &[(10., 80.), (30., 80.), (55., 80.)],
        "drawn",
    );
    near(
        &points(&made[1]),
        &[(110., 80.), (90., 80.), (65., 80.)],
        "mirror",
    );
}

#[test]
fn a_horizontal_axis_clips_the_trimmed_stroke_the_same_way() {
    // The same stroke turned a quarter: walls are horizontal, the axis is y=60.
    let walls = vec![
        wall("w-10", (0., 10.), (100., 10.)),
        wall("w-40", (0., 40.), (100., 40.)),
    ];
    let axis = [0., 60., 100., 60.];
    let drawn = path(
        "",
        &[(80., 0.), (80., 30.), (80., 55.), (80., 80.), (80., 100.)],
    );
    let (response, made, _) = run(many(drawn, axis, 24.), walls);
    assert_eq!(response["ok"], true, "{response}");
    near(
        &points(&made[0]),
        &[(80., 10.), (80., 30.), (80., 55.)],
        "drawn",
    );
    near(
        &points(&made[1]),
        &[(80., 110.), (80., 90.), (80., 65.)],
        "mirror",
    );
}

#[test]
fn a_free_axis_clips_at_the_first_anchor_on_the_far_side_and_both_ends_are_trimmed_first() {
    // Axis y=x; the stroke lies above it until its last anchor. Wall x=95 trims the tail by 5 (the last anchor moves onto
    // the wall, which is on the axis's far side), wall x=10 the head by 10.
    let axis = [0., 0., 100., 100.];
    let walls = vec![
        wall("w-10", (10., 0.), (10., 100.)),
        wall("w-95", (95., 0.), (95., 100.)),
    ];
    let drawn = path(
        "",
        &[(0., 80.), (30., 80.), (55., 80.), (80., 80.), (100., 80.)],
    );
    let (response, made, _) = run(many(drawn, axis, 24.), walls);
    assert_eq!(response["ok"], true, "{response}");
    near(
        &points(&made[0]),
        &[(10., 80.), (30., 80.), (55., 80.), (80., 80.)],
        "drawn",
    );
    let mirrored: Vec<_> = points(&made[0])
        .into_iter()
        .map(|p| reflect(p, axis))
        .collect();
    near(&points(&made[1]), &mirrored, "mirror");
    near(
        &points(&made[1]),
        &[(80., 10.), (80., 30.), (80., 55.), (80., 80.)],
        "mirror coordinates",
    );
}

#[test]
fn without_a_trim_crossing_the_clip_is_the_baselines_anchor_cut_and_an_axis_free_stroke_is_whole() {
    let axis = [60., 0., 60., 100.];
    let (_, made, _) = run(
        many(
            path("", &[(0., 80.), (30., 80.), (55., 80.), (80., 80.)]),
            axis,
            24.,
        ),
        vec![],
    );
    near(
        &points(&made[0]),
        &[(0., 80.), (30., 80.), (55., 80.)],
        "drawn",
    );
    near(
        &points(&made[1]),
        &[(120., 80.), (90., 80.), (65., 80.)],
        "mirror",
    );
    let (_, whole, _) = run(
        many(path("", &[(0., 80.), (20., 80.), (40., 80.)]), axis, 24.),
        vec![],
    );
    near(
        &points(&whole[0]),
        &[(0., 80.), (20., 80.), (40., 80.)],
        "drawn whole",
    );
    near(
        &points(&whole[1]),
        &[(120., 80.), (100., 80.), (80., 80.)],
        "mirror whole",
    );
}

#[test]
fn a_closed_mirror_copy_keeps_the_anchors_the_reversal_left_last() {
    // clipPathAtAxis clips the closed drawn path from its start; the copy was reflected from that clipped path and then
    // reversed, so after the clip it holds the reversal of the first `cut` anchors.
    let axis = [60., 0., 60., 100.];
    let mut drawn = path(
        "",
        &[(10., 10.), (30., 10.), (50., 30.), (80., 30.), (90., 10.)],
    );
    drawn["geometry"]["closed"] = json!(true);
    let mut mirror = mirror_of(&drawn, axis);
    mirror["geometry"]["closed"] = json!(true);
    let reversed: Vec<Value> = mirror["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .cloned()
        .collect();
    mirror["geometry"]["segments"] = json!(reversed);
    let payload = json!({"command":"object.vector.createMany","records":[drawn,mirror],"trimEnds":24,"trimClip":{"axis":axis,"mirrors":[[0,1]]}});
    let (response, made, _) = run(payload, vec![]);
    assert_eq!(response["ok"], true, "{response}");
    near(
        &points(&made[0]),
        &[(10., 10.), (30., 10.), (50., 30.)],
        "drawn",
    );
    near(
        &points(&made[1]),
        &[(70., 30.), (90., 10.), (110., 10.)],
        "mirror",
    );
}

#[test]
fn a_multi_frame_batch_clips_only_the_drawn_pair_and_its_stamps_keep_the_trimmed_whole_stroke() {
    // draw-bridge stamps the trimmed path before symmetry-bridge clips it: the frame-5 stamp is the whole trimmed stroke.
    let axis = [60., 0., 60., 100.];
    let drawn = path(
        "",
        &[(0., 80.), (30., 80.), (55., 80.), (80., 80.), (100., 80.)],
    );
    let mirror = mirror_of(&drawn, axis);
    let mut stamp = drawn.clone();
    stamp["target"]["frameScope"]["frame"] = json!(5);
    let payload = json!({"command":"object.vector.batch","trimEnds":24,"trimClip":{"axis":axis,"mirrors":[[0,1]]},
        "ops":[{"op":"create","record":drawn},{"op":"create","record":mirror},{"op":"create","record":stamp,"keyframe":"copy"}]});
    let (response, made, revision) = run(payload, vertical_walls());
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        (revision, response["result"]["historyEntriesAdded"].as_u64()),
        (1, Some(1))
    );
    near(
        &points(&made[0]),
        &[(10., 80.), (30., 80.), (55., 80.)],
        "drawn",
    );
    near(
        &points(&made[1]),
        &[(110., 80.), (90., 80.), (65., 80.)],
        "mirror",
    );
    near(
        &points(&made[2]),
        &[(10., 80.), (30., 80.), (55., 80.), (80., 80.), (100., 80.)],
        "stamp",
    );
}

fn seg(point: &Value, handle_in: &Value, handle_out: &Value) -> Value {
    json!({"point":{"x":point[0],"y":point[1]},"handleIn":{"x":handle_in[0],"y":handle_in[1]},"handleOut":{"x":handle_out[0],"y":handle_out[1]}})
}
fn ribbon(entry: &Value, width: &Value, link: Option<&str>) -> Value {
    let mut r = record_at("C", "L1", 0, "");
    r["geometry"]["closed"] = json!(true);
    r["geometry"]["segments"] = entry["outline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| seg(&s["point"], &s["handleIn"], &s["handleOut"]))
        .collect();
    r["vector"]["strokeWidth"] = width.clone();
    let centre: Vec<Value> = entry["centerSegments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let mut v = seg(&s["point"], &s["handleIn"], &s["handleOut"]);
            v["width"] = s["width"].clone();
            v
        })
        .collect();
    r["vectorExt"] =
        json!({"pressure":{"centerSegments":centre,"widthProfile":entry["widthProfile"]}});
    if let Some(id) = link {
        r["vectorExt"]["linkedFillId"] = json!(id);
    }
    r
}
fn xy(v: &Value) -> (f64, f64) {
    if v.is_array() {
        (v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
    } else {
        (v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap())
    }
}
fn outline_points(segments: &Value) -> Vec<(f64, f64)> {
    segments
        .as_array()
        .unwrap()
        .iter()
        .map(|s| xy(&s["point"]))
        .collect()
}
/// The baseline clipPathAtAxis on the oracle's trimmed outline: the index of the first anchor on the far side of the axis.
fn baseline_cut(outline: &[(f64, f64)], axis: [f64; 4]) -> usize {
    let side = |p: (f64, f64)| {
        (axis[2] - axis[0]) * (p.1 - axis[1]) - (axis[3] - axis[1]) * (p.0 - axis[0]) >= 0.
    };
    let first = side(outline[0]);
    outline
        .iter()
        .position(|p| side(*p) != first)
        .unwrap_or(outline.len())
}
const SYMMETRY_ORACLE: &str =
    include_str!("fixtures/object-vector-trim-ribbon/symmetry-oracle.json");

#[test]
fn a_pressure_ribbon_is_trimmed_on_its_whole_centreline_then_its_outline_is_clipped_as_the_baseline(
) {
    // The oracle's cases are the baseline trimStrokeEnds + symmetry (Extend on) on Paper.js: the unclipped mirror pair.
    // The clip is then the baseline's anchor cut on its trimmed outline: the drawn ribbon is cut at the first far-side
    // anchor; a copy without a linked fill is the reflected, reversed outline cut to the same anchors; a copy with a linked
    // fill is rebuilt from its whole centreline and keeps its whole outline.
    let oracle: Value = serde_json::from_str(SYMMETRY_ORACLE).unwrap();
    let axis = [60., 0., 60., 100.];
    let mut checked = 0;
    for case in oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with("mirror-"))
    {
        let name = case["name"].as_str().unwrap();
        let walls: Vec<Value> = case["walls"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, w)| wall(&format!("w-{i}"), xy(&w[0]), xy(&w[1])))
            .collect();
        let (mut records, mut ribbons) = (Vec::new(), Vec::new());
        for (k, entry) in case["input"].as_array().unwrap().iter().enumerate() {
            let link = format!("lf-{k}");
            let linked = !entry["fill"].is_null();
            if linked {
                let mut fill = record_at("C", "L1", 0, "");
                fill["geometry"]["closed"] = json!(true);
                fill["geometry"]["segments"] = entry["fill"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| seg(&s["point"], &s["handleIn"], &s["handleOut"]))
                    .collect();
                fill["vectorExt"] = json!({"isLinkedFillCompanion":true,"linkedFillId":link});
                records.push(fill);
            }
            ribbons.push(records.len());
            records.push(ribbon(
                entry,
                &case["strokeWidth"],
                linked.then_some(link.as_str()),
            ));
        }
        let payload = json!({"command":"object.vector.createMany","records":records,"trimEnds":case["max"],
            "trimClip":{"axis":axis,"mirrors":[[ribbons[0],ribbons[1]]]}});
        let (response, made, _) = run(payload, walls);
        assert_eq!(response["ok"], true, "{name}: {response}");
        let made: Vec<&Value> = made
            .iter()
            .filter(|r| !r["vectorExt"]["pressure"].is_null())
            .collect();
        let want: Vec<Vec<(f64, f64)>> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| outline_points(&e["outline"]))
            .collect();
        let cut = baseline_cut(&want[0], axis);
        assert!(
            (2..want[0].len()).contains(&cut),
            "{name}: the axis must cut the ribbon ({cut} of {})",
            want[0].len()
        );
        near(
            &outline_points(&made[0]["geometry"]["segments"]),
            &want[0][..cut],
            &format!("{name} drawn"),
        );
        let copy_linked = !case["input"][1]["fill"].is_null();
        let expected_copy: Vec<_> = if copy_linked {
            want[1].clone()
        } else {
            want[1][want[1].len() - cut..].to_vec()
        };
        near(
            &outline_points(&made[1]["geometry"]["segments"]),
            &expected_copy,
            &format!("{name} copy"),
        );
        // The centreline stays whole on both (the codec stores it as authored).
        for (k, m) in made.iter().enumerate() {
            let centre = m["vectorExt"]["pressure"]["centerSegments"]
                .as_array()
                .unwrap()
                .len();
            assert_eq!(
                centre,
                case["expected"][k]["centerSegments"]
                    .as_array()
                    .unwrap()
                    .len(),
                "{name} centre {k}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 2);
}

#[test]
fn a_stroke_left_with_one_anchor_is_refused_and_nothing_is_written() {
    let axis = [5., 0., 5., 100.];
    let (response, made, revision) = run(
        many(
            path("", &[(0., 80.), (30., 80.), (55., 80.), (100., 80.)]),
            axis,
            24.,
        ),
        vec![],
    );
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!((made.len(), revision), (0, 0));
}

#[test]
fn trim_clip_needs_trim_ends_a_create_and_a_well_formed_plan() {
    let drawn = path("", &[(0., 80.), (30., 80.), (55., 80.), (80., 80.)]);
    let mirror = mirror_of(&drawn, [60., 0., 60., 100.]);
    let good = |extra: Value| {
        let mut payload = json!({"command":"object.vector.createMany","records":[drawn.clone(),mirror.clone()],"trimEnds":24,
            "trimClip":{"axis":[60.,0.,60.,100.],"mirrors":[[0,1]]}});
        for (k, v) in extra.as_object().unwrap() {
            if v.is_null() && k == "trimEnds" {
                payload.as_object_mut().unwrap().remove(k);
            } else {
                payload[k] = v.clone();
            }
        }
        payload
    };
    let (control, _, _) = run(good(json!({})), vec![]);
    assert_eq!(control["ok"], true, "{control}");
    for (name, payload) in [
        ("no trimEnds", good(json!({"trimEnds":null}))),
        (
            "short axis",
            good(json!({"trimClip":{"axis":[60.,0.,60.],"mirrors":[[0,1]]}})),
        ),
        (
            "no mirrors",
            good(json!({"trimClip":{"axis":[60.,0.,60.,100.],"mirrors":[]}})),
        ),
        (
            "same record",
            good(json!({"trimClip":{"axis":[60.,0.,60.,100.],"mirrors":[[1,1]]}})),
        ),
        (
            "reused record",
            good(json!({"trimClip":{"axis":[60.,0.,60.,100.],"mirrors":[[0,1],[1,0]]}})),
        ),
        (
            "out of range",
            good(json!({"trimClip":{"axis":[60.,0.,60.,100.],"mirrors":[[0,7]]}})),
        ),
        ("null", good(json!({"trimClip":null}))),
        (
            "replace",
            json!({"command":"object.vector.replace","record":path("w-1",&[(1.,1.),(2.,2.)]),"trimClip":{"axis":[60.,0.,60.,100.],"mirrors":[[0,1]]}}),
        ),
    ] {
        let (refused, made, revision) = run(payload, vec![wall("w-1", (1., 1.), (2., 2.))]);
        assert_eq!(refused["ok"], false, "{name}: {refused}");
        assert_eq!((made.len(), revision), (0, 0), "{name}");
    }
}
