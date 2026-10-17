//! Oracle tests for the temporal offset and the copy loop (L04-D, I08).
//!
//! `fixtures/duplicator-placement-temporal/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-D leaf: every scenario is one call of the
//! verbatim baseline `applyLayerDuplicator` (baseline `fbe4b8a3`) with a
//! resample port, a span and a `timeOffset`, recording the frames it asked
//! for, the placement descriptors it received (and the pivot of each) and the
//! strokes it returned. `test_codec::run` replays the inputs through the real
//! kernel and `test_codec::mismatch` compares everything it produced.

use super::test_codec::{mismatch, run, scenarios, Scenario};
use super::{
    build_copies, shift_frames, shifted_index, temporal_on, BuildError, CopyRequest, Direction,
    SpanBounds, TemporalOffset, BASELINE_RULES, TEMPORAL_STRIDE,
};
use crate::evaluation::object_component_kernels::duplicator_placement_apply::PlacementApply;
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    parse_record, same,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{Descriptor, Pivot};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::duplicator_placement_rng::{
    stream_seed, SeededRng,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-temporal/oracle.txt");

pub(crate) fn scenario(name: &str) -> Scenario {
    scenarios(ORACLE)
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("scenario {name}"))
}

fn flat(shift_dx: f64) -> PlacementApply {
    PlacementApply {
        matrix: Descriptor {
            rot: 0.0,
            sx: 1.0,
            sy: 1.0,
            dx: shift_dx,
            dy: 0.0,
        },
        opacity_factor: 1.0,
        hue_deg: 0.0,
        dup_3d: None,
    }
}

const BOX: &str = "box/#102030/0/4024000000000000:4034000000000000:0000000000000000:0000000000000000:0000000000000000:0000000000000000";

#[test]
fn every_temporal_scenario_matches_the_baseline() {
    let all = scenarios(ORACLE);
    assert_eq!(all.len(), 38);
    for sc in &all {
        let ran = run(sc, BASELINE_RULES);
        assert_eq!(mismatch(sc, &ran), None, "{}", sc.name);
    }
}

/// The frames the baseline asked for in a scenario, as plain numbers.
fn asked(name: &str) -> Vec<f64> {
    scenario(name).calls
}

#[test]
fn shifted_frames_wrap_into_the_span_as_documented() {
    // each pin is derived by hand from the formula and equals what the verbatim baseline asked for
    let pins: [(&str, &[f64]); 11] = [
        ("forward_in_span", &[10.0, 8.0, 6.0, 4.0]),
        ("backward_in_span", &[4.0, 6.0, 8.0, 10.0]),
        ("center_out", &[2.0, 5.0, 8.0, 8.0, 5.0, 2.0]),
        ("center_out_half_steps", &[7.0, 8.0, 10.0, 8.0, 7.0]),
        (
            "center_out_negative_offset",
            &[14.0, 12.0, 11.0, 11.0, 12.0, 14.0],
        ),
        ("wraps_below_the_span_start", &[1.0, 6.0, 3.0, 0.0]),
        ("wraps_above_the_span_end", &[7.0, 2.0, 5.0, 0.0]),
        ("span_with_nonzero_start", &[30.0, 28.0, 26.0, 24.0]),
        ("frame_before_the_span", &[27.0, 25.0, 35.0, 33.0]),
        ("span_length_zero_is_one", &[5.0, 5.0, 5.0, 5.0]),
        ("span_length_fractional", &[2.5, 0.5, 6.0, 4.0]),
    ];
    for (name, frames) in pins {
        let want = asked(name);
        assert_eq!(want.len(), frames.len(), "{name}");
        assert!(
            want.iter().zip(frames).all(|(a, b)| same(*a, *b)),
            "{name}: {want:?}"
        );
        // the kernel computes the same frames from the scenario's own inputs
        let sc = scenario(name);
        let (_, off, dir) = sc.temporal.clone().expect("a temporal scenario");
        let t = TemporalOffset {
            enabled: true,
            offset_frames: off,
            direction: dir.as_deref(),
        };
        let got: Vec<f64> = (0..frames.len() as u32)
            .map(|k| shifted_index(sc.frame, &sc.span, shift_frames(&t, k, sc.count, sc.seed)))
            .collect();
        assert!(
            got.iter().zip(frames).all(|(a, b)| same(*a, *b)),
            "{name}: {got:?}"
        );
    }
}

#[test]
fn a_span_shorter_than_one_frame_counts_as_one() {
    let span = SpanBounds {
        start: 5.0,
        len: -9.0,
    };
    assert_eq!(shifted_index(100.0, &span, 3.0), 5.0);
    let none = SpanBounds {
        start: 0.0,
        len: 0.0,
    };
    assert_eq!(shifted_index(-17.0, &none, 0.0), 0.0);
}

#[test]
fn directions_choose_the_shift_per_clone() {
    let t = |direction: Option<&'static str>, off: f64| TemporalOffset {
        enabled: true,
        offset_frames: off,
        direction,
    };
    let at = |t: &TemporalOffset<'_>, k: u32, count: f64| {
        shift_frames(t, k, count, JsNumber::Value(1.0))
    };
    assert_eq!(at(&t(None, 2.0), 3, 8.0), 6.0);
    assert_eq!(at(&t(Some("forward"), 2.0), 3, 8.0), 6.0);
    assert_eq!(at(&t(Some("anything else"), 2.0), 3, 8.0), 6.0);
    assert_eq!(at(&t(Some("backward"), 2.0), 3, 8.0), 8.0);
    assert_eq!(at(&t(Some("backward"), 2.0), 7, 8.0), 0.0);
    assert_eq!(at(&t(Some("centerOut"), 2.0), 3, 8.0), 1.0);
    assert_eq!(at(&t(Some("centerOut"), 2.0), 0, 8.0), 7.0);
    for label in [
        None,
        Some(""),
        Some("Forward"),
        Some("random "),
        Some("CENTEROUT"),
    ] {
        assert_eq!(
            Direction::from_label(label),
            Direction::Forward,
            "{label:?}"
        );
    }
    assert_eq!(Direction::from_label(Some("backward")), Direction::Backward);
    assert_eq!(
        Direction::from_label(Some("centerOut")),
        Direction::CenterOut
    );
    assert_eq!(Direction::from_label(Some("random")), Direction::Random);
}

#[test]
fn the_random_direction_uses_its_own_prime_stride() {
    assert_eq!(TEMPORAL_STRIDE, 104_729.0);
    let sc = scenario("random_seed_42");
    let (_, off, _) = sc.temporal.clone().expect("temporal");
    let t = TemporalOffset {
        enabled: true,
        offset_frames: off,
        direction: Some("random"),
    };
    // floor(first draw of the stream seeded (42 + k * 104729) >>> 0, times the count) * offset
    let mut differs = 0;
    for k in 0..6u32 {
        let mut rng = SeededRng::new(stream_seed(JsNumber::Value(42.0), k, TEMPORAL_STRIDE));
        let want = (rng.next_f64() * 6.0).floor() * off;
        assert_eq!(shift_frames(&t, k, 6.0, JsNumber::Value(42.0)), want);
        // a placement-stream seed (stride 7919) would draw a different sequence
        let mut other = SeededRng::new(stream_seed(JsNumber::Value(42.0), k, 7919.0));
        if (other.next_f64() * 6.0).floor() * off != want {
            differs += 1;
        }
    }
    assert!(differs > 0, "the two strides must give different shifts");
}

#[test]
fn the_offset_switch_follows_to_int32_of_the_offset() {
    let t = |enabled: bool, offset_frames: f64| TemporalOffset {
        enabled,
        offset_frames,
        direction: None,
    };
    assert!(temporal_on(Some(&t(true, 1.0)), true));
    assert!(temporal_on(Some(&t(true, 1.5)), true));
    assert!(temporal_on(Some(&t(true, -1.0)), true));
    assert!(temporal_on(Some(&t(true, 2_147_483_648.0)), true));
    for off in [
        0.0,
        -0.0,
        0.5,
        -0.75,
        0.999_999,
        f64::NAN,
        f64::INFINITY,
        4_294_967_296.0,
    ] {
        assert!(!temporal_on(Some(&t(true, off)), true), "{off}");
    }
    assert!(!temporal_on(Some(&t(false, 2.0)), true));
    assert!(!temporal_on(Some(&t(true, 2.0)), false));
    assert!(!temporal_on(None, true));
}

#[test]
fn count_one_and_below_places_nothing_and_touches_no_port() {
    let base = vec![parse_record(BOX)];
    for count in [1.0, 0.5, 0.0, -3.0, f64::NAN] {
        let req = request(count, true);
        let result = build_copies(
            &base,
            &req,
            &mut |_, _| panic!("no clone is placed at count {count}"),
            &mut |_| panic!("nothing is resampled at count {count}"),
        );
        // NaN is not <= 1, so it reaches the loop bound and places no clone either
        if count.is_nan() {
            assert_eq!(result.expect("no error").map(|c| c.len()), Some(0));
        } else {
            assert_eq!(result, Ok(None), "count {count}");
        }
    }
}

fn request(count: f64, resample: bool) -> CopyRequest<'static> {
    CopyRequest {
        seed: JsNumber::Value(1.0),
        count,
        frame_idx: 4.0,
        canvas_w: 100.0,
        canvas_h: 100.0,
        temporal: Some(TemporalOffset {
            enabled: true,
            offset_frames: 1.0,
            direction: None,
        }),
        span: SpanBounds {
            start: 0.0,
            len: 10.0,
        },
        can_resample: resample,
    }
}

#[test]
fn a_count_above_the_grid_cap_is_a_typed_error() {
    let base = vec![parse_record(BOX)];
    let req = request(901.0, false);
    let result = build_copies(&base, &req, &mut |_, _| flat(0.0), &mut |_| Vec::new());
    assert_eq!(
        result.map(|r| r.map(|c| c.len())),
        Err(BuildError::CountAboveCap)
    );
    // the cap itself is fine
    let at_cap = request(900.0, false);
    let result = build_copies(&base, &at_cap, &mut |_, _| flat(0.0), &mut |_| Vec::new());
    assert_eq!(result.expect("900 clones").map(|c| c.len()), Some(900));
}

#[test]
fn the_resample_port_is_used_once_per_clone_and_only_when_the_offset_is_on() {
    let base = vec![parse_record(BOX)];
    let mut calls = 0;
    let on = request(5.0, true);
    build_copies(&base, &on, &mut |_, _| flat(0.0), &mut |_| {
        calls += 1;
        Vec::new()
    })
    .expect("copies");
    assert_eq!(calls, 5);
    let mut calls = 0;
    let off = request(5.0, false);
    build_copies(&base, &off, &mut |_, _| flat(0.0), &mut |_| {
        calls += 1;
        Vec::new()
    })
    .expect("copies");
    assert_eq!(calls, 0);
}

#[test]
fn the_placement_is_asked_for_every_clone_even_when_nothing_is_drawn() {
    let mut asked_for = Vec::new();
    let empty: Vec<crate::object_document::ObjectRecord> = Vec::new();
    let req = request(3.0, false);
    let result = build_copies(
        &empty,
        &req,
        &mut |k, pivot: Pivot| {
            asked_for.push((k, pivot.x, pivot.y));
            flat(0.0)
        },
        &mut |_| Vec::new(),
    )
    .expect("no error");
    assert_eq!(result.map(|c| c.len()), Some(0));
    // no anchor: every clone is placed around the canvas center
    assert_eq!(
        asked_for,
        vec![(0, 50.0, 50.0), (1, 50.0, 50.0), (2, 50.0, 50.0)]
    );
}

#[test]
fn the_base_and_the_resampled_strokes_are_never_mutated() {
    let base = vec![parse_record(BOX)];
    let shifted = vec![parse_record("moved/#abcdef/1/4044000000000000:4044000000000000:3ff0000000000000:3ff0000000000000:0000000000000000:0000000000000000")];
    let before = (
        serde_json::to_vec(&base).expect("serialize"),
        serde_json::to_vec(&shifted).expect("serialize"),
    );
    let req = request(4.0, true);
    let list = shifted.clone();
    let copies = build_copies(
        &base,
        &req,
        &mut |k, _| flat(f64::from(k) * 10.0),
        &mut |_| list.clone(),
    )
    .expect("copies")
    .expect("count > 1");
    assert_eq!(copies.len(), 4);
    assert_eq!(serde_json::to_vec(&base).expect("serialize"), before.0);
    assert_eq!(serde_json::to_vec(&shifted).expect("serialize"), before.1);
    for (k, copy) in copies.iter().enumerate() {
        assert_eq!(copy.dup_index as usize, k);
        assert_eq!(copy.record.target.stroke_id(), "moved");
    }
}

#[test]
fn a_hold_blank_frame_falls_back_to_the_base_content() {
    let sc = scenario("hold_blank_falls_back");
    let ran = run(&sc, BASELINE_RULES);
    let copies = ran.result.expect("no error").expect("copies");
    // odd frames are blank in this scenario: those clones show the base stroke "a1"
    let ids: Vec<&str> = copies.iter().map(|c| c.record.target.stroke_id()).collect();
    assert!(ids.contains(&"a1"), "{ids:?}");
    assert!(ids.iter().any(|id| id.starts_with('w')), "{ids:?}");
}
