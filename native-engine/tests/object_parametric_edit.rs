//! L01-G (G22/G13): the sealed parameter edit and the path-effect stack edit through the real dispatch caller,
//! the existing history and the existing revision owner. Expected values are Paper.js oracle cells
//! (fixtures/object-parametric-edit/oracle.txt, generated from the baseline at fbe4b8a3) and hand-written
//! literals. The anchors are rebuilt by the evaluation-layer kernels, so those tests run only with the
//! `evaluation` feature; without it the same edits must refuse with `unavailable` and change nothing.
use serde_json::{json, Value};

#[path = "object_parametric_edit_controls.rs"]
mod controls;
#[path = "object_path_effects_edit.rs"]
mod path_effects;
#[path = "object_parametric_edit_refusals.rs"]
mod refusals;
#[path = "object_parametric_edit_support.rs"]
mod support;
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod vsupport;

use support::*;
#[cfg_attr(not(feature = "evaluation"), allow(unused_imports))]
use vsupport::{bytes, document, objects, owner_of, state, target};

/// The edited record `s-1` on layer `L1` (frame 0) beside an unrelated plain vector `s-2` on `L2`.
fn shape_doc(version: u32, ext: Option<Value>, segments: &Segs) -> Value {
    let shape = record("s-1", "L1", segments, true, ext);
    let other = record("s-2", "L2", &straight_rect(), true, None);
    document(version, vec![shape, other], None)
}

fn t1() -> Value {
    target("L1", 0, "s-1")
}

fn rect_ext() -> Value {
    json!({"paramShape": {"kind": "rect", "box": {"x1": 0, "y1": 0, "x2": 100, "y2": 60}}})
}

fn rect_owner() -> crate::history::NativeObjectHistory {
    owner_of(&shape_doc(1, Some(rect_ext()), &straight_rect()))
}

#[test]
fn oracle_is_pinned_to_the_baseline_and_complete() {
    assert_eq!(meta("baseline"), BASELINE);
    assert_eq!(
        meta("slice-sha256-tools"),
        "d28e1d4ca4c1330482444e2da58dabc7ef629ae7b52e7d197122af4d47bf97b9"
    );
    assert_eq!(
        meta("sha256-paper"),
        "13526b252be3856db63b096ca282270454a3bb4567943e137875d9ced54ce9ba"
    );
    assert_eq!(
        meta("sha256-tools.js"),
        "20d4fdc0c717e497821f305226383ff8e3d059faa687ec70256ee60eb62e4979"
    );
    for (kind, key) in [("tr", "rows-tr"), ("sc", "rows-sc"), ("st", "rows-st")] {
        assert_eq!(rows(kind).len().to_string(), meta(key), "{kind}");
    }
    assert_eq!(
        (rows("tr").len(), rows("sc").len(), rows("st").len()),
        (74, 102, 32)
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn set_rows_rebuild_what_the_baseline_panel_commit_builds() {
    let mut checked = 0;
    for row in rows("st") {
        let (label, spec, box_in, angle) = (row[1], row[2], frame(row[3]), hexf(row[4]));
        let ext = ext_json(&blank_spec(spec), box_in, angle);
        let mut owner = owner_of(&shape_doc(1, Some(ext), &segs(row[5])));
        let before = bytes(&owner);
        let response = send(&mut owner, label, OP_PARAM, set(t1(), spec_json(spec)));
        if label == "rect-selfheal-curved" {
            // The baseline stamps the missing box from the CURVED path's bounds; native curve bounds are not
            // delivered by this leaf, so the edit refuses instead of guessing a frame.
            assert_eq!(refusal(&response), "unavailable");
            assert_eq!(bytes(&owner), before);
            continue;
        }
        assert_eq!(response["ok"], true, "{label}: {response}");
        let after = state(&owner);
        let object = &objects(&after)[0];
        let (got, want) = (stored_segments(object), segs(row[6]));
        let outcome = if spec.starts_with('R') && angle == 0.0 {
            same_bits(&got, &want)
        } else {
            within_bound(&got, &want)
        };
        outcome.unwrap_or_else(|why| panic!("{label} {spec}: {why}"));
        assert_eq!(stored_frame(object), frame(row[7]), "{label} box");
        assert_eq!(object["geometry"]["closed"], true, "{label}");
        checked += 1;
    }
    assert_eq!(checked, 31);
}

#[cfg(feature = "evaluation")]
#[test]
fn translate_rows_match_paper_bit_for_bit_through_dispatch() {
    let mut checked = 0;
    for row in rows("tr") {
        let (label, spec, box_in) = (row[1], row[2], frame(row[3]));
        let [dx, dy] = pair(row[4]);
        let ext = (spec != "-").then(|| ext_json(spec, box_in, 0.0));
        let mut owner = owner_of(&shape_doc(1, ext, &segs(row[5])));
        let response = send(&mut owner, label, OP_PARAM, translate(t1(), dx, dy));
        if spec == "-" {
            assert_eq!(refusal(&response), "unavailable", "{label}");
            continue;
        }
        assert_eq!(response["ok"], true, "{label}: {response}");
        let after = state(&owner);
        let object = &objects(&after)[0];
        same_bits(&stored_segments(object), &segs(row[6]))
            .unwrap_or_else(|why| panic!("{label} {}: {why}", row[4]));
        assert_eq!(stored_frame(object), frame(row[7]), "{label} box");
        assert_eq!(object["geometry"]["closed"], true, "{label}");
        checked += 1;
    }
    assert_eq!(checked, 73);
}

#[cfg(feature = "evaluation")]
#[test]
fn scale_rows_match_paper_and_rebuild_a_rect_from_the_scaled_box() {
    let mut checked = 0;
    for row in rows("sc") {
        let (label, spec, box_in, angle) = (row[1], row[2], frame(row[3]), hexf(row[4]));
        let (factors, pivot) = (pair(row[5]), pair(row[6]));
        let ext = (spec != "-").then(|| ext_json(spec, box_in, angle));
        let mut owner = owner_of(&shape_doc(1, ext, &segs(row[7])));
        let response = send(&mut owner, label, OP_PARAM, scale(t1(), factors, pivot));
        if spec == "-" {
            assert_eq!(refusal(&response), "unavailable", "{label}");
            continue;
        }
        assert_eq!(response["ok"], true, "{label}: {response}");
        let after = state(&owner);
        let object = &objects(&after)[0];
        let (got, want) = (stored_segments(object), segs(row[9]));
        // Only a rebuilt rect with a non-zero boxAngle goes through cos/sin.
        let rotated = spec.starts_with('R') && box_in.is_some() && angle != 0.0;
        let outcome = if rotated {
            within_bound(&got, &want)
        } else {
            same_bits(&got, &want)
        };
        outcome.unwrap_or_else(|why| panic!("{label} {}: {why}", row[5]));
        assert_eq!(stored_frame(object), frame(row[10]), "{label} box");
        assert_eq!(object["geometry"]["closed"], true, "{label}");
        checked += 1;
    }
    assert_eq!(checked, 101);
}

#[cfg(feature = "evaluation")]
#[test]
fn set_rect_corner_radius_writes_baked_anchors_and_intent_literally() {
    let mut owner = rect_owner();
    let before = state(&owner);
    let response = send(
        &mut owner,
        "r1",
        OP_PARAM,
        set(t1(), json!({"kind": "rect", "tl": 8})),
    );
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"],
        json!({"applied": true, "historyEntriesAdded": 1})
    );
    let after = state(&owner);
    let object = &objects(&after)[0];
    // 8 is a power of two, so 8 * 0.5522847498 is exactly the decimal below.
    let k = 4.4182779984;
    let want: Segs = vec![
        [8.0, 0.0, -k, 0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [100.0, 60.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 60.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 8.0, 0.0, 0.0, 0.0, -k],
    ];
    assert_eq!(stored_segments(object), want);
    assert_eq!(object["geometry"]["closed"], true);
    assert_eq!(
        object["vectorExt"]["paramShape"],
        json!({"kind": "rect", "tl": 8, "box": {"x1": 0.0, "y1": 0.0, "x2": 100.0, "y2": 60.0}})
    );
    assert_eq!(objects(&after)[1], objects(&before)[1], "bystander record");
    assert_eq!(after["layers"], before["layers"]);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[cfg(feature = "evaluation")]
#[test]
fn repeating_the_same_parameters_is_a_no_op_that_keeps_history_and_redo() {
    let mut owner = rect_owner();
    let first = set(t1(), json!({"kind": "rect", "tl": 8}));
    let second = set(t1(), json!({"kind": "rect", "tl": 16, "br": 4}));
    assert!(applied(&send(&mut owner, "a", OP_PARAM, first.clone())));
    assert!(applied(&send(&mut owner, "b", OP_PARAM, second)));
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(owner.history_depths(), (1, 1));
    let content = bytes(&owner);
    let again = send(&mut owner, "a2", OP_PARAM, first);
    assert!(!applied(&again), "{again}");
    assert_eq!(again["result"]["historyEntriesAdded"], 0);
    assert_eq!(again["contentRevision"], 3);
    assert_eq!(owner.content_revision(), 3);
    assert_eq!(owner.history_depths(), (1, 1), "redo survives the no-op");
    assert_eq!(bytes(&owner), content);
    assert_eq!(owner.redo(3), Ok(4));
    let redone = state(&owner);
    assert_eq!(redone["objects"][0]["vectorExt"]["paramShape"]["tl"], 16);
}

#[cfg(feature = "evaluation")]
#[test]
fn translate_moves_anchors_and_box_together_and_undo_restores_the_bytes() {
    let mut owner = rect_owner();
    let before = bytes(&owner);
    let response = send(&mut owner, "t", OP_PARAM, translate(t1(), 10.0, -5.0));
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    let object = &objects(&after)[0];
    let want: Segs = vec![
        [10.0, -5.0, 0.0, 0.0, 0.0, 0.0],
        [110.0, -5.0, 0.0, 0.0, 0.0, 0.0],
        [110.0, 55.0, 0.0, 0.0, 0.0, 0.0],
        [10.0, 55.0, 0.0, 0.0, 0.0, 0.0],
    ];
    assert_eq!(stored_segments(object), want);
    assert_eq!(stored_frame(object), Some([10.0, -5.0, 110.0, 55.0]));
    let moved = bytes(&owner);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(bytes(&owner), before, "undo restores the exact prior bytes");
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(bytes(&owner), moved);
}

#[cfg(feature = "evaluation")]
#[test]
fn negative_scale_reorders_the_box_and_rebuilds_the_rect_from_it() {
    let mut owner = rect_owner();
    let response = send(
        &mut owner,
        "s",
        OP_PARAM,
        scale(t1(), [-2.0, 0.5], [0.0, 0.0]),
    );
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    let object = &objects(&after)[0];
    // x: min(0, -200) .. max(0, -200); y: 0 .. 30. The rect is rebuilt from the new box (radii unchanged: none).
    assert_eq!(stored_frame(object), Some([-200.0, 0.0, 0.0, 30.0]));
    let want: Segs = vec![
        [-200.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 30.0, 0.0, 0.0, 0.0, 0.0],
        [-200.0, 30.0, 0.0, 0.0, 0.0, 0.0],
    ];
    assert_eq!(stored_segments(object), want);
}

#[cfg(feature = "evaluation")]
#[test]
fn a_box_less_shape_moves_its_anchors_and_stamps_nothing() {
    let polyline: Segs = vec![
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [10.0, 10.0, 0.0, 0.0, 0.0, 0.0],
    ];
    let ext = json!({"paramShape": {"kind": "ellipse"}});
    let mut owner = owner_of(&shape_doc(1, Some(ext), &polyline));
    let response = send(&mut owner, "t", OP_PARAM, translate(t1(), 3.0, 4.0));
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    let object = &objects(&after)[0];
    let want: Segs = vec![
        [3.0, 4.0, 0.0, 0.0, 0.0, 0.0],
        [13.0, 4.0, 0.0, 0.0, 0.0, 0.0],
        [13.0, 14.0, 0.0, 0.0, 0.0, 0.0],
    ];
    assert_eq!(stored_segments(object), want);
    assert_eq!(stored_frame(object), None);
    assert_eq!(
        object["vectorExt"]["paramShape"],
        json!({"kind": "ellipse"})
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn the_edit_touches_only_its_record_and_leaves_the_order_plan_alone() {
    let mut owner = owner_of(&shape_doc(2, Some(rect_ext()), &straight_rect()));
    let before = state(&owner);
    assert!(before.get("compositionOrder").is_some());
    let response = send(&mut owner, "t", OP_PARAM, translate(t1(), 1.5, 2.5));
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    assert_eq!(after["compositionOrder"], before["compositionOrder"]);
    assert_eq!(after["layers"], before["layers"]);
    assert_eq!(objects(&after)[1], objects(&before)[1]);
    assert_eq!(objects(&after).len(), 2);
    assert_ne!(objects(&after)[0], objects(&before)[0]);
}

#[cfg(feature = "evaluation")]
#[test]
fn a_replayed_request_does_not_execute_twice_and_a_changed_body_is_refused() {
    let mut owner = rect_owner();
    let envelope = request(&owner, "same", OP_PARAM, translate(t1(), 4.0, 4.0));
    let first = vsupport::dispatch(&mut owner, &envelope);
    assert_eq!(first["ok"], true, "{first}");
    let second = vsupport::dispatch(&mut owner, &envelope);
    assert_eq!(second, first, "retained receipt replays");
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let mut changed = envelope.clone();
    changed["payload"] = translate(t1(), 5.0, 5.0);
    let refused = vsupport::dispatch(&mut owner, &changed);
    assert_eq!(refusal(&refused), "invalid_request");
    assert_eq!(owner.content_revision(), 1);
}

#[cfg(not(feature = "evaluation"))]
#[test]
fn without_the_evaluation_layer_every_parameter_edit_is_unavailable_and_changes_nothing() {
    let payloads = [
        set(t1(), json!({"kind": "rect", "tl": 8})),
        translate(t1(), 1.0, 2.0),
        scale(t1(), [2.0, 2.0], [0.0, 0.0]),
    ];
    for (index, payload) in payloads.into_iter().enumerate() {
        let mut owner = rect_owner();
        let before = bytes(&owner);
        let response = send(&mut owner, &format!("u{index}"), OP_PARAM, payload);
        assert_eq!(refusal(&response), "unavailable", "{response}");
        assert_eq!(bytes(&owner), before);
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(owner.history_depths(), (0, 0));
    }
}
