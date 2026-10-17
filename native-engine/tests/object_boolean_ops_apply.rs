//! L02-I boolean-tool transactions (`booleanOp`): islands and holes retained, source and style order
//! preserved, the style source's id on the first island, one revision and one Undo. Helpers live in
//! `object_boolean_ops_plan.rs`.
use super::object_boolean_ops_tests::{
    apply, bytes, cycle, cycle_of, document, four_apart, frame_records, ids_of, legacy, near, ngon,
    op_payload, owner_of, paint, plan_order, rect, ring_points, shoelace, square, state, vector,
    VectorSpec, BLUE, RED,
};
use super::*;
use serde_json::{json, Value};

#[test]
fn unite_of_apart_squares_keeps_both_islands_and_gives_the_style_id_to_the_first() {
    for version in [1, 2] {
        let mut owner = owner_of(&four_apart(version));
        let before = state(&owner);
        assert_eq!(
            apply(
                &mut owner,
                "apply",
                &op_payload("unite", "L0", 0, &["s1", "s2"])
            ),
            Ok(true)
        );
        let after = state(&owner);
        // s1 and s2 left; the stack is the survivors then the islands (left to right):
        // the left island is the first and keeps the style source's id (the LAST operand, s2)
        let ids = ids_of(&frame_records(&after, "L0", 0));
        assert_eq!(ids, ["s3", "s4", "s2", "native-stroke:1:0"]);
        if version == 2 {
            assert_eq!(
                plan_order(&after, "L0", 0),
                ["s3", "s4", "s2", "native-stroke:1:0"]
            );
        }
        let records = frame_records(&after, "L0", 0);
        assert_eq!(
            cycle(&ring_points(&records[2])),
            cycle_of(&square(0.0, 0.0, 10.0))
        );
        assert_eq!(
            cycle(&ring_points(&records[3])),
            cycle_of(&square(50.0, 0.0, 10.0))
        );
        // paint of the style source (BLUE), both islands; family kept
        for record in &records[2..] {
            assert_eq!(record["fill"], paint(BLUE));
            assert_eq!(record["family"], "closed-solid-fill-cubic-path");
            assert_eq!(record["geometry"]["closed"], true);
        }
        // everything else is untouched, byte for byte
        assert_eq!(
            frame_records(&after, "L1", 0),
            frame_records(&before, "L1", 0)
        );
        assert_eq!(records[0], frame_records(&before, "L0", 0)[2]);
        assert_eq!(owner.content_revision(), 1);
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[test]
fn one_undo_restores_the_originals_and_redo_replays_the_result() {
    let mut owner = owner_of(&four_apart(2));
    let original = bytes(&owner);
    apply(
        &mut owner,
        "apply",
        &op_payload("unite", "L0", 0, &["s1", "s2"]),
    )
    .unwrap();
    let edited = bytes(&owner);
    assert_ne!(original, edited);
    let head = owner.content_revision();
    assert_eq!(owner.undo(head), Ok(2));
    assert_eq!(bytes(&owner), original);
    assert_eq!(owner.history_depths(), (0, 1));
    let head = owner.content_revision();
    assert_eq!(owner.redo(head), Ok(3));
    assert_eq!(bytes(&owner), edited);
}

#[test]
fn subtract_with_a_bar_across_makes_two_islands_on_top() {
    let doc = document(
        2,
        vec![
            legacy("L0", "a", 0, &rect(0.0, 0.0, 100.0, 40.0), RED),
            legacy("L0", "cut", 0, &rect(40.0, -10.0, 20.0, 60.0), BLUE),
            legacy("L0", "z", 0, &square(300.0, 0.0, 10.0), RED),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("subtract", "L0", 0, &["a", "cut"]),
    )
    .unwrap();
    let after = state(&owner);
    let records = frame_records(&after, "L0", 0);
    // The style source is the LAST operand, `cut`: its paint and id go on the result.
    assert_eq!(ids_of(&records), ["z", "cut", "native-stroke:1:0"]);
    assert_eq!(
        cycle(&ring_points(&records[1])),
        cycle_of(&rect(0.0, 0.0, 40.0, 40.0))
    );
    assert_eq!(
        cycle(&ring_points(&records[2])),
        cycle_of(&rect(60.0, 0.0, 40.0, 40.0))
    );
    assert!(records[1..].iter().all(|r| r["fill"] == paint(BLUE)));
    assert_eq!(
        plan_order(&after, "L0", 0),
        ["z", "cut", "native-stroke:1:0"]
    );
}

#[test]
fn a_bite_through_the_middle_keeps_the_hole_as_a_slit_in_one_ring() {
    let doc = document(
        1,
        vec![
            legacy("L0", "a", 0, &square(0.0, 0.0, 100.0), RED),
            legacy("L0", "b", 0, &square(40.0, 40.0, 20.0), BLUE),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("subtract", "L0", 0, &["a", "b"]),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["b"]);
    let ring = ring_points(&records[0]);
    // 4 exterior + 5 hole run + 1 slit return = 10 anchors, area 100 x 100 - 20 x 20, no handles
    assert_eq!(ring.len(), 10);
    assert!(near(shoelace(&ring), 9_600.0));
    assert!(records[0]["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["handleIn"] == json!({"x":0.0,"y":0.0})
            && s["handleOut"] == json!({"x":0.0,"y":0.0})));
}

#[test]
fn a_unite_of_two_discs_is_refit_to_curves_keeping_its_anchors_inside_the_union() {
    let doc = document(
        1,
        vec![
            legacy("L0", "a", 0, &ngon(50.0, 50.0, 40.0, 24), RED),
            legacy("L0", "b", 0, &ngon(90.0, 50.0, 40.0, 24), BLUE),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("unite", "L0", 0, &["a", "b"]),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["b"]);
    let segments = records[0]["geometry"]["segments"].as_array().unwrap();
    assert!(
        segments.len() < 40 && segments.len() >= 3,
        "refit reduces anchors: {}",
        segments.len()
    );
    assert!(segments
        .iter()
        .any(|s| s["handleOut"] != json!({"x":0.0,"y":0.0})));
    // The refit is sensitive to the ring start vertex, which comes from the provider, so no refit area is
    // pinned. Start-independent invariant: every anchor is a vertex of the union, inside its exact bounds
    // (10, 10)..(130, 90).
    assert!(segments.iter().all(|s| {
        let (x, y) = (
            s["point"]["x"].as_f64().unwrap(),
            s["point"]["y"].as_f64().unwrap(),
        );
        x > 9.999 && x < 130.001 && y > 9.999 && y < 90.001
    }));
}

#[test]
fn intersect_of_overlapping_squares_leaves_the_overlap_with_the_style_id() {
    let doc = document(
        1,
        vec![
            legacy("L0", "a", 0, &square(0.0, 0.0, 100.0), RED),
            legacy("L0", "b", 0, &square(50.0, 50.0, 100.0), BLUE),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("intersect", "L0", 0, &["a", "b"]),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["b"]);
    assert_eq!(records[0]["fill"], paint(BLUE));
    let anchors = ring_points(&records[0]);
    // refit anchors are source vertices: all inside the overlap square 50..100
    assert!(anchors
        .iter()
        .all(|p| p[0] > 49.999 && p[0] < 100.001 && p[1] > 49.999 && p[1] < 100.001));
}

#[test]
fn a_vector_style_source_keeps_family_opacity_and_box_angle_and_drops_the_outline() {
    let styled = VectorSpec {
        stroke: Some([0.0, 0.2, 0.4, 1.0]),
        opacity: 0.5,
        box_angle: Some(12.0),
        ..VectorSpec::default()
    };
    let doc = document(
        1,
        vec![
            vector(
                "L0",
                "a",
                0,
                &square(0.0, 0.0, 10.0),
                &VectorSpec {
                    fill: Some(BLUE),
                    ..VectorSpec::default()
                },
            ),
            vector("L0", "b", 0, &square(50.0, 0.0, 10.0), &styled),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("unite", "L0", 0, &["a", "b"]),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["b", "native-stroke:1:0"]);
    for record in &records {
        assert_eq!(record["family"], "editable-vector-path");
        assert_eq!(record["fill"], paint([0.9, 0.1, 0.1, 1.0]));
        assert_eq!(record["vector"]["stroke"], Value::Null);
        assert_eq!(record["vector"]["hasRealStroke"], false);
        assert_eq!(record["vector"]["opacity"], 0.5);
        assert_eq!(record["vectorExt"], json!({"boxAngle":12.0}));
    }
}

#[test]
fn a_single_path_result_keeps_the_style_sources_stroke_style_and_split_islands_do_not() {
    // v1 (width 4, round, round) under v2 (width 6, round, round): the LAST operand is the style source
    let wide = VectorSpec {
        width: 6.0,
        ..VectorSpec::default()
    };
    let doc = document(
        2,
        vec![
            vector(
                "L0",
                "v1",
                0,
                &square(0.0, 0.0, 10.0),
                &VectorSpec::default(),
            ),
            vector("L0", "v2", 0, &square(5.0, 5.0, 10.0), &wide),
            vector("L0", "v3", 0, &square(100.0, 0.0, 10.0), &wide),
        ],
        None,
        None,
    );
    let style_of = |record: &Value| {
        (
            record["vector"]["stroke"].clone(),
            record["vector"]["strokeWidth"].clone(),
            record["vector"]["strokeCap"].clone(),
            record["vector"]["strokeJoin"].clone(),
        )
    };
    // overlapping squares unite into ONE plain path: baseline `booleanOp` had set width/cap/join on it
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("unite", "L0", 0, &["v1", "v2"]),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["v3", "v2"]);
    assert_eq!(
        style_of(&records[1]),
        (Value::Null, json!(6.0), json!("round"), json!("round"))
    );
    // apart squares are two children of a compound: fresh paths, Paper's defaults, still no stroke colour
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "apply",
        &op_payload("unite", "L0", 0, &["v1", "v3"]),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(records.len(), 3);
    for record in records.iter().filter(|r| r["target"]["strokeId"] != "v2") {
        assert_eq!(
            style_of(record),
            (Value::Null, json!(1.0), json!("butt"), json!("miter"))
        );
    }
}
