//! Flawed copies of the rig weights and anchors kernel for the controls (L03-D). Each `Flaw` is one
//! plausible mistake (most are JavaScript mutants the generator proved the oracle catches); with
//! `Flaw::None` every copy equals production. Used only by `rig_pose_weights_controls`.

use std::f64::consts::PI;

use super::oracle_rows::Ops;
use super::{
    anchor_for, chord_angle, clamp_softness, effective_radius, js_max, js_min, normalize_angle,
    truthy, Anchor, AnchorError, ChordError, ControlAnchor, ControlWeight, Point2, PointSource,
    WeightBone, WeightEntry, WeightParts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flaw {
    None,
    // weigher
    RadiusSwapped,
    BoneRadiusOnly,
    SoftBeforeClamp,
    ClampDropped,
    EmitZero,
    MissingPointAsOrigin,
    SoftNotZero,
    SmoothWrong,
    BlendDropped,
    LiveSegments,
    IndexCompacted,
    Manhattan,
    // chord angle
    ChordShortGuard,
    ChordNoStartClamp,
    ChordNoEndClamp,
    ChordAtanSwapped,
    ChordReversed,
    // normalize
    NormSingleMod,
    NormNoShift,
    NormPiStaysPi,
    // shapes and anchors
    ClassifyTruthy,
    ClassifyOffsetFirst,
    AnchorNoNormalize,
    AnchorDaReversed,
    AnchorRestSwapped,
    AnchorOneSidedGuard,
    AnchorBoneGuard,
    // bind preprocessing
    BindRadiusNanKept,
    BindRadiusZeroKept,
    SoftNoUpperClamp,
    SoftNoLowerClamp,
    SoftNanKept,
}

/// Every flaw with the row kind that must reject it.
pub(crate) const FLAWS: [(Flaw, &str); 32] = [
    (Flaw::RadiusSwapped, "wgt"),
    (Flaw::BoneRadiusOnly, "wgt"),
    (Flaw::SoftBeforeClamp, "wgt"),
    (Flaw::ClampDropped, "wgt"),
    (Flaw::EmitZero, "wgt"),
    (Flaw::MissingPointAsOrigin, "wgt"),
    (Flaw::SoftNotZero, "wgt"),
    (Flaw::SmoothWrong, "wgt"),
    (Flaw::BlendDropped, "wgt"),
    (Flaw::LiveSegments, "wgt"),
    (Flaw::IndexCompacted, "wgt"),
    (Flaw::Manhattan, "wgt"),
    (Flaw::ChordShortGuard, "chd"),
    (Flaw::ChordNoStartClamp, "chd"),
    (Flaw::ChordNoEndClamp, "chd"),
    (Flaw::ChordAtanSwapped, "chd"),
    (Flaw::ChordReversed, "chd"),
    (Flaw::NormSingleMod, "nrm"),
    (Flaw::NormNoShift, "nrm"),
    (Flaw::NormPiStaysPi, "nrm"),
    (Flaw::ClassifyTruthy, "anc"),
    (Flaw::ClassifyOffsetFirst, "anc"),
    (Flaw::AnchorNoNormalize, "anc"),
    (Flaw::AnchorDaReversed, "anc"),
    (Flaw::AnchorRestSwapped, "anc"),
    (Flaw::AnchorOneSidedGuard, "anc"),
    (Flaw::AnchorBoneGuard, "anc"),
    (Flaw::BindRadiusNanKept, "bpr"),
    (Flaw::BindRadiusZeroKept, "bpr"),
    (Flaw::SoftNoUpperClamp, "bpr"),
    (Flaw::SoftNoLowerClamp, "bpr"),
    (Flaw::SoftNanKept, "bpr"),
];

pub(crate) struct Flawed(pub(crate) Flaw);

pub(crate) fn f_weigh(
    flaw: Flaw,
    bones: &[WeightBone],
    ids: &[String],
    p: Point2,
    radius: f64,
    softness: f64,
) -> Vec<ControlWeight> {
    let mut out = Vec::new();
    for bid in ids {
        let Some(bone) = bones.iter().find(|b| &b.id == bid) else {
            continue;
        };
        let list = if flaw == Flaw::LiveSegments {
            &bone.segments
        } else {
            &bone.rest_segments
        };
        let segs: &[Option<Point2>] = list.as_deref().unwrap_or(&[]);
        let reach = match flaw {
            Flaw::RadiusSwapped if truthy(radius) => radius,
            Flaw::RadiusSwapped => bone.radius.unwrap_or(f64::NAN),
            Flaw::BoneRadiusOnly => bone.radius.unwrap_or(radius),
            _ => effective_radius(bone, radius),
        };
        let mut emitted = 0usize;
        for (k, seg) in segs.iter().enumerate() {
            let cp = match seg {
                Some(cp) => *cp,
                None if flaw == Flaw::MissingPointAsOrigin => [0.0, 0.0],
                None => continue,
            };
            let (dx, dy) = (p[0] - cp[0], p[1] - cp[1]);
            let dist = if flaw == Flaw::Manhattan {
                dx.abs() + dy.abs()
            } else {
                (dx * dx + dy * dy).sqrt()
            };
            let infl = f_influence(flaw, 1.0 - dist / reach, softness);
            let keep = if flaw == Flaw::EmitZero {
                infl >= 0.0
            } else {
                infl > 0.0
            };
            if keep {
                let pt_index = if flaw == Flaw::IndexCompacted {
                    emitted
                } else {
                    k
                };
                emitted += 1;
                out.push(ControlWeight {
                    bone_id: bid.clone(),
                    pt_index,
                    w: infl,
                    stray_offset: None,
                });
            }
        }
    }
    out
}

pub(crate) fn f_influence(flaw: Flaw, t_raw: f64, softness: f64) -> f64 {
    let t = match flaw {
        Flaw::SoftBeforeClamp | Flaw::ClampDropped => t_raw,
        _ => js_max(0.0, t_raw),
    };
    let on = if flaw == Flaw::SoftNotZero {
        softness != 0.0
    } else {
        truthy(softness)
    };
    let smooth = if flaw == Flaw::SmoothWrong {
        t * t * (2.0 - 3.0 * t)
    } else {
        t * t * (3.0 - 2.0 * t)
    };
    let infl = match (on, flaw) {
        (true, Flaw::BlendDropped) => smooth,
        (true, _) => t + (smooth - t) * softness,
        (false, _) => t,
    };
    if flaw == Flaw::SoftBeforeClamp {
        js_max(0.0, infl)
    } else {
        infl
    }
}

pub(crate) fn f_params(flaw: Flaw, radius: Option<f64>, softness: Option<f64>) -> (f64, f64) {
    let r = match (flaw, radius) {
        (Flaw::BindRadiusNanKept, Some(r)) if r != 0.0 => r,
        (Flaw::BindRadiusZeroKept, Some(r)) if !r.is_nan() => r,
        (Flaw::BindRadiusNanKept | Flaw::BindRadiusZeroKept, _) => 200.0,
        _ => super::bind_radius(radius),
    };
    let s = match flaw {
        Flaw::SoftNoUpperClamp => js_max(0.0, softness.filter(|s| truthy(*s)).unwrap_or(0.0)),
        Flaw::SoftNoLowerClamp => js_min(1.0, softness.filter(|s| truthy(*s)).unwrap_or(0.0)),
        Flaw::SoftNanKept => js_max(0.0, js_min(1.0, softness.unwrap_or(0.0))),
        _ => clamp_softness(softness),
    };
    (r, s)
}

pub(crate) fn f_chord(flaw: Flaw, segs: &[Option<Point2>], k: usize) -> Result<f64, ChordError> {
    let len = segs.len();
    let min_len = if flaw == Flaw::ChordShortGuard { 1 } else { 2 };
    if len < min_len {
        return Ok(0.0);
    }
    let prev_i = if flaw == Flaw::ChordNoStartClamp {
        k as i64 - 1
    } else {
        k.saturating_sub(1) as i64
    };
    let next_i = if flaw == Flaw::ChordNoEndClamp {
        k as i64 + 1
    } else {
        (k + 1).min(len - 1) as i64
    };
    let slot = |i: i64| -> Result<Point2, ChordError> {
        let index = i.max(0) as usize;
        if i < 0 {
            return Err(ChordError { index });
        }
        segs.get(index)
            .copied()
            .flatten()
            .ok_or(ChordError { index })
    };
    let next = slot(next_i)?;
    let prev = slot(prev_i)?;
    let (dy, dx) = (next[1] - prev[1], next[0] - prev[0]);
    let (dy, dx) = if flaw == Flaw::ChordReversed {
        (-dy, -dx)
    } else {
        (dy, dx)
    };
    Ok(if flaw == Flaw::ChordAtanSwapped {
        dx.atan2(dy)
    } else {
        dy.atan2(dx)
    })
}

pub(crate) fn f_norm(flaw: Flaw, a: f64) -> f64 {
    match flaw {
        Flaw::NormSingleMod => (a + PI) % (2.0 * PI) - PI,
        Flaw::NormNoShift => (a % (2.0 * PI) + 2.0 * PI) % (2.0 * PI) - PI,
        Flaw::NormPiStaysPi => normalize_angle(a) + if a == PI { 2.0 * PI } else { 0.0 },
        _ => normalize_angle(a),
    }
}

pub(crate) fn f_classify(flaw: Flaw, mut parts: WeightParts) -> WeightEntry {
    let routed_away = match flaw {
        Flaw::ClassifyTruthy => parts.pt_index == Some(0),
        Flaw::ClassifyOffsetFirst => parts.offset.is_some(),
        _ => false,
    };
    if routed_away {
        parts.pt_index = None;
    }
    WeightEntry::classify(parts)
}

pub(crate) fn f_anchor(
    flaw: Flaw,
    bones: &[WeightBone],
    entry: &WeightEntry,
) -> Result<Anchor, AnchorError> {
    let WeightEntry::Control(c) = entry else {
        return anchor_for(bones, entry);
    };
    let k = c.pt_index;
    let Some(bone) = bones.iter().find(|b| b.id == c.bone_id) else {
        return Ok(Anchor::Unresolved);
    };
    let missing = |source: PointSource| AnchorError::MissingPoint { source, index: k };
    let (rest, live) = (bone.rest_segments.as_deref(), bone.segments.as_deref());
    let (Some(rest), Some(live)) = (rest, live) else {
        return if flaw == Flaw::AnchorBoneGuard {
            Err(missing(PointSource::Rest))
        } else {
            Ok(Anchor::Unresolved)
        };
    };
    let (rs, cs) = (rest.get(k), live.get(k));
    let (Some(rs), Some(cs)) = (rs, cs) else {
        // One-sided guard: only the rest side is checked, the live side then throws.
        return if flaw == Flaw::AnchorOneSidedGuard && rs.is_some() {
            Err(missing(PointSource::Live))
        } else {
            Ok(Anchor::Unresolved)
        };
    };
    let rp = (*rs).ok_or(missing(PointSource::Rest))?;
    let cp = (*cs).ok_or(missing(PointSource::Live))?;
    let lc = chord_angle(live, k).map_err(|e| AnchorError::MissingPoint {
        source: PointSource::Live,
        index: e.index,
    })?;
    let rc = chord_angle(rest, k).map_err(|e| AnchorError::MissingPoint {
        source: PointSource::Rest,
        index: e.index,
    })?;
    let da = match flaw {
        Flaw::AnchorNoNormalize => lc - rc,
        Flaw::AnchorDaReversed => normalize_angle(rc - lc),
        _ => normalize_angle(lc - rc),
    };
    let (rest_xy, cur_xy) = if flaw == Flaw::AnchorRestSwapped {
        (cp, rp)
    } else {
        (rp, cp)
    };
    Ok(Anchor::Control(ControlAnchor {
        rest_x: rest_xy[0],
        rest_y: rest_xy[1],
        cur_x: cur_xy[0],
        cur_y: cur_xy[1],
        da,
    }))
}

impl Ops for Flawed {
    fn norm(&self, a: f64) -> f64 {
        f_norm(self.0, a)
    }
    fn chord(&self, segs: &[Option<Point2>], k: usize) -> Result<f64, ChordError> {
        f_chord(self.0, segs, k)
    }
    fn weigh(
        &self,
        bones: &[WeightBone],
        ids: &[String],
        p: Point2,
        radius: f64,
        softness: f64,
    ) -> Vec<ControlWeight> {
        f_weigh(self.0, bones, ids, p, radius, softness)
    }
    fn bind(
        &self,
        bones: &[WeightBone],
        ids: &[String],
        pts: &[Point2],
        radius: Option<f64>,
        softness: Option<f64>,
    ) -> Vec<Vec<ControlWeight>> {
        let (r, s) = f_params(self.0, radius, softness);
        pts.iter()
            .map(|p| f_weigh(self.0, bones, ids, *p, r, s))
            .collect()
    }
    fn params(&self, radius: Option<f64>, softness: Option<f64>) -> (f64, f64) {
        f_params(self.0, radius, softness)
    }
    fn anchor(&self, bones: &[WeightBone], entry: &WeightEntry) -> Result<Anchor, AnchorError> {
        f_anchor(self.0, bones, entry)
    }
    fn classify(&self, parts: WeightParts) -> WeightEntry {
        f_classify(self.0, parts)
    }
}
