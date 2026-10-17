//! Tests of the rig weights and anchors kernel (L03-D, I12): the executed-baseline oracle rows, the
//! hand-pinned literals of the independent bone/bind/weight fixture (`tools/literals.cjs`, same
//! verbatim baseline functions) and the typed contract (weight shapes, errors, falsy tables).

use super::oracle_rows::{counts, failures, parse_rig, rows, Production};
use super::{
    bind_radius, clamp_softness, effective_radius, influence, weigh_bind_points,
    weigh_one_point_by_control, ControlWeight, Point2, WeightBone,
};

pub(crate) fn bone(id: &str, rest: &[Point2]) -> WeightBone {
    let segs: Vec<Option<Point2>> = rest.iter().map(|p| Some(*p)).collect();
    WeightBone {
        id: id.to_string(),
        radius: None,
        segments: Some(segs.clone()),
        rest_segments: Some(segs),
    }
}

/// The independent fixture: 2 bones x 3 control points; b2 carries its own reach of 150.
pub(crate) fn hand() -> Vec<WeightBone> {
    let mut b1 = bone("b1", &[[0.0, 0.0], [100.0, 0.0], [200.0, 0.0]]);
    b1.segments = Some(vec![
        Some([0.0, 0.0]),
        Some([100.0, 50.0]),
        Some([200.0, 0.0]),
    ]);
    let mut b2 = bone("b2", &[[0.0, 100.0], [100.0, 100.0], [200.0, 100.0]]);
    b2.radius = Some(150.0);
    b2.segments = Some(vec![
        Some([0.0, 100.0]),
        Some([100.0, 100.0]),
        Some([200.0, 130.0]),
    ]);
    vec![b1, b2]
}

pub(crate) fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn flat(list: &[ControlWeight]) -> Vec<(String, usize, f64)> {
    list.iter()
        .map(|c| (c.bone_id.clone(), c.pt_index, c.w))
        .collect()
}

fn want(list: &[(&str, usize, f64)]) -> Vec<(String, usize, f64)> {
    list.iter()
        .map(|(b, k, w)| (b.to_string(), *k, *w))
        .collect()
}

fn weigh(
    bones: &[WeightBone],
    list: &[&str],
    p: Point2,
    radius: f64,
    soft: f64,
) -> Vec<(String, usize, f64)> {
    flat(&weigh_one_point_by_control(
        bones,
        &ids(list),
        p,
        radius,
        soft,
    ))
}

#[test]
fn oracle_row_counts_are_pinned() {
    assert_eq!(rows().len(), 1029);
    assert_eq!(counts(), [97, 173, 422, 70, 117, 150]);
}

#[test]
fn every_oracle_row_matches_production() {
    let bad = failures(&Production);
    assert!(
        bad.is_empty(),
        "{} rows differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(3)]
    );
}

#[test]
fn baseline_facts_the_rows_pin() {
    let anc: Vec<&str> = rows()
        .into_iter()
        .filter(|r| r.starts_with("anc "))
        .collect();
    let tail = |t: &str| anc.iter().filter(|r| r.ends_with(t)).count();
    assert_eq!(tail(" null"), 41);
    assert_eq!(tail(" off"), 6);
    assert_eq!(tail(" throw"), 8);
    assert_eq!(anc.iter().filter(|r| r.contains(" ctl:")).count(), 95);
    let wgt: Vec<&str> = rows()
        .into_iter()
        .filter(|r| r.starts_with("wgt "))
        .collect();
    let empty = wgt.iter().filter(|r| r.ends_with(" _")).count();
    assert_eq!((empty, wgt.len() - empty), (223, 199));
    let chd: Vec<&str> = rows()
        .into_iter()
        .filter(|r| r.starts_with("chd "))
        .collect();
    assert_eq!(chd.iter().filter(|r| r.ends_with(" throw")).count(), 70);
}

#[test]
fn hand_fixture_weights_at_softness_zero_and_half() {
    let b = hand();
    assert_eq!(
        weigh(&b, &["b1", "b2"], [50.0, 50.0], 200.0, 0.0),
        want(&[
            ("b1", 0, 0.6464466094067263),
            ("b1", 1, 0.6464466094067263),
            ("b1", 2, 0.20943058495790512),
            ("b2", 0, 0.5285954792089683),
            ("b2", 1, 0.5285954792089683),
        ])
    );
    assert_eq!(
        weigh(&b, &["b1", "b2"], [50.0, 50.0], 200.0, 0.5),
        want(&[
            ("b1", 0, 0.6799174785275224),
            ("b1", 1, 0.6799174785275224),
            ("b1", 2, 0.16132117688026182),
            ("b2", 0, 0.5357209664469356),
            ("b2", 1, 0.5357209664469356),
        ])
    );
    assert_eq!(
        weigh(&b, &["b1", "b2"], [50.0, 50.0], 200.0, 1.0),
        want(&[
            ("b1", 0, 0.7133883476483185),
            ("b1", 1, 0.7133883476483185),
            ("b1", 2, 0.1132117688026185),
            ("b2", 0, 0.5428464536849029),
            ("b2", 1, 0.5428464536849029),
        ])
    );
}

#[test]
fn bone_list_order_decides_entry_order() {
    let b = hand();
    let got = weigh(&b, &["b2", "b1"], [50.0, 50.0], 200.0, 0.0);
    let order: Vec<(&str, usize)> = got.iter().map(|(id, k, _)| (id.as_str(), *k)).collect();
    assert_eq!(
        order,
        vec![("b2", 0), ("b2", 1), ("b1", 0), ("b1", 1), ("b1", 2)]
    );
}

#[test]
fn a_vertex_outside_every_radius_yields_an_empty_list_at_any_softness() {
    let b = hand();
    for soft in [0.0, 0.5, 1.0] {
        assert!(weigh(&b, &["b1", "b2"], [1000.0, 1000.0], 200.0, soft).is_empty());
        // Just outside the 200 reach of b1 point 0 (dist 220): the clamp runs BEFORE the softness
        // blend, so even full softness gives no weight (an unclamped smoothstep of -0.1 is +0.032).
        assert!(weigh(&b, &["b1", "b2"], [0.0, -220.0], 200.0, soft).is_empty());
    }
    // Inside by 10 units the single weight is (1 - 190/200).
    assert_eq!(
        weigh(&b, &["b1", "b2"], [0.0, -190.0], 200.0, 0.0),
        want(&[("b1", 0, 0.050000000000000044)])
    );
}

#[test]
fn bone_radius_overrides_the_bind_radius_even_when_smaller() {
    let b = hand();
    // b2 reach 150: 149 units away is inside, 151 outside; its default would be 200.
    assert_eq!(
        weigh(&b, &["b2"], [100.0, 249.0], 200.0, 0.0),
        want(&[("b2", 1, 0.00666666666666671)])
    );
    assert!(weigh(&b, &["b2"], [100.0, 251.0], 200.0, 0.0).is_empty());
    // b1 has no radius of its own and keeps the 200 bind reach.
    assert_eq!(
        weigh(&b, &["b1"], [100.0, 199.0], 200.0, 0.0),
        want(&[("b1", 1, 0.0050000000000000044)])
    );
}

#[test]
fn segments_without_a_point_are_skipped_but_keep_their_index() {
    let mut b = bone("b1", &[]);
    b.rest_segments = Some(vec![
        Some([0.0, 0.0]),
        None,
        Some([100.0, 0.0]),
        None,
        Some([200.0, 0.0]),
    ]);
    assert_eq!(
        weigh(&[b], &["b1"], [60.0, 30.0], 200.0, 0.0),
        want(&[
            ("b1", 0, 0.6645898033750315),
            ("b1", 2, 0.75),
            ("b1", 4, 0.2841089468361824)
        ])
    );
}

#[test]
fn missing_bones_and_absent_rest_segments_contribute_nothing_and_duplicates_repeat() {
    let mut empty = bone("e", &[]);
    empty.rest_segments = None;
    let single = bone("b1", &[[0.0, 0.0]]);
    assert!(weigh(&[empty], &["e", "zz"], [0.0, 0.0], 200.0, 0.0).is_empty());
    assert_eq!(
        weigh(&[single], &["b1", "b1", "zz"], [0.0, 0.0], 200.0, 0.0),
        want(&[("b1", 0, 1.0), ("b1", 0, 1.0)])
    );
}

#[test]
fn rest_segments_are_weighed_not_the_live_pose() {
    let mut b = bone("b1", &[[0.0, 0.0], [10.0, 0.0]]);
    b.segments = Some(vec![Some([400.0, 400.0]), Some([500.0, 500.0])]);
    let got = weigh(&[b], &["b1"], [5.0, 5.0], 200.0, 0.0);
    assert_eq!(got.len(), 2);
}

#[test]
fn bind_pass_applies_the_baseline_radius_and_softness_preprocessing() {
    assert_eq!(bind_radius(None), 200.0);
    assert_eq!(bind_radius(Some(0.0)), 200.0);
    assert_eq!(bind_radius(Some(-0.0)), 200.0);
    assert_eq!(bind_radius(Some(f64::NAN)), 200.0);
    assert_eq!(bind_radius(Some(120.0)), 120.0);
    assert_eq!(bind_radius(Some(-5.0)), -5.0);
    assert_eq!(clamp_softness(None), 0.0);
    assert_eq!(clamp_softness(Some(7.0)), 1.0);
    assert_eq!(clamp_softness(Some(-1.0)), 0.0);
    assert_eq!(clamp_softness(Some(f64::NAN)), 0.0);
    assert_eq!(clamp_softness(Some(0.3)), 0.3);
    assert_eq!(clamp_softness(Some(f64::INFINITY)), 1.0);
    assert_eq!(clamp_softness(Some(f64::NEG_INFINITY)), 0.0);
    assert!(clamp_softness(Some(-0.0)).is_sign_positive());
    let b = hand();
    let pts = [[50.0, 50.0], [1000.0, 1000.0]];
    let lists = weigh_bind_points(&b, &ids(&["b1", "b2"]), &pts, None, Some(7.0));
    assert_eq!(
        flat(&lists[0]),
        weigh(&b, &["b1", "b2"], [50.0, 50.0], 200.0, 1.0)
    );
    assert!(lists[1].is_empty());
}

#[test]
fn falsy_radius_and_softness_tables() {
    let mut b = bone("b", &[[0.0, 0.0]]);
    for falsy in [None, Some(0.0), Some(-0.0), Some(f64::NAN)] {
        b.radius = falsy;
        assert_eq!(effective_radius(&b, 200.0), 200.0);
    }
    b.radius = Some(-50.0);
    assert_eq!(effective_radius(&b, 200.0), -50.0);
    b.radius = Some(60.0);
    assert_eq!(effective_radius(&b, 200.0), 60.0);
    // NaN and zero softness are falsy: the plain linear taper.
    assert_eq!(influence(50.0, 100.0, f64::NAN), 0.5);
    assert_eq!(influence(50.0, 100.0, 0.0), 0.5);
    assert_eq!(influence(50.0, 100.0, 1.0), 0.5);
    assert_eq!(influence(25.0, 100.0, 1.0), 0.84375);
    assert_eq!(influence(100.0, 100.0, 1.0), 0.0);
    assert!(influence(0.0, 0.0, 0.0).is_nan());
}

#[test]
fn rig_token_parser_round_trips_the_oracle_rig_of_a_known_row() {
    let row = rows().into_iter().find(|r| r.starts_with("wgt ")).unwrap();
    let rig = parse_rig(row.split(' ').nth(1).unwrap());
    assert_eq!(rig.len(), 2);
    assert_eq!(rig[0].id, "b1");
    assert_eq!(rig[1].radius, Some(150.0));
    assert_eq!(rig[0].radius, None);
}
