//! #2356: the owner's point-type semantics (Ilya, 2026-10-31) on the geometry kernel. The frozen 1065e9d8 oracles stay
//! the evidence for everything the decision did not change; `baseline_intent` maps each frozen case onto the part of it
//! that still holds (Corner is now `collapse`, and a case whose rule the decision replaced is compared by the new rules here).
use crate::object_document::subselect::point_type::{current_tangent, infer, Kind};
use crate::object_document::subselect::{edit, edit_typed, Anchor, Intent, Refusal};

/// The frozen case's intent as the baseline outcome still holds for it, or `None` when the decision replaced its rule.
pub(crate) fn baseline_intent(source: &[Anchor], intent: &Intent) -> Option<Intent> {
    match intent {
        Intent::PointType {
            indices,
            point_type,
        } if point_type == "corner" => Some(Intent::PointType {
            indices: indices.clone(),
            point_type: "collapse".into(),
        }),
        Intent::PointType { indices, .. } => indices
            .iter()
            .all(|i| current_tangent(&source[*i]).is_none())
            .then(|| intent.clone()),
        Intent::Handle {
            index,
            break_pair: false,
            ..
        } => (infer(&source[*index]) == Kind::Symmetric).then(|| intent.clone()),
        _ => Some(intent.clone()),
    }
}

fn a(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> Anchor {
    Anchor {
        point,
        handle_in,
        handle_out,
    }
}
fn near(x: [f64; 2], y: [f64; 2]) -> bool {
    (x[0] - y[0]).abs() < 1e-9 && (x[1] - y[1]).abs() < 1e-9
}
fn len(v: [f64; 2]) -> f64 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}
/// Three vertices; the middle one has two handles at an angle (a corner) of different lengths.
fn path() -> Vec<Anchor> {
    vec![
        a([0.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
        a([50.0, 20.0], [-10.0, 5.0], [6.0, 8.0]),
        a([100.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
    ]
}
fn point_type(index: usize, kind: &str) -> Intent {
    Intent::PointType {
        indices: vec![index],
        point_type: kind.into(),
    }
}
fn drag(index: usize, handle: &str, point: [f64; 2], break_pair: bool) -> Intent {
    Intent::Handle {
        index,
        handle: handle.into(),
        point,
        break_pair,
        paired_point: Default::default(),
    }
}

#[test]
fn smooth_keeps_the_current_tangent_and_both_lengths_and_symmetric_equalises_them() {
    let before = path();
    let (li, lo) = (len(before[1].handle_in), len(before[1].handle_out));
    let (o, i) = ([6.0 / lo, 8.0 / lo], [10.0 / li, -5.0 / li]);
    let sum = [o[0] + i[0], o[1] + i[1]];
    let dir = [sum[0] / len(sum), sum[1] / len(sum)];
    let (smooth, hints) =
        edit_typed(&before, None, false, false, &point_type(1, "smooth")).unwrap();
    assert_eq!(smooth[1].point, before[1].point, "the anchor never moves");
    assert!(near(smooth[1].handle_out, [dir[0] * lo, dir[1] * lo]));
    assert!(
        near(smooth[1].handle_in, [-dir[0] * li, -dir[1] * li]),
        "smooth keeps each length"
    );
    assert_eq!(
        (smooth[0].clone(), smooth[2].clone()),
        (before[0].clone(), before[2].clone())
    );
    assert_eq!(
        hints,
        Some(vec![Kind::Symmetric, Kind::Smooth, Kind::Symmetric])
    );
    let (sym, hints) =
        edit_typed(&before, None, false, false, &point_type(1, "symmetric")).unwrap();
    let mean = (li + lo) / 2.0;
    assert_eq!(sym[1].point, before[1].point);
    assert!(near(sym[1].handle_out, [dir[0] * mean, dir[1] * mean]));
    assert!(near(sym[1].handle_in, [-dir[0] * mean, -dir[1] * mean]));
    assert_eq!(hints.unwrap()[1], Kind::Symmetric);
    // Switching Smooth <-> Symmetric twice never rotates the placed tangent.
    let again = edit(&smooth, false, false, &point_type(1, "symmetric")).unwrap();
    let back = edit(&again, false, false, &point_type(1, "smooth")).unwrap();
    for pair in [&smooth, &again, &back] {
        assert!(near(
            pair[1].handle_out,
            [
                dir[0] * len(pair[1].handle_out),
                dir[1] * len(pair[1].handle_out)
            ]
        ));
    }
}

#[test]
fn a_lone_handle_gives_the_direction_and_no_handle_falls_back_to_the_neighbour_chord() {
    let mut one = path();
    one[1] = a([50.0, 20.0], [0.0, 0.0], [0.0, 30.0]);
    let out = edit(&one, false, false, &point_type(1, "smooth")).unwrap();
    assert!(
        near(out[1].handle_out, [0.0, 30.0])
            && out[1].handle_in[0] == 0.0
            && out[1].handle_in[1] < 0.0
    );
    let mut lone_in = path();
    lone_in[1] = a([50.0, 20.0], [-20.0, 0.0], [0.0, 0.0]);
    let out = edit(&lone_in, false, false, &point_type(1, "smooth")).unwrap();
    assert!(
        out[1].handle_out[0] > 0.0 && out[1].handle_out[1] == 0.0,
        "the reversed in handle points out"
    );
    let mut bare = path();
    bare[1] = a([50.0, 20.0], [0.0, 0.0], [0.0, 0.0]);
    let out = edit(&bare, false, false, &point_type(1, "symmetric")).unwrap();
    let chord = [100.0, 0.0];
    let want = [chord[0] / len(chord), chord[1] / len(chord)];
    assert!(
        out[1].handle_out[0] > 0.0 && out[1].handle_out[1].abs() < 1e-9,
        "chord direction {want:?}"
    );
    assert!(near(out[1].handle_in, [-out[1].handle_out[0], 0.0]));
    // Handles pointing the same way cancel in the bisector: the chord decides.
    let mut cusp = path();
    cusp[1] = a([50.0, 20.0], [-10.0, 0.0], [-10.0, 0.0]);
    let out = edit(&cusp, false, false, &point_type(1, "symmetric")).unwrap();
    assert!(out[1].handle_out[0] > 0.0);
}

#[test]
fn corner_keeps_both_handles_and_collapse_removes_them_each_recording_a_corner() {
    let before = path();
    let (corner, hints) =
        edit_typed(&before, None, false, false, &point_type(1, "corner")).unwrap();
    assert_eq!(
        corner, before,
        "Corner breaks the angle and keeps the handles"
    );
    assert_eq!(
        hints,
        Some(vec![Kind::Symmetric, Kind::Corner, Kind::Symmetric])
    );
    let (collapsed, hints) =
        edit_typed(&before, None, false, false, &point_type(1, "collapse")).unwrap();
    assert_eq!(collapsed[1], a([50.0, 20.0], [0.0, 0.0], [0.0, 0.0]));
    assert_eq!(
        (collapsed[0].clone(), collapsed[2].clone()),
        (before[0].clone(), before[2].clone())
    );
    assert_eq!(hints.unwrap()[1], Kind::Corner);
    assert_eq!(
        edit(&before, false, false, &point_type(1, "nope")),
        Err(Refusal::Invalid)
    );
}

#[test]
fn a_plain_handle_drag_follows_the_vertex_type() {
    // Symmetric: the opposite handle mirrors the drag.
    let sym = vec![a([0.0, 0.0], [-10.0, 0.0], [10.0, 0.0])];
    let (out, _) = edit_typed(
        &sym,
        None,
        false,
        false,
        &drag(0, "handleOut", [0.0, 6.0], false),
    )
    .unwrap();
    assert!(near(out[0].handle_out, [0.0, 6.0]) && near(out[0].handle_in, [0.0, -6.0]));
    // Smooth: the opposite stays on the line and keeps its own length.
    let smooth = vec![a([0.0, 0.0], [-20.0, 0.0], [10.0, 0.0])];
    assert_eq!(infer(&smooth[0]), Kind::Smooth);
    let (out, hints) = edit_typed(
        &smooth,
        None,
        false,
        false,
        &drag(0, "handleOut", [0.0, 6.0], false),
    )
    .unwrap();
    assert!(near(out[0].handle_out, [0.0, 6.0]) && near(out[0].handle_in, [0.0, -20.0]));
    assert_eq!(hints, None, "a plain drag records nothing");
    let (out, _) = edit_typed(
        &smooth,
        None,
        false,
        false,
        &drag(0, "handleIn", [-3.0, -4.0], false),
    )
    .unwrap();
    assert!(
        near(out[0].handle_in, [-3.0, -4.0]) && near(out[0].handle_out, [6.0, 8.0]),
        "length 10 on the opposite line"
    );
    // Corner (inferred from a bent pair): the opposite is untouched.
    let bent = path();
    let (out, _) = edit_typed(
        &bent,
        None,
        false,
        false,
        &drag(1, "handleOut", [70.0, 0.0], false),
    )
    .unwrap();
    assert_eq!(out[1].handle_in, bent[1].handle_in);
    assert!(near(out[1].handle_out, [20.0, -20.0]));
    // A stored hint beats the geometry: a collinear equal pair typed Corner is dragged independently.
    let (out, _) = edit_typed(
        &sym,
        Some(&[Kind::Corner]),
        false,
        false,
        &drag(0, "handleOut", [0.0, 6.0], false),
    )
    .unwrap();
    assert_eq!(out[0].handle_in, sym[0].handle_in);
    // A bare vertex (no handles, nothing typed) still pulls a mirrored pair out.
    let bare = vec![a([5.0, 5.0], [0.0, 0.0], [0.0, 0.0])];
    let (out, _) = edit_typed(
        &bare,
        None,
        false,
        false,
        &drag(0, "handleOut", [15.0, 5.0], false),
    )
    .unwrap();
    assert!(near(out[0].handle_in, [-10.0, 0.0]));
}

#[test]
fn alt_drag_is_independent_and_records_a_corner_and_collapse_then_drag_stays_one_sided() {
    let sym = vec![
        a([0.0, 0.0], [-10.0, 0.0], [10.0, 0.0]),
        a([40.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
    ];
    let (out, hints) = edit_typed(
        &sym,
        None,
        false,
        false,
        &drag(0, "handleOut", [0.0, 6.0], true),
    )
    .unwrap();
    assert!(near(out[0].handle_out, [0.0, 6.0]));
    assert_eq!(
        out[0].handle_in, sym[0].handle_in,
        "Alt leaves the opposite handle where it is"
    );
    assert_eq!(hints, Some(vec![Kind::Corner, Kind::Symmetric]));
    let (collapsed, hints) =
        edit_typed(&sym, None, false, false, &point_type(0, "collapse")).unwrap();
    let (out, _) = edit_typed(
        &collapsed,
        hints.as_deref(),
        false,
        false,
        &drag(0, "handleOut", [0.0, 6.0], false),
    )
    .unwrap();
    assert!(near(out[0].handle_out, [0.0, 6.0]) && out[0].handle_in == [0.0, 0.0]);
}

#[test]
fn the_hint_array_stays_aligned_through_insert_delete_toggle_and_transforms() {
    let src = vec![
        a([0.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
        a([50.0, 20.0], [-10.0, 5.0], [6.0, 8.0]),
        a([100.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
    ];
    let hints = [Kind::Corner, Kind::Smooth, Kind::Symmetric];
    let run = |intent: Intent| edit_typed(&src, Some(&hints), false, false, &intent).unwrap();
    let (moved, h) = run(Intent::Move {
        indices: vec![0, 1],
        delta: [3.0, 4.0],
    });
    assert_eq!((moved.len(), h.unwrap()), (3, hints.to_vec()));
    let (_, h) = run(Intent::Rotate {
        indices: vec![1],
        degrees: 30.0,
        center: [0.0, 0.0],
    });
    assert_eq!(h.unwrap(), hints.to_vec());
    let (inserted, h) = run(Intent::Insert {
        curve: 0,
        time: 0.5,
    });
    let h = h.unwrap();
    assert_eq!((inserted.len(), h.len()), (4, 4));
    assert_eq!(
        (h[0], h[2], h[3]),
        (Kind::Corner, Kind::Smooth, Kind::Symmetric),
        "old entries shift, the new vertex is inserted"
    );
    assert_eq!(h[1], infer(&inserted[1]));
    let (deleted, h) = run(Intent::Delete { indices: vec![0] });
    assert_eq!(
        (deleted.len(), h.unwrap()),
        (2, vec![Kind::Smooth, Kind::Symmetric])
    );
    let (_, h) = run(Intent::Toggle { index: 1 });
    assert_eq!(
        h.unwrap()[1],
        Kind::Corner,
        "toggling handles off is a corner"
    );
    let (toggled, h) = run(Intent::Toggle { index: 0 });
    assert_eq!(h.unwrap()[0], infer(&toggled[0]));
    // Without an array nothing is invented by an edit that sets no type.
    for intent in [
        Intent::Move {
            indices: vec![0],
            delta: [1.0, 1.0],
        },
        Intent::Insert {
            curve: 1,
            time: 0.5,
        },
        Intent::Delete { indices: vec![2] },
        Intent::Toggle { index: 1 },
        drag(1, "handleIn", [40.0, 20.0], false),
    ] {
        assert_eq!(
            edit_typed(&src, None, false, false, &intent).unwrap().1,
            None,
            "{intent:?}"
        );
    }
    // A hint array of the wrong length is refused, and a pressure centreline never carries one.
    assert_eq!(
        edit_typed(
            &src,
            Some(&hints[..2]),
            false,
            false,
            &point_type(0, "smooth")
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        edit_typed(&src, None, false, true, &point_type(1, "corner"))
            .unwrap()
            .1,
        None
    );
}

#[test]
fn every_frozen_point_type_case_keeps_its_anchor_and_follows_the_documented_rule() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/oracle.json")).unwrap();
    let mut seen = 0;
    for case in oracle["cases"].as_array().unwrap() {
        let source: Vec<Anchor> = serde_json::from_value(case["segments"].clone()).unwrap();
        let intent: Intent = serde_json::from_value(case["intent"].clone()).unwrap();
        let Intent::PointType {
            indices,
            point_type,
        } = &intent
        else {
            continue;
        };
        let (closed, pressure) = (
            case["closed"].as_bool().unwrap(),
            case["pressure"].as_bool().unwrap(),
        );
        let (after, hints) = edit_typed(&source, None, closed, pressure, &intent).unwrap();
        seen += 1;
        for (i, (x, y)) in after.iter().zip(&source).enumerate() {
            assert_eq!(x.point, y.point, "{case}");
            if !indices.contains(&i) {
                assert_eq!(x, y, "untouched vertices stay as they are: {case}");
                continue;
            }
            match point_type.as_str() {
                "corner" => assert_eq!(x, y, "{case}"),
                "collapse" => assert_eq!((x.handle_in, x.handle_out), ([0.0; 2], [0.0; 2])),
                kind => {
                    let (li, lo) = (len(x.handle_in), len(x.handle_out));
                    assert!(
                        x.handle_in[0] * x.handle_out[1] - x.handle_in[1] * x.handle_out[0]
                            < 1e-6 * (li * lo).max(1.0),
                        "collinear: {case}"
                    );
                    if kind == "symmetric" {
                        assert!((li - lo).abs() < 1e-9, "equal: {case}");
                    }
                    if kind == "smooth" && len(y.handle_in) > 0.0 && len(y.handle_out) > 0.0 {
                        assert!(
                            (li - len(y.handle_in)).abs() < 1e-9
                                && (lo - len(y.handle_out)).abs() < 1e-9,
                            "smooth keeps lengths: {case}"
                        );
                    }
                }
            }
        }
        assert_eq!(hints.is_some(), !pressure, "{case}");
    }
    assert!(seen >= 40, "the frozen corpus has {seen} point-type cases");
}
