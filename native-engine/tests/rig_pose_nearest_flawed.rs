//! Flawed copies of the nearest-location pipeline (L03-I controls). Each [`Flaw`] is one plausible mistake;
//! with [`Flaw::None`] the copy equals production, so a rejection by the oracle rows is caused by the flaw alone.
//! Every flaw is also a textual mutant of the shipped Paper build that `tools/gen.cjs` proves the oracle
//! catches (`KERNEL_MUTANTS`). The copies reuse the production primitives (curve eval, `time_of`, lengths) and
//! restate only the selection, search, hand-over and offset logic that `rig_pose_nearest.rs` owns.

use super::oracle_rows::Ops;
use super::{NearestError, NearestLocation};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_curve::{
    is_straight, point_at, range_length, Values,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_solve::time_of;
use crate::evaluation::property_follow_path_sample::{path_curves, path_length, SegmentIn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flaw {
    None,
    /// `<=` instead of `<`: the last equidistant curve wins.
    TieLast,
    /// No hand-over to the next curve.
    NoHandOver,
    /// Hand over from a time of 0.9 instead of 0.99999999.
    HandOverAt09,
    /// 11 samples instead of 101.
    CoarseSamples,
    /// No refinement after the sampling.
    NoRefine,
    /// The refinement stops at a step of 1e-3 instead of 1e-8.
    CoarseRefineFloor,
    /// The offset omits the part length of the final curve.
    NoPartLength,
    /// The offset omits the lengths of the curves before the final one.
    NoPreviousLengths,
    /// A straight curve answers with the chord fraction instead of the curve time.
    LinearStraightTime,
    /// A straight curve is searched by sampling like any other.
    StraightBySampling,
    /// The squared distance is stored instead of the distance.
    SquaredDistance,
    /// The last curve of a closed path does not hand over to the first.
    NoClosedWrap,
    /// A zero-length chord is not special-cased.
    ZeroChordNotSpecial,
    /// A projection below 1e-12 is not snapped to time 0.
    StraightLowClampOff,
}

/// Every flaw with the row kind that must reject it.
pub(crate) const FLAWS: [(Flaw, &str); 14] = [
    (Flaw::TieLast, "nst"),
    (Flaw::NoHandOver, "nst"),
    (Flaw::HandOverAt09, "nst"),
    (Flaw::CoarseSamples, "nst"),
    (Flaw::NoRefine, "nst"),
    (Flaw::CoarseRefineFloor, "nst"),
    (Flaw::NoPartLength, "nst"),
    (Flaw::NoPreviousLengths, "nst"),
    (Flaw::LinearStraightTime, "nst"),
    (Flaw::StraightBySampling, "nst"),
    (Flaw::SquaredDistance, "nst"),
    (Flaw::NoClosedWrap, "nst"),
    (Flaw::ZeroChordNotSpecial, "nst"),
    (Flaw::StraightLowClampOff, "nst"),
];

pub(crate) struct Flawed(pub(crate) Flaw);

fn sampled_time(flaw: Flaw, v: &Values, p: [f64; 2]) -> f64 {
    let (samples, floor) = match flaw {
        Flaw::CoarseSamples => (10, 1e-8),
        Flaw::CoarseRefineFloor => (100, 1e-3),
        _ => (100, 1e-8),
    };
    let (mut best, mut time) = (f64::INFINITY, 0.0);
    let try_time = |t: f64, best: &mut f64, time: &mut f64| -> bool {
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let Some(q) = point_at(v, t) else {
            return false;
        };
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let d = dx * dx + dy * dy;
        if d < *best {
            *best = d;
            *time = t;
            return true;
        }
        false
    };
    for l in 0..=samples {
        try_time(l as f64 / samples as f64, &mut best, &mut time);
    }
    let mut step = 0.005;
    while flaw != Flaw::NoRefine && floor < step {
        let h = time;
        if !try_time(h - step, &mut best, &mut time) && !try_time(h + step, &mut best, &mut time) {
            step /= 2.0;
        }
    }
    time
}

fn time_for(flaw: Flaw, v: &Values, p: [f64; 2]) -> Option<f64> {
    if !is_straight(v) || flaw == Flaw::StraightBySampling {
        return Some(sampled_time(flaw, v, p));
    }
    let (x0, y0) = (v[0], v[1]);
    let (r, s) = (v[6] - x0, v[7] - y0);
    let len2 = r * r + s * s;
    if len2 == 0.0 && flaw != Flaw::ZeroChordNotSpecial {
        return Some(0.0);
    }
    let a = ((p[0] - x0) * r + (p[1] - y0) * s) / len2;
    if a < 1e-12 && flaw != Flaw::StraightLowClampOff {
        return Some(0.0);
    }
    if 0.999999999999 < a {
        return Some(1.0);
    }
    if flaw == Flaw::LinearStraightTime {
        return Some(a);
    }
    time_of(v, [x0 + a * r, y0 + a * s])
}

fn nearest(
    flaw: Flaw,
    segs: &[SegmentIn],
    closed: bool,
    p: [f64; 2],
) -> Result<Option<NearestLocation>, NearestError> {
    let curves = path_curves(segs, closed);
    let mut min_dist = f64::INFINITY;
    let mut best: Option<(usize, Option<f64>, Option<[f64; 2]>, f64)> = None;
    for (i, v) in curves.iter().enumerate() {
        let time = time_for(flaw, v, p);
        let point = time.and_then(|t| point_at(v, t));
        let distance = match point {
            Some(q) => {
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let sq = dx * dx + dy * dy;
                if flaw == Flaw::SquaredDistance {
                    sq
                } else {
                    sq.sqrt()
                }
            }
            None => (p[0] * p[0] + p[1] * p[1]).sqrt(),
        };
        let wins = if flaw == Flaw::TieLast {
            distance <= min_dist
        } else {
            distance < min_dist
        };
        if wins {
            min_dist = distance;
            best = Some((i, time, point, distance));
        }
    }
    let Some((index, time, point, distance)) = best else {
        return Ok(None);
    };
    let (Some(time), Some(point)) = (time, point) else {
        return Err(NearestError::TimeUnresolved { curve_index: index });
    };
    let threshold = if flaw == Flaw::HandOverAt09 {
        0.9
    } else {
        0.99999999
    };
    let (mut at, mut t) = (index, time);
    if flaw != Flaw::NoHandOver && threshold <= t {
        if index + 1 < curves.len() {
            (at, t) = (index + 1, 0.0);
        } else if closed && flaw != Flaw::NoClosedWrap {
            (at, t) = (0, 0.0);
        }
    }
    let before = if flaw == Flaw::NoPreviousLengths {
        0.0
    } else {
        path_length(&curves[..at])
    };
    let part = if flaw == Flaw::NoPartLength {
        0.0
    } else {
        range_length(&curves[at], 0.0, t)
    };
    Ok(Some(NearestLocation {
        point,
        offset: before + part,
        distance,
        curve_index: at,
        time: t,
    }))
}

impl Ops for Flawed {
    fn nearest(
        &self,
        segs: &[SegmentIn],
        closed: bool,
        p: [f64; 2],
    ) -> Result<Option<NearestLocation>, NearestError> {
        nearest(self.0, segs, closed, p)
    }
}
