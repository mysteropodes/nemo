//! Paper.js `Path.getNearestLocation` (I12): the nearest point of a bone's curve to a query point, with
//! the path-length `offset` the rig stores in a legacy weight entry and the `distance` the falloff reads.
//!
//! Mirrors the shipped `src/paper-full.min.js` (Paper.js 0.12.18) used by `_boneSegsToPath`,
//! `rigWeighOnePoint` and `rigAutoAssignLayer` of the baseline at fbe4b8a3 (`src/js/app.js`):
//! - `Path.getNearestLocation`: every curve is asked, the strictly smaller `_distance` wins (so the first
//!   curve wins a tie); a path without a curve (no segments, or one open segment) gives `None`;
//! - `Curve.getNearestTime`: a STRAIGHT curve (`Curve.isStraight`) projects the query on its chord (a
//!   zero-length chord is time 0, a projection below 1e-12 is 0, above 0.999999999999 is 1, else
//!   `Curve.getTimeOf` of the projected point); any other curve samples 101 times, then refines with a
//!   step of 0.005 that halves until 1e-8 (`rig_pose_nearest_solve` / `rig_pose_nearest_curve`);
//! - `new CurveLocation(curve, time, point, null, distance)`: a time of at least 0.99999999 hands over
//!   to the NEXT curve at time 0 (the last curve of a closed path hands over to the first, the last of an
//!   open path keeps its own end), while `point` and `distance` stay those of the original time;
//! - `location.offset`: the lengths of the curves before the final one, summed in order from zero, plus
//!   `Curve.getLength(values, 0, time)` of the final one.
//!
//! The curves come from `property_follow_path_sample::path_curves` (the D30 Paper path model, handles
//! default to `[0, 0]`) and the previous-curve length from its `path_length`; nothing builds a second path.
//!
//! Kept on purpose: the offset is NOT the arc length to the returned `point` when a hand-over happened
//! (it is the start of the next curve, which is the same place); equidistant curves resolve to the lowest
//! index; a straight curve with zero handles has the smoothstep time `3t^2 - 2t^3`, so a mid-chord query
//! gets a time of about 0.36 for a chord fraction of 0.3 and a point that is a few ulps off the chord.
//!
//! Deliberate difference, typed: when `getTimeOf` finds no time (only coordinates beyond about 1e11 were
//! seen to do it) Paper builds a location whose `point` is null and whose `_distance` is the distance of
//! the QUERY to the origin; if that bogus location wins, every baseline caller either throws a TypeError
//! (`loc.point.getDistance`) or stores a meaningless offset. That case is
//! [`NearestError::TimeUnresolved`]; an unresolved location that loses to a resolved one changes nothing.

use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_curve::{
    is_straight, point_at, range_length, Values,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_solve::time_of;
use crate::evaluation::property_follow_path_sample::{path_curves, path_length, SegmentIn};

/// The nearest location: `point` and `distance` at the nearest time of the matched curve; `curve_index`
/// and `time` after the hand-over; `offset` the path-length offset of that curve time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NearestLocation {
    pub(crate) point: [f64; 2],
    pub(crate) offset: f64,
    pub(crate) distance: f64,
    pub(crate) curve_index: usize,
    pub(crate) time: f64,
}

/// A winning location that has no point (see the module note).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NearestError {
    TimeUnresolved { curve_index: usize },
}

/// What one `Curve.getNearestLocation` yields before the hand-over: `time` and `point` are `None` when
/// `getTimeOf` found none, and `distance` is then the query's distance to the origin (Paper's quirk).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CurveHit {
    pub(crate) time: Option<f64>,
    pub(crate) point: Option<[f64; 2]>,
    pub(crate) distance: f64,
}

/// The running minimum of the sampled squared distance and the time that gave it (`getNearestTime`'s
/// `o` and `h`).
struct Probe {
    dist: f64,
    time: f64,
}

impl Probe {
    /// `refine(t)`: true when `t` is inside `[0, 1]` and strictly closer than every earlier time.
    fn try_time(&mut self, v: &Values, p: [f64; 2], t: f64) -> bool {
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let Some(q) = point_at(v, t) else {
            return false;
        };
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let d = dx * dx + dy * dy;
        if d < self.dist {
            self.dist = d;
            self.time = t;
            return true;
        }
        false
    }
}

/// `Curve.getNearestTime(values, point)`: `None` only for a straight curve whose projected point
/// `getTimeOf` cannot place.
pub(crate) fn nearest_time(v: &Values, p: [f64; 2]) -> Option<f64> {
    if is_straight(v) {
        let (x0, y0) = (v[0], v[1]);
        let (r, s) = (v[6] - x0, v[7] - y0);
        let len2 = r * r + s * s;
        if len2 == 0.0 {
            return Some(0.0);
        }
        let a = ((p[0] - x0) * r + (p[1] - y0) * s) / len2;
        return if a < 1e-12 {
            Some(0.0)
        } else if 0.999999999999 < a {
            Some(1.0)
        } else {
            time_of(v, [x0 + a * r, y0 + a * s])
        };
    }
    let mut probe = Probe {
        dist: f64::INFINITY,
        time: 0.0,
    };
    for l in 0..=100 {
        probe.try_time(v, p, l as f64 / 100.0);
    }
    let mut step = 0.005;
    while 1e-8 < step {
        let h = probe.time;
        if !probe.try_time(v, p, h - step) && !probe.try_time(v, p, h + step) {
            step /= 2.0;
        }
    }
    Some(probe.time)
}

/// `Curve.getNearestLocation(point)` before the hand-over.
pub(crate) fn curve_hit(v: &Values, p: [f64; 2]) -> CurveHit {
    let time = nearest_time(v, p);
    let point = time.and_then(|t| point_at(v, t));
    let distance = match point {
        Some(q) => {
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            (dx * dx + dy * dy).sqrt()
        }
        None => (p[0] * p[0] + p[1] * p[1]).sqrt(),
    };
    CurveHit {
        time,
        point,
        distance,
    }
}

/// The `CurveLocation` constructor's hand-over: a time of at least 0.99999999 moves to the next curve
/// (the first one for the last curve of a closed path) at time 0.
fn hand_over(count: usize, closed: bool, index: usize, time: f64) -> (usize, f64) {
    if 0.99999999 <= time {
        if index + 1 < count {
            return (index + 1, 0.0);
        }
        if closed && count > 0 {
            return (0, 0.0);
        }
    }
    (index, time)
}

/// `Path.getNearestLocation(point)` over prebuilt curves (`path_curves` output): `Ok(None)` is Paper's
/// null (no curve, or every distance NaN or infinite).
pub(crate) fn nearest_on_curves(
    curves: &[Values],
    closed: bool,
    p: [f64; 2],
) -> Result<Option<NearestLocation>, NearestError> {
    let mut min_dist = f64::INFINITY;
    let mut best: Option<(usize, CurveHit)> = None;
    for (i, v) in curves.iter().enumerate() {
        let hit = curve_hit(v, p);
        if hit.distance < min_dist {
            min_dist = hit.distance;
            best = Some((i, hit));
        }
    }
    let Some((index, hit)) = best else {
        return Ok(None);
    };
    let (Some(time), Some(point)) = (hit.time, hit.point) else {
        return Err(NearestError::TimeUnresolved { curve_index: index });
    };
    let (index, time) = hand_over(curves.len(), closed, index, time);
    let offset = path_length(&curves[..index]) + range_length(&curves[index], 0.0, time);
    Ok(Some(NearestLocation {
        point,
        offset,
        distance: hit.distance,
        curve_index: index,
        time,
    }))
}

/// `new Path({insert:false})` from segments, then `getNearestLocation(point)`.
pub(crate) fn nearest_location(
    segments: &[SegmentIn],
    closed: bool,
    p: [f64; 2],
) -> Result<Option<NearestLocation>, NearestError> {
    nearest_on_curves(&path_curves(segments, closed), closed, p)
}

#[cfg(test)]
#[path = "../tests/rig_pose_nearest_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_nearest_flawed.rs"]
mod flawed;
#[cfg(test)]
#[path = "../tests/rig_pose_nearest_rows.rs"]
pub(crate) mod oracle_rows;
#[cfg(test)]
#[path = "../tests/rig_pose_nearest_pins.rs"]
mod pins_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_nearest_prims.rs"]
mod prim_rows;
#[cfg(test)]
#[path = "../tests/rig_pose_nearest.rs"]
mod tests;
