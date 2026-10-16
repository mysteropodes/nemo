//! Literal pins, negative controls and mutation controls for the scene stroke kernel.
//!
//! Every mutation control rebuilds `StrokeRules` with exactly one baseline
//! constant changed and requires either an oracle section or a pinned literal to
//! fail, so a kernel that silently drifts from engine.rs / engine-bridge.js
//! cannot keep passing. Literal vectors below were derived by hand from the
//! engine.rs `dash_pattern_with_offset` text (baseline fbe4b8a3) and cross
//! checked against the oracle rows quoted in each test.

use super::tests::{first_divergence, p_mismatches, s_mismatches, ORACLE};
use super::{
    scene_stroke, EngineStrokeItem, SceneCap, SceneJoin, StrokeError, StrokeRules, StrokeSource,
    BASELINE,
};

fn mutant(change: impl FnOnce(&mut StrokeRules)) -> StrokeRules {
    let mut rules = BASELINE;
    change(&mut rules);
    rules
}

fn engine_item() -> EngineStrokeItem {
    EngineStrokeItem {
        width: Some(2.0),
        cap: None,
        join: None,
        miter_limit: None,
        dash_pattern: None,
        dash_offset: None,
    }
}

fn source() -> StrokeSource {
    StrokeSource {
        color: Some([255, 0, 0, 255]),
        width: Some(2.0),
        path_transform: false,
        stroke_scale: 1.0,
        cap: None,
        join: None,
        miter_limit: None,
        dash_array: None,
        dash_offset: None,
    }
}

#[test]
fn baseline_rules_are_the_pinned_constants() {
    assert_eq!(
        BASELINE,
        StrokeRules {
            absent_width: 1.0,
            js_fallback_width: 1.0,
            zero_is_falsy: true,
            path_transform_skips_scale: true,
            fallback_cap: SceneCap::Round,
            fallback_join: SceneJoin::Round,
            empty_dash_skips: true,
            euclid_wrap: true,
            gap_zero_dash: true,
        }
    );
    assert_eq!(BASELINE.cap_from("butt"), SceneCap::Butt);
    assert_eq!(BASELINE.cap_from("square"), SceneCap::Square);
    assert_eq!(BASELINE.join_from("bevel"), SceneJoin::Bevel);
    assert_eq!(BASELINE.join_from("miter"), SceneJoin::Miter);
    for name in ["round", "Butt", "SQUARE", "", " butt", "miter-clip"] {
        assert_eq!(BASELINE.cap_from(name), SceneCap::Round, "cap {name:?}");
        assert_eq!(BASELINE.join_from(name), SceneJoin::Round, "join {name:?}");
    }
}

#[test]
fn dash_bake_literals_follow_engine_rs() {
    let bake = |pattern: &[f64], offset: f64| BASELINE.bake_dash_offset(pattern, offset).unwrap();
    // Oracle: P e 5,3 @2 -> 3,3 (first drift probe 36); @-1 -> 0,1,5 (probe 24).
    assert_eq!(bake(&[5.0, 3.0], 2.0), vec![3.0, 3.0]);
    assert_eq!(bake(&[5.0, 3.0], -1.0), vec![0.0, 1.0, 5.0]);
    // Offset 6 lands one unit into the gap (5 + 1): zero-length dash, 2 of gap left, then the dash.
    assert_eq!(bake(&[5.0, 3.0], 6.0), vec![0.0, 2.0, 5.0]);
    // A whole period is the identity rotation.
    assert_eq!(bake(&[5.0, 3.0], 8.0), vec![5.0, 3.0]);
    assert_eq!(bake(&[5.0, 3.0], 16.0), vec![5.0, 3.0]);
    // Odd arrays rotate over their own length; the gap parity follows the index.
    assert_eq!(bake(&[3.0], 1.0), vec![2.0]);
    assert_eq!(bake(&[2.0, 2.0, 2.0], 3.0), vec![0.0, 1.0, 2.0, 2.0]);
    // total <= 0 or empty returns the pattern unchanged, whatever the offset.
    assert_eq!(bake(&[0.0, 0.0], 3.0), vec![0.0, 0.0]);
    assert_eq!(bake(&[], 3.0), Vec::<f64>::new());
}

#[test]
fn the_documented_baseline_drift_after_the_first_baked_cycle_is_pinned() {
    // [5,3] @2 bakes to [3,3]: period 6 against the original period 8, so the
    // baked array stops matching at x = 9 (probe 36 is x = 9.125).
    let baked = BASELINE.bake_dash_offset(&[5.0, 3.0], 2.0).unwrap();
    assert_eq!(first_divergence(&[5.0, 3.0], 2.0, &baked), Some(36));
    // A pure rotation has no drift: offset 8 is the identity.
    let identity = BASELINE.bake_dash_offset(&[5.0, 3.0], 8.0).unwrap();
    assert_eq!(first_divergence(&[5.0, 3.0], 8.0, &identity), None);
}

#[test]
fn engine_widths_scene_widths_and_stroke_presence_follow_the_bridge() {
    assert_eq!(BASELINE.engine_width(None), 1.0);
    assert_eq!(
        BASELINE.engine_width(Some(0.0)),
        0.0,
        "an explicit 0 is not defaulted by the engine"
    );
    assert_eq!(BASELINE.engine_width(Some(-2.0)), -2.0);
    assert_eq!(BASELINE.scene_width(None, false, 2.0), 2.0);
    assert_eq!(
        BASELINE.scene_width(Some(0.0), false, 2.0),
        2.0,
        "|| 1 replaces 0"
    );
    assert_eq!(
        BASELINE.scene_width(Some(-0.0), false, 2.0),
        2.0,
        "|| 1 replaces -0"
    );
    assert_eq!(
        BASELINE.scene_width(Some(f64::NAN), false, 2.0),
        2.0,
        "|| 1 replaces NaN"
    );
    assert_eq!(BASELINE.scene_width(Some(3.0), false, 2.0), 6.0);
    assert_eq!(
        BASELINE.scene_width(Some(3.0), true, 2.0),
        3.0,
        "pathTransform skips the scale"
    );
    assert_eq!(BASELINE.scene_width(None, true, 2.0), 1.0);
    let mut item = source();
    item.color = None;
    item.dash_array = Some(vec![f64::NAN, 1.0]);
    assert_eq!(
        scene_stroke(&item),
        Ok(None),
        "no colour, no stroke, nothing validated"
    );
    let mut scaled = source();
    scaled.stroke_scale = f64::NAN;
    assert_eq!(scene_stroke(&scaled), Err(StrokeError::NonFiniteWidth));
}

#[test]
fn engine_stroke_defaults_miter_and_offset_rules() {
    let mut item = engine_item();
    let stroke = BASELINE.engine_stroke(&item).unwrap();
    assert_eq!(
        (stroke.cap, stroke.join),
        (SceneCap::Round, SceneJoin::Round)
    );
    assert_eq!(
        (stroke.miter_limit, stroke.effective_miter_limit()),
        (None, 4.0)
    );
    item.miter_limit = Some(2.5);
    assert_eq!(
        BASELINE.engine_stroke(&item).unwrap().miter_limit,
        Some(2.5)
    );
    item.miter_limit = Some(f64::NAN);
    assert_eq!(
        BASELINE.engine_stroke(&item).unwrap().miter_limit,
        None,
        "null reads as absent"
    );
    item.miter_limit = Some(0.0);
    assert_eq!(
        BASELINE.engine_stroke(&item).unwrap().miter_limit,
        Some(0.0),
        "0 is present"
    );
    item.dash_pattern = Some(vec![5.0, 3.0]);
    for offset in [
        None,
        Some(0.0),
        Some(-0.0),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        item.dash_offset = offset;
        let dash = BASELINE.engine_stroke(&item).unwrap().dash;
        assert_eq!(dash, Some(vec![5.0, 3.0]), "offset {offset:?} is not baked");
    }
    item.dash_offset = Some(2.0);
    let baked = BASELINE.engine_stroke(&item).unwrap();
    assert_eq!(baked.dash, Some(vec![3.0, 3.0]));
    assert_eq!(baked.dash_offset(), 0.0, "the offset is always baked out");
    item.dash_pattern = Some(Vec::new());
    assert_eq!(
        BASELINE.engine_stroke(&item).unwrap().dash,
        None,
        "empty pattern: no dash"
    );
}

#[test]
fn invalid_numbers_are_typed_errors_not_hangs() {
    let mut item = engine_item();
    item.width = Some(f64::NAN);
    assert_eq!(
        BASELINE.engine_stroke(&item),
        Err(StrokeError::NonFiniteWidth)
    );
    item.width = Some(f64::INFINITY);
    assert_eq!(
        BASELINE.engine_stroke(&item),
        Err(StrokeError::NonFiniteWidth)
    );
    item.width = Some(2.0);
    for (pattern, why) in [
        (vec![f64::NAN, 1.0], StrokeError::NonFiniteDashEntry),
        (vec![1.0, f64::INFINITY], StrokeError::NonFiniteDashEntry),
        (vec![3.0, -1.0], StrokeError::NegativeDashEntry),
        (vec![1e308, 1e308], StrokeError::NonFiniteDashPeriod),
    ] {
        item.dash_pattern = Some(pattern.clone());
        assert_eq!(BASELINE.engine_stroke(&item), Err(why), "{pattern:?}");
        item.dash_offset = Some(-1.0);
        assert_eq!(BASELINE.engine_stroke(&item), Err(why), "{pattern:?} @-1");
        item.dash_offset = None;
    }
    assert_eq!(
        BASELINE.bake_dash_offset(&[1.0, 1.0], f64::NAN),
        Err(StrokeError::NonFiniteDashOffset)
    );
    assert_eq!(
        BASELINE.bake_dash_offset(&[1.0, 1.0], f64::NEG_INFINITY),
        Err(StrokeError::NonFiniteDashOffset)
    );
    for error in [
        StrokeError::NonFiniteWidth,
        StrokeError::NonFiniteDashEntry,
        StrokeError::NegativeDashEntry,
        StrokeError::NonFiniteDashPeriod,
        StrokeError::NonFiniteDashOffset,
    ] {
        assert!(error.message().contains("must be finite") || error.message().contains("negative"));
    }
}

#[test]
fn baseline_passes_every_oracle_section() {
    assert!(s_mismatches(&BASELINE, ORACLE).is_empty());
    assert!(p_mismatches(&BASELINE, ORACLE).is_empty());
}

#[test]
fn mutating_the_gap_parity_zero_dash_is_caught() {
    let rules = mutant(|r| r.gap_zero_dash = false);
    assert_eq!(
        rules.bake_dash_offset(&[5.0, 3.0], 6.0).unwrap(),
        vec![2.0, 5.0]
    );
    assert_ne!(
        rules.bake_dash_offset(&[5.0, 3.0], 6.0),
        BASELINE.bake_dash_offset(&[5.0, 3.0], 6.0)
    );
    assert!(!p_mismatches(&rules, ORACLE).is_empty());
    assert!(!s_mismatches(&rules, ORACLE).is_empty());
}

#[test]
fn mutating_the_rem_euclid_sign_is_caught() {
    let rules = mutant(|r| r.euclid_wrap = false);
    // Truncating `%` keeps the sign: -1 % 8 = -1, so the first dash grows to 6.
    assert_eq!(
        rules.bake_dash_offset(&[5.0, 3.0], -1.0).unwrap(),
        vec![6.0, 3.0]
    );
    assert!(!p_mismatches(&rules, ORACLE).is_empty());
    assert!(!s_mismatches(&rules, ORACLE).is_empty());
}

#[test]
fn mutating_the_round_fallbacks_is_caught() {
    let cap = mutant(|r| r.fallback_cap = SceneCap::Butt);
    assert_eq!(cap.cap_from("nonsense"), SceneCap::Butt);
    assert!(!s_mismatches(&cap, ORACLE).is_empty());
    let join = mutant(|r| r.fallback_join = SceneJoin::Miter);
    assert_eq!(join.join_from("nonsense"), SceneJoin::Miter);
    assert!(!s_mismatches(&join, ORACLE).is_empty());
}

#[test]
fn mutating_the_empty_dash_test_is_caught() {
    let rules = mutant(|r| r.empty_dash_skips = false);
    let mut item = engine_item();
    item.dash_pattern = Some(Vec::new());
    assert_eq!(BASELINE.engine_stroke(&item).unwrap().dash, None);
    assert_eq!(rules.engine_stroke(&item).unwrap().dash, Some(Vec::new()));
}

#[test]
fn mutating_the_width_rules_is_caught() {
    let defaulted = mutant(|r| r.absent_width = 2.0);
    assert_ne!(defaulted.engine_width(None), BASELINE.engine_width(None));
    let fallback = mutant(|r| r.js_fallback_width = 2.0);
    assert!(!s_mismatches(&fallback, ORACLE).is_empty());
    let falsy = mutant(|r| r.zero_is_falsy = false);
    assert_eq!(falsy.scene_width(Some(0.0), false, 2.0), 0.0);
    assert!(!s_mismatches(&falsy, ORACLE).is_empty());
    let scaled = mutant(|r| r.path_transform_skips_scale = false);
    assert_eq!(scaled.scene_width(Some(3.0), true, 2.0), 6.0);
    assert!(!s_mismatches(&scaled, ORACLE).is_empty());
}

#[test]
fn a_corrupted_oracle_row_is_detected_in_every_section() {
    let scene_row = " OK 3ff0000000000000 Round Round U -";
    assert!(ORACLE.contains(scene_row));
    let corrupted = ORACLE.replacen(scene_row, " OK 3ff0000000000000 Butt Round U -", 1);
    assert_eq!(s_mismatches(&BASELINE, &corrupted).len(), 1);
    let dash_row = "P e 4014000000000000,4008000000000000 4000000000000000 4008000000000000,4008000000000000 36";
    assert!(ORACLE.contains(dash_row));
    let baked = ORACLE.replacen(
        dash_row,
        &dash_row.replace(",4008000000000000 36", ",4000000000000000 36"),
        1,
    );
    assert_eq!(p_mismatches(&BASELINE, &baked).len(), 1);
    let verdict = ORACLE.replacen(dash_row, &dash_row.replace(" 36", " 35"), 1);
    assert_eq!(p_mismatches(&BASELINE, &verdict).len(), 1);
}
