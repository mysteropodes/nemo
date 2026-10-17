//! Negative and mutation controls for the rig weights and anchors kernel (L03-D). Each `Flaw` is one
//! plausible mistake (most are JavaScript mutants the generator proved the oracle catches). The
//! flawed copies run through the SAME row harness as production; with `Flaw::None` the copies must
//! equal production, so a rejection is caused by the flaw alone. The three named flaws of the leaf
//! acceptance (swapped radius precedence, softness applied before the clamp, missing end clamp)
//! also fail dedicated tests against the hand-pinned fixture.

use std::collections::BTreeSet;
use std::f64::consts::PI;

use super::flawed::{f_chord, f_norm, f_weigh, Flaw, Flawed, FLAWS};
use super::oracle_rows::{failures, Ops, Production};
use super::{
    chord_angle, normalize_angle, Anchor, AnchorError, ChordError, ControlWeight, Point2,
    WeightBone, WeightEntry,
};

/// The row kinds (`nrm`, `chd`, ...) among a failure list.
fn kinds(bad: &[String]) -> BTreeSet<String> {
    bad.iter()
        .map(|m| {
            m.split(' ')
                .nth(1)
                .unwrap_or("")
                .trim_end_matches(':')
                .to_string()
        })
        .collect()
}

#[test]
fn the_flaw_free_copies_equal_production_on_every_row() {
    assert!(failures(&Flawed(Flaw::None)).is_empty());
    assert!(failures(&Production).is_empty());
}

#[test]
fn every_flaw_is_rejected_by_the_row_kind_it_breaks() {
    for (flaw, kind) in FLAWS {
        let bad = failures(&Flawed(flaw));
        assert!(!bad.is_empty(), "{flaw:?} survived every oracle row");
        assert!(
            kinds(&bad).contains(kind),
            "{flaw:?} must fail a {kind} row, failed {:?}",
            kinds(&bad)
        );
    }
}

#[test]
fn constant_functions_cannot_pass_the_rows() {
    struct Constant;
    impl Ops for Constant {
        fn norm(&self, _: f64) -> f64 {
            0.0
        }
        fn chord(&self, _: &[Option<Point2>], _: usize) -> Result<f64, ChordError> {
            Ok(0.0)
        }
        fn weigh(
            &self,
            _: &[WeightBone],
            _: &[String],
            _: Point2,
            _: f64,
            _: f64,
        ) -> Vec<ControlWeight> {
            Vec::new()
        }
        fn bind(
            &self,
            _: &[WeightBone],
            _: &[String],
            pts: &[Point2],
            _: Option<f64>,
            _: Option<f64>,
        ) -> Vec<Vec<ControlWeight>> {
            pts.iter().map(|_| Vec::new()).collect()
        }
        fn params(&self, _: Option<f64>, _: Option<f64>) -> (f64, f64) {
            (200.0, 0.0)
        }
        fn anchor(&self, _: &[WeightBone], _: &WeightEntry) -> Result<Anchor, AnchorError> {
            Ok(Anchor::Unresolved)
        }
    }
    let seen = kinds(&failures(&Constant));
    for kind in ["nrm", "chd", "wgt", "bnd", "bpr", "anc"] {
        assert!(
            seen.contains(kind),
            "a constant answer passed every {kind} row"
        );
    }
}

fn bone(id: &str, radius: Option<f64>, pts: &[Point2]) -> WeightBone {
    let segs: Vec<Option<Point2>> = pts.iter().map(|p| Some(*p)).collect();
    WeightBone {
        id: id.to_string(),
        radius,
        segments: Some(segs.clone()),
        rest_segments: Some(segs),
    }
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn swapped_radius_precedence_fails_the_bone_reach_pin() {
    let b2 = bone(
        "b2",
        Some(150.0),
        &[[0.0, 100.0], [100.0, 100.0], [200.0, 100.0]],
    );
    // 151 units from b2 point 1: outside its own reach 150, inside the bind radius 200.
    let p = [100.0, 251.0];
    assert!(super::weigh_one_point_by_control(
        std::slice::from_ref(&b2),
        &ids(&["b2"]),
        p,
        200.0,
        0.0
    )
    .is_empty());
    let swapped = f_weigh(
        Flaw::RadiusSwapped,
        std::slice::from_ref(&b2),
        &ids(&["b2"]),
        p,
        200.0,
        0.0,
    );
    // The bind reach 200 also lets in points 0 and 2 (181 units away); point 1 is the one at 151.
    assert_eq!(swapped.len(), 3);
    assert!(swapped.iter().any(|c| c.pt_index == 1));
}

#[test]
fn softness_before_the_clamp_fails_the_outside_the_radius_pin() {
    let b1 = bone("b1", None, &[[0.0, 0.0], [100.0, 0.0], [200.0, 0.0]]);
    // 220 units from b1 point 0 (reach 200): the clamped taper is 0, an unclamped smoothstep of -0.1 is +0.032.
    let p = [0.0, -220.0];
    assert!(super::weigh_one_point_by_control(
        std::slice::from_ref(&b1),
        &ids(&["b1"]),
        p,
        200.0,
        1.0
    )
    .is_empty());
    let early = f_weigh(
        Flaw::SoftBeforeClamp,
        std::slice::from_ref(&b1),
        &ids(&["b1"]),
        p,
        200.0,
        1.0,
    );
    // Every point of b1 is past its reach, and the unclamped smoothstep of a negative taper is positive.
    assert_eq!(early.len(), 3);
    assert!(early[0].w > 0.03 && early[0].w < 0.04);
    let none = f_weigh(
        Flaw::ClampDropped,
        std::slice::from_ref(&b1),
        &ids(&["b1"]),
        p,
        200.0,
        1.0,
    );
    assert_eq!(none.len(), 3);
}

#[test]
fn a_missing_end_clamp_fails_the_chord_pins() {
    let l3: Vec<Option<Point2>> = vec![Some([0.0, 0.0]), Some([10.0, 0.0]), Some([10.0, 10.0])];
    assert_eq!(chord_angle(&l3, 0), Ok(0.0));
    assert_eq!(chord_angle(&l3, 2), Ok(PI / 2.0));
    // Without the lower clamp the first point reads index -1; without the upper clamp the last reads index 3.
    assert_eq!(
        f_chord(Flaw::ChordNoStartClamp, &l3, 0),
        Err(ChordError { index: 0 })
    );
    assert_eq!(
        f_chord(Flaw::ChordNoEndClamp, &l3, 2),
        Err(ChordError { index: 3 })
    );
    assert_eq!(f_chord(Flaw::None, &l3, 2), Ok(PI / 2.0));
}

#[test]
fn normalize_flaws_break_the_wrap_pins() {
    // pi wraps to -pi, exactly; a single remainder leaves -7 alone; no shift turns -pi into 0.
    assert_eq!(f_norm(Flaw::None, PI), -PI);
    assert_eq!(f_norm(Flaw::NormPiStaysPi, PI), PI);
    assert_ne!(f_norm(Flaw::NormSingleMod, -7.0), normalize_angle(-7.0));
    assert_ne!(f_norm(Flaw::NormNoShift, -PI), normalize_angle(-PI));
}

#[test]
fn the_oracle_is_not_vacuous() {
    let rows = super::oracle_rows::rows();
    for kind in ["nrm ", "chd ", "wgt ", "bnd ", "bpr ", "anc "] {
        assert!(
            rows.iter().filter(|r| r.starts_with(kind)).count() > 50,
            "{kind} rows"
        );
    }
    let kind_rows =
        |kind: &str| -> Vec<&&str> { rows.iter().filter(|r| r.starts_with(kind)).collect() };
    let wgt = kind_rows("wgt ");
    assert!(wgt.iter().any(|r| r.ends_with(" _")) && wgt.iter().any(|r| !r.ends_with(" _")));
    let chd = kind_rows("chd ");
    assert!(
        chd.iter().any(|r| r.ends_with(" throw")) && chd.iter().any(|r| !r.ends_with(" throw"))
    );
    let anc = kind_rows("anc ");
    assert!(anc.iter().any(|r| r.ends_with(" null")) && anc.iter().any(|r| r.ends_with(" off")));
    assert!(rows
        .iter()
        .any(|r| r.starts_with("anc ") && r.ends_with(" throw")));
    assert!(rows
        .iter()
        .any(|r| r.starts_with("anc ") && r.contains(" ctl:")));
}
