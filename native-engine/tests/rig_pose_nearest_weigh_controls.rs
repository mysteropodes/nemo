//! Negative and mutation controls for the rig callers of the nearest-location kernel (L03-I). Each `Flaw` is one
//! plausible mistake; every one is also a textual mutant of the verbatim `app.js` function that `tools/gen.cjs`
//! proves the oracle catches (`CONSUMER_MUTANTS`). The flawed copies run through the same row harness as
//! production; with `Flaw::None` the copy equals production.

use std::collections::BTreeSet;

use super::tests::{bone, failures, line, Ops, Production};
use super::{BoneNearestError, NearestBone};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::{
    nearest_on_curves, NearestLocation,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::OffsetWeight;
use crate::evaluation::property_follow_path_sample::path_curves;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flaw {
    None,
    /// `radius || bone.radius`: the bind radius wins when it is truthy.
    RadiusPrecedenceSwapped,
    /// The taper `1 - dist / reach` is not clamped to zero.
    TaperNotClamped,
    /// The smoothstep is `t*t*(2 - 3t)`.
    WrongSmoothstep,
    /// An entry is emitted at a zero influence too.
    EmitAtZero,
    /// The distance is stored where the offset belongs.
    StoresDistance,
    /// The softness blend runs for every softness.
    SoftnessAlwaysBlended,
    /// The scan result is not multiplied by 1.05.
    NoMargin,
    /// `Math.round` instead of `Math.ceil`.
    RoundNotCeil,
    /// A bone with its own radius is scanned as well.
    BoneRadiusNotSkipped,
    /// The scan starts at the given default without `|| 200`.
    NoDefault200,
}

const FLAWS: [(Flaw, &str); 10] = [
    (Flaw::RadiusPrecedenceSwapped, "wgh"),
    (Flaw::TaperNotClamped, "wgh"),
    (Flaw::WrongSmoothstep, "wgh"),
    (Flaw::EmitAtZero, "wgh"),
    (Flaw::StoresDistance, "wgh"),
    (Flaw::SoftnessAlwaysBlended, "wgh"),
    (Flaw::NoMargin, "rad"),
    (Flaw::RoundNotCeil, "rad"),
    (Flaw::BoneRadiusNotSkipped, "rad"),
    (Flaw::NoDefault200, "rad"),
];

struct Flawed(Flaw);

fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

fn locate(b: &NearestBone, p: [f64; 2]) -> Result<Option<NearestLocation>, BoneNearestError> {
    let segs = b
        .rest_segments
        .as_deref()
        .ok_or_else(|| BoneNearestError::MissingRestSegments {
            bone_id: b.id.clone(),
        })?;
    nearest_on_curves(&path_curves(segs, b.closed), b.closed, p).map_err(|error| {
        BoneNearestError::Unresolved {
            bone_id: b.id.clone(),
            error,
        }
    })
}

impl Ops for Flawed {
    fn weigh(
        &self,
        bones: &[NearestBone],
        ids: &[String],
        p: [f64; 2],
        radius: f64,
        softness: f64,
    ) -> Result<Vec<OffsetWeight>, BoneNearestError> {
        let flaw = self.0;
        let mut out = Vec::new();
        for bid in ids {
            let Some(b) = bones.iter().find(|b| &b.id == bid) else {
                continue;
            };
            let Some(loc) = locate(b, p)? else {
                continue;
            };
            let (dx, dy) = (p[0] - loc.point[0], p[1] - loc.point[1]);
            let dist = (dx * dx + dy * dy).sqrt();
            let own = b.radius.unwrap_or(f64::NAN);
            let reach = if flaw == Flaw::RadiusPrecedenceSwapped {
                if truthy(radius) {
                    radius
                } else {
                    own
                }
            } else if truthy(own) {
                own
            } else {
                radius
            };
            let raw = 1.0 - dist / reach;
            let t = if flaw == Flaw::TaperNotClamped || raw.is_nan() {
                raw
            } else {
                raw.max(0.0)
            };
            let smooth = if flaw == Flaw::WrongSmoothstep {
                t * t * (2.0 - 3.0 * t)
            } else {
                t * t * (3.0 - 2.0 * t)
            };
            let infl = if truthy(softness) || flaw == Flaw::SoftnessAlwaysBlended {
                t + (smooth - t) * softness
            } else {
                t
            };
            let keep = if flaw == Flaw::EmitAtZero {
                infl >= 0.0
            } else {
                infl > 0.0
            };
            if keep {
                let offset = if flaw == Flaw::StoresDistance {
                    loc.distance
                } else {
                    loc.offset
                };
                out.push(OffsetWeight {
                    bone_id: bid.clone(),
                    offset: Some(offset),
                    w: infl,
                });
            }
        }
        Ok(out)
    }

    fn radius(
        &self,
        default: Option<f64>,
        bones: &[NearestBone],
        points: &[[f64; 2]],
    ) -> Result<Option<f64>, BoneNearestError> {
        let flaw = self.0;
        if bones.is_empty() {
            return Ok(None);
        }
        let mut needed = match default {
            Some(d) if truthy(d) || flaw == Flaw::NoDefault200 => d,
            _ if flaw == Flaw::NoDefault200 => f64::NAN,
            _ => 200.0,
        };
        for p in points {
            for b in bones {
                if b.radius.is_some_and(truthy) && flaw != Flaw::BoneRadiusNotSkipped {
                    continue;
                }
                let Some(loc) = locate(b, *p)? else {
                    continue;
                };
                let (dx, dy) = (p[0] - loc.point[0], p[1] - loc.point[1]);
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > needed {
                    needed = dist;
                }
            }
        }
        let scaled = if flaw == Flaw::NoMargin {
            needed
        } else {
            needed * 1.05
        };
        Ok(Some(if flaw == Flaw::RoundNotCeil {
            (scaled + 0.5).floor()
        } else {
            scaled.ceil()
        }))
    }
}

fn kinds(bad: &[String]) -> BTreeSet<String> {
    bad.iter()
        .map(|m| m.split(':').next().unwrap_or("").to_string())
        .collect()
}

#[test]
fn the_flaw_free_copy_equals_production_on_every_row() {
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
fn a_swapped_radius_precedence_changes_a_pinned_weight() {
    let b = [bone("a", Some(50.0), line(0.0, 0.0, 100.0, 0.0))];
    let ids = ["a".to_string()];
    let real = Production
        .weigh(&b, &ids, [30.0, 40.0], 200.0, 0.0)
        .expect("resolved");
    let flawed = Flawed(Flaw::RadiusPrecedenceSwapped)
        .weigh(&b, &ids, [30.0, 40.0], 200.0, 0.0)
        .expect("resolved");
    assert!((real[0].w - 0.2).abs() < 1e-12 && (flawed[0].w - 0.8).abs() < 1e-12);
}

#[test]
fn the_unclamped_taper_and_the_blend_without_a_clamp_emit_outside_the_reach() {
    let a = [bone("a", None, line(0.0, 0.0, 100.0, 0.0))];
    let ids = ["a".to_string()];
    assert!(Production
        .weigh(&a, &ids, [30.0, 400.0], 200.0, 1.0)
        .expect("ok")
        .is_empty());
    let unclamped = Flawed(Flaw::TaperNotClamped)
        .weigh(&a, &ids, [30.0, 190.0], 100.0, 1.0)
        .expect("ok");
    assert_eq!(
        unclamped.len(),
        1,
        "a negative taper blended with the smoothstep turns positive"
    );
    assert!(Production
        .weigh(&a, &ids, [30.0, 190.0], 100.0, 1.0)
        .expect("ok")
        .is_empty());
}

#[test]
fn the_margin_the_ceiling_and_the_default_are_each_pinned() {
    let a = [bone("a", None, line(0.0, 0.0, 100.0, 0.0))];
    let pts = [[0.0, 100.0]];
    assert_eq!(Production.radius(Some(100.0), &a, &pts), Ok(Some(105.0)));
    assert_eq!(
        Flawed(Flaw::NoMargin).radius(Some(100.0), &a, &pts),
        Ok(Some(100.0))
    );
    let far = [[50.0, 249.7]];
    assert_eq!(Production.radius(Some(100.0), &a, &far), Ok(Some(263.0)));
    assert_eq!(
        Flawed(Flaw::RoundNotCeil).radius(Some(100.0), &a, &far),
        Ok(Some(262.0))
    );
    assert_eq!(Production.radius(None, &a, &[]), Ok(Some(210.0)));
    assert!(Flawed(Flaw::NoDefault200)
        .radius(None, &a, &[])
        .expect("ok")
        .is_some_and(f64::is_nan));
    let own = [bone("a", Some(40.0), line(0.0, 0.0, 100.0, 0.0))];
    assert_eq!(
        Production.radius(Some(200.0), &own, &[[50.0, 900.0]]),
        Ok(Some(210.0))
    );
    assert_eq!(
        Flawed(Flaw::BoneRadiusNotSkipped).radius(Some(200.0), &own, &[[50.0, 900.0]]),
        Ok(Some(945.0))
    );
}

#[test]
fn the_flawed_weigher_keeps_the_typed_edges() {
    let missing = [bone("m", None, None)];
    assert!(Flawed(Flaw::None)
        .weigh(&missing, &["m".to_string()], [1.0, 1.0], 200.0, 0.0)
        .is_err());
}
