//! B-eraser-tool: the eraser bites what the baseline bit - a curved ribbon or shape, an open stroke, an open filled path,
//! a held cell - and a drag is ONE undo step (`joinHistory`). Expected values are hand-derived from the geometry.
use super::object_boolean_ops_tests::{
    apply, bytes, document, frame_records, ids_of, legacy, near, ngon, owner_of, rect, ring_points, send,
    shoelace, state, vector, VectorSpec, BLUE, RED,
};
use super::*;
use serde_json::{json, Value};

fn erase(layer: &str, frame: u32, id: &str, footprint: &[(f64, f64)]) -> Value {
    super::object_boolean_ops_tests::erase_payload(layer, frame, id, footprint)
}
fn circle(id: &str, cx: f64, cy: f64, r: f64) -> Value {
    let k = 0.5522847498 * r;
    let mut record = legacy("L0", id, 0, &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)], RED);
    let seg = |p: (f64, f64), i: (f64, f64), o: (f64, f64)| {
        json!({"point":{"x":p.0,"y":p.1},"handleIn":{"x":i.0,"y":i.1},"handleOut":{"x":o.0,"y":o.1}})
    };
    record["geometry"]["segments"] = json!([
        seg((cx + r, cy), (0.0, -k), (0.0, k)),
        seg((cx, cy + r), (k, 0.0), (-k, 0.0)),
        seg((cx - r, cy), (0.0, k), (0.0, -k)),
        seg((cx, cy - r), (-k, 0.0), (k, 0.0))
    ]);
    record
}
fn xs(points: &[[f64; 2]]) -> (f64, f64) {
    points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
        (lo.min(p[0]), hi.max(p[0]))
    })
}

#[test]
fn a_curved_shape_is_bitten_and_the_bite_is_one_undo() {
    let doc = document(2, vec![circle("c", 50.0, 50.0, 50.0)], None, None);
    let mut owner = owner_of(&doc);
    let before = bytes(&owner);
    // a bar across the right edge: nothing of the circle remains inside 90 < x, 20 < y < 80
    let bar = rect(90.0, 20.0, 20.0, 60.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "c", &bar)), Ok(true));
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["c"], "one island keeps the identity");
    let ring = ring_points(&records[0]);
    // the result is refitted (Path#simplify 2.5): the anchors prove where the silhouette ends, not an area
    assert!(ring.iter().all(|p| p[0] <= 93.0 || p[1] <= 23.0 || p[1] >= 77.0), "no anchor inside the bar: {ring:?}");
    assert_ne!(bytes(&owner), before, "the circle changed");
    assert_eq!(owner.history_depths(), (1, 0));
    let after = bytes(&owner);
    owner.undo(1).unwrap();
    assert_eq!(bytes(&owner), before, "one undo restores the circle");
    owner.redo(2).unwrap();
    assert_eq!(bytes(&owner), after);
}

/// The footprint the adapter sends: a stadium of 2 x 21 points (native-object-eraser-tool.js `capsule`).
fn stadium(a: (f64, f64), b: (f64, f64), r: f64) -> Vec<(f64, f64)> {
    let angle = (b.1 - a.1).atan2(b.0 - a.0);
    let arc = |c: (f64, f64), from: f64| -> Vec<(f64, f64)> {
        (0..=20)
            .map(|i| {
                let t = from + std::f64::consts::PI * i as f64 / 20.0;
                (c.0 + r * t.cos(), c.1 + r * t.sin())
            })
            .collect()
    };
    let mut out = arc(b, angle - std::f64::consts::FRAC_PI_2);
    out.extend(arc(a, angle + std::f64::consts::FRAC_PI_2));
    out
}

#[test]
fn the_adapters_capsule_footprint_cuts_a_channel_through_a_curved_shape() {
    let doc = document(2, vec![circle("c", 50.0, 50.0, 50.0)], None, None);
    let mut owner = owner_of(&doc);
    let before = bytes(&owner);
    // a 14-wide channel from the middle out through the right edge splits nothing but opens the silhouette
    let sweep = stadium((50.0, 50.0), (120.0, 50.0), 7.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "c", &sweep)), Ok(true));
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["c"]);
    assert_ne!(bytes(&owner), before);
    // the channel crosses the edge at x = 100: the ring is still one island and its anchors keep out of the channel
    let ring = ring_points(&records[0]);
    assert!(ring.iter().all(|p| !(p[0] > 72.0 && (p[1] - 50.0).abs() < 5.0)), "{ring:?}");
    // a sweep that cuts the circle in two leaves two islands, the first keeping the identity
    let mut owner = owner_of(&doc);
    let across = stadium((50.0, -20.0), (50.0, 120.0), 7.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "c", &across)), Ok(true));
    assert_eq!(ids_of(&frame_records(&state(&owner), "L0", 0)), ["c", "native-stroke:1:0"]);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn a_bite_through_the_middle_of_a_curved_shape_keeps_the_slit() {
    let doc = document(2, vec![circle("c", 50.0, 50.0, 50.0)], None, None);
    let mut owner = owner_of(&doc);
    apply(&mut owner, "erase", &erase("L0", 0, "c", &rect(40.0, 40.0, 20.0, 20.0))).unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["c"]);
    let area = shoelace(&ring_points(&records[0])).abs();
    // 7853.98 minus the 400 bite, give or take the flattening of the outline and the slit
    assert!(area > 7300.0 && area < 7500.0, "area {area}");
}

#[test]
fn an_open_stroke_is_cut_in_two_islands_painted_in_its_stroke_colour() {
    let line = VectorSpec {
        fill: None,
        stroke: Some(BLUE),
        width: 10.0,
        closed: false,
        ..VectorSpec::default()
    };
    let doc = document(
        2,
        vec![vector("L0", "line", 0, &[(0.0, 50.0), (200.0, 50.0)], &line)],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let before = bytes(&owner);
    let bar = rect(90.0, 0.0, 20.0, 100.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "line", &bar)), Ok(true));
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["line", "native-stroke:1:0"]);
    for record in &records {
        assert_eq!(record["fill"], json!({"kind":"solid","r":0.0,"g":0.0,"b":1.0,"a":1.0}));
        assert_eq!(record["geometry"]["closed"], json!(true));
    }
    let (left_min, left_max) = xs(&ring_points(&records[0]));
    let (right_min, right_max) = xs(&ring_points(&records[1]));
    assert!(left_min < -3.0 && left_max < 93.0 && left_max > 88.0, "{left_min} {left_max}");
    assert!(right_min > 107.0 && right_min < 112.0 && right_max > 203.0, "{right_min} {right_max}");
    owner.undo(1).unwrap();
    assert_eq!(bytes(&owner), before);
}

#[test]
fn an_open_filled_path_is_bitten_as_its_closed_fill() {
    let open = VectorSpec {
        closed: false,
        ..VectorSpec::default()
    };
    let doc = document(
        1,
        vec![vector("L0", "t", 0, &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)], &open)],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let corner = rect(80.0, -10.0, 40.0, 30.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "t", &corner)), Ok(true));
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["t"]);
    let ring = ring_points(&records[0]);
    // the result is refitted (Path#simplify), so the anchors are checked, not an area: the corner is gone, the rest stays
    assert!(ring.len() >= 4, "{ring:?}");
    assert!(ring.iter().all(|p| !(p[0] > 80.5 && p[1] < 19.5)), "no anchor in the bitten corner: {ring:?}");
    assert!(ring.iter().any(|p| near(p[0], 100.0) && p[1] > 90.0), "the far side stays: {ring:?}");
}

#[test]
fn a_footprint_that_misses_a_curved_or_open_target_changes_nothing() {
    let line = VectorSpec { fill: None, stroke: Some(BLUE), closed: false, ..VectorSpec::default() };
    let doc = document(
        1,
        vec![
            circle("c", 50.0, 50.0, 50.0),
            vector("L0", "line", 0, &[(300.0, 0.0), (400.0, 0.0)], &line),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let before = bytes(&owner);
    let far = rect(1000.0, 1000.0, 10.0, 10.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "c", &far)), Ok(false));
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 0, "line", &far)), Ok(false));
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
}

fn held() -> Value {
    let mut d = document(2, vec![legacy("L0", "t", 0, &rect(0.0, 0.0, 100.0, 100.0), RED)], None, None);
    d["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
        "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    d
}

#[test]
fn a_bite_on_a_held_cell_keys_it_in_the_same_undo_step() {
    let mut owner = owner_of(&held());
    let before = bytes(&owner);
    let bar = rect(40.0, -10.0, 20.0, 120.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 2, "t", &bar)), Ok(true));
    let after = state(&owner);
    assert_eq!(after["layers"][0]["timing"]["keys"], json!([{"frame":0},{"frame":2}]));
    assert_eq!(frame_records(&after, "L0", 0).len(), 1, "the key the cell held is untouched");
    assert_eq!(ids_of(&frame_records(&after, "L0", 2)), ["t", "native-stroke:1:0"]);
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    assert_eq!(bytes(&owner), before, "the key and the bite go in one undo");
}

#[test]
fn a_miss_on_a_held_cell_leaves_it_held() {
    let mut owner = owner_of(&held());
    let before = bytes(&owner);
    let far = rect(1000.0, 1000.0, 10.0, 10.0);
    assert_eq!(apply(&mut owner, "erase", &erase("L0", 2, "t", &far)), Ok(false));
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
}

fn joined(footprint: &[(f64, f64)]) -> Value {
    let mut payload = erase("L0", 0, "t", footprint);
    payload["joinHistory"] = json!(true);
    payload
}
/// A 64-gon, so that every bite of a drag meets the silhouette the previous one left (a refitted result of few corners
/// is a different shape).
fn square_doc() -> Value {
    document(2, vec![legacy("L0", "t", 0, &ngon(50.0, 50.0, 50.0, 64), RED)], None, None)
}
fn notch(x: f64) -> Vec<(f64, f64)> {
    rect(x, 40.0, 25.0, 20.0)
}

#[test]
fn bites_that_join_are_one_history_entry_and_one_undo() {
    let mut owner = owner_of(&square_doc());
    let before = bytes(&owner);
    let first = send(&mut owner, "a", "erase", &erase("L0", 0, "t", &notch(-10.0)));
    assert_eq!(first["result"], json!({"applied":true,"historyEntriesAdded":1}));
    let second = send(&mut owner, "b", "erase", &joined(&notch(5.0)));
    assert_eq!(second["result"], json!({"applied":true,"historyEntriesAdded":0}));
    assert_eq!(second["contentRevision"], 2, "every bite is a revision");
    let third = send(&mut owner, "c", "erase", &joined(&notch(20.0)));
    assert_eq!(third["result"], json!({"applied":true,"historyEntriesAdded":0}));
    assert_eq!(third["contentRevision"], 3);
    assert_eq!(owner.history_depths(), (1, 0), "three bites, one entry");
    let ring = ring_points(&frame_records(&state(&owner), "L0", 0)[0]);
    assert!(ring.iter().all(|p| !(p[0] < 40.0 && p[1] > 42.5 && p[1] < 57.5)), "the notch is cut to x = 45: {ring:?}");
    let after = bytes(&owner);
    owner.undo(3).unwrap();
    assert_eq!(bytes(&owner), before, "one undo takes back the whole drag");
    assert_eq!(owner.history_depths(), (0, 1));
    owner.redo(4).unwrap();
    assert_eq!(bytes(&owner), after, "one redo puts the whole drag back");
}

#[test]
fn a_joined_bite_that_changes_nothing_adds_nothing() {
    let mut owner = owner_of(&square_doc());
    send(&mut owner, "a", "erase", &erase("L0", 0, "t", &notch(-10.0)));
    let revision = owner.content_revision();
    let miss = send(&mut owner, "b", "erase", &joined(&rect(1000.0, 1000.0, 10.0, 10.0)));
    assert_eq!(miss["result"], json!({"applied":false,"historyEntriesAdded":0}));
    assert_eq!(owner.content_revision(), revision);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn a_join_with_nothing_to_continue_is_refused_and_never_a_second_entry() {
    let mut owner = owner_of(&square_doc());
    let before = bytes(&owner);
    let refused = send(&mut owner, "a", "erase", &joined(&notch(-10.0)));
    assert_eq!(refused["ok"], json!(false));
    assert_eq!(refused["error"]["code"], json!("unavailable"));
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
    // an undo in between breaks the chain: the next bite cannot reach back past it
    send(&mut owner, "b", "erase", &erase("L0", 0, "t", &notch(-10.0)));
    owner.undo(1).unwrap();
    let broken = send(&mut owner, "c", "erase", &joined(&notch(5.0)));
    assert_eq!(broken["error"]["code"], json!("unavailable"));
    assert_eq!(owner.history_depths(), (0, 1), "redo survives the refusal");
}

#[test]
fn only_the_eraser_may_join_and_only_with_a_boolean() {
    let mut owner = owner_of(&square_doc());
    let mut payload = erase("L0", 0, "t", &notch(-10.0));
    payload["joinHistory"] = json!("yes");
    let response = send(&mut owner, "a", "erase", &payload);
    assert_eq!(response["ok"], json!(false));
    assert_eq!(response["error"]["code"], json!("invalid_request"));
    assert_eq!(owner.history_depths(), (0, 0));
}
