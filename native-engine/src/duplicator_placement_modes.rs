//! Duplicator count caps and mode base offsets (L04-B, I07).
//!
//! Mirrors `_duplicatorCount`, `_duplicatorModeOffset` and the selection and
//! refusal rules of `_resolveDuplicatorPath` (`src/js/app.js`) at baseline
//! `fbe4b8a3`: how many clones a duplicator places, and where clone `k` sits
//! for the grid, radial and path modes BEFORE any random stagger or effector
//! contribution. The path curve mathematics is NOT re-derived: the Paper.js
//! length, point and tangent come from `property_follow_path_sample` (D30),
//! the same sampler `getPointAt`/`getTangentAt` map to.
//!
//! Every field is read with the JavaScript rules it had in the baseline:
//! `x || d` (undefined, null, 0, -0 and NaN all read as the default), the
//! `Math.max(1, ..)` and `Math.min(cap, ..)` clamps, floating `%` and
//! `Math.floor`, so a count can be fractional (`rows: 2.5`) and an unusual
//! field yields the same number the baseline did. The kernel owns no layer
//! lookup: the layer that supplies the path is a caller port
//! ([`PathSource`], `None` when the uid does not resolve).

use crate::evaluation::property_follow_path_sample::{
    path_curves, path_length, point_tangent_at, SegmentIn,
};
use std::f64::consts::PI;

/// Clone-count ceiling of grid mode (`Math.min(900, rows * cols)`).
pub(crate) const GRID_CAP: f64 = 900.0;
/// Clone-count ceiling of every other mode (`Math.min(500, count)`).
pub(crate) const OTHER_CAP: f64 = 500.0;

/// A numeric duplicator field as the JSON the project holds it can present
/// it. `undefined` and `null` stay distinct because the baseline subtracts
/// `startAngle` from `endAngle` (`undefined` is NaN there, `null` is 0).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum JsNumber {
    #[default]
    Undefined,
    Null,
    Value(f64),
}

impl JsNumber {
    /// `x || default`: only a nonzero, non-NaN number survives.
    pub(crate) fn or_default(self, default: f64) -> f64 {
        match self {
            JsNumber::Value(v) if !(v == 0.0 || v.is_nan()) => v,
            _ => default,
        }
    }

    /// `x != null`.
    pub(crate) fn is_present(self) -> bool {
        !matches!(self, JsNumber::Undefined | JsNumber::Null)
    }

    /// The operand of a `-`: `undefined` is NaN, `null` is 0.
    pub(crate) fn operand(self) -> f64 {
        match self {
            JsNumber::Undefined => f64::NAN,
            JsNumber::Null => 0.0,
            JsNumber::Value(v) => v,
        }
    }
}

/// `dup.mode || 'grid'` decided against the three labels the baseline
/// compares with `===`. Any other label is `Other`: counted like a path or
/// radial mode (cap 500) and placed by the final `else` of the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DupMode {
    Grid,
    Radial,
    Path,
    Other,
}

/// `undefined`, `null` and the empty label all read as `'grid'`.
pub(crate) fn dup_mode(label: Option<&str>) -> DupMode {
    match label {
        None | Some("") | Some("grid") => DupMode::Grid,
        Some("radial") => DupMode::Radial,
        Some("path") => DupMode::Path,
        Some(_) => DupMode::Other,
    }
}

/// The fields of `layer.duplicator` this kernel reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DuplicatorSpec<'a> {
    pub mode: Option<&'a str>,
    pub rows: JsNumber,
    pub cols: JsNumber,
    pub count: JsNumber,
    pub spacing_x: JsNumber,
    pub spacing_y: JsNumber,
    pub start_angle: JsNumber,
    pub end_angle: JsNumber,
    pub radius: JsNumber,
    pub radial_orient: bool,
    pub path_align_tangent: bool,
}

/// `Math.min`: NaN when either side is NaN. (No signed-zero tie occurs at
/// the call sites: every right-hand side is at least 1 or positive.)
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max` with the same NaN rule.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `_duplicatorCount(dup)`. A value in `[1, 900]` (grid) or `[1, 500]`; it
/// is NOT an integer when a field is fractional. Loop with [`clone_indices`].
pub(crate) fn duplicator_count(spec: &DuplicatorSpec) -> f64 {
    duplicator_count_with_caps(spec, GRID_CAP, OTHER_CAP)
}

/// [`duplicator_count`] with explicit ceilings, so a control can feed a
/// wrong cap through the same code path.
pub(crate) fn duplicator_count_with_caps(
    spec: &DuplicatorSpec,
    grid_cap: f64,
    other_cap: f64,
) -> f64 {
    if dup_mode(spec.mode) == DupMode::Grid {
        js_min(
            grid_cap,
            js_max(1.0, spec.rows.or_default(1.0)) * js_max(1.0, spec.cols.or_default(1.0)),
        )
    } else {
        js_min(other_cap, js_max(1.0, spec.count.or_default(1.0)))
    }
}

/// `Math.max(1, dup.cols || 1)`, the column count the callers pass to
/// [`mode_offset`] (`applyLayerDuplicator` and both video branches).
pub(crate) fn grid_cols(spec: &DuplicatorSpec) -> f64 {
    js_max(1.0, spec.cols.or_default(1.0))
}

/// Iterations of `for (k = 0; k < count; k++)`: `ceil(count)` for the
/// positive counts [`duplicator_count`] returns.
pub(crate) fn clone_indices(count: f64) -> usize {
    if count.is_nan() || count <= 0.0 {
        0
    } else {
        count.ceil() as usize
    }
}

/// `if (count <= 1) return base`: no clone is placed at all.
pub(crate) fn places_no_clones(count: f64) -> bool {
    count <= 1.0
}

/// One stroke of the layer that supplies the path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PathStroke<'a> {
    pub is_raster: bool,
    /// `None` is a stroke without a `segments` field.
    pub segments: Option<&'a [SegmentIn]>,
    pub closed: bool,
}

/// The layer `pathLayerUid` resolved to, with its strokes at the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PathSource<'a> {
    /// `srcLd.duplicator` is set: a duplicator cannot be a path source.
    pub has_duplicator: bool,
    pub strokes: &'a [PathStroke<'a>],
}

/// What `_resolveDuplicatorPath` returns besides the Paper `Path` itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathInfo {
    pub curves: Vec<[f64; 8]>,
    pub length: f64,
    pub closed: bool,
    /// `path.getPointAt(0)`.
    pub start: [f64; 2],
}

/// Where the baseline throws a `TypeError` instead of returning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathResolveError {
    /// A segment of the chosen stroke has no `handleIn` or `handleOut`
    /// (`s.handleIn[0]` is read on it).
    MissingHandle,
    /// `getPointAt(0)` was null on a path of positive length. Paper cannot
    /// return this (the first curve of positive length always yields time 0);
    /// the baseline would throw reading `start.x` later.
    StartUnavailable,
}

/// `_resolveDuplicatorPath` over a supplied candidate: `Ok(None)` is the
/// baseline's `null` (uid not found, a duplicator source, no stroke with two
/// or more segments, or zero/NaN length).
pub(crate) fn resolve_path(
    source: Option<&PathSource>,
) -> Result<Option<PathInfo>, PathResolveError> {
    let source = match source {
        Some(s) => s,
        None => return Ok(None),
    };
    if source.has_duplicator {
        return Ok(None);
    }
    let found = source.strokes.iter().find_map(|s| {
        if s.is_raster {
            return None;
        }
        s.segments.filter(|g| g.len() >= 2).map(|g| (g, s.closed))
    });
    let (segments, closed) = match found {
        Some(v) => v,
        None => return Ok(None),
    };
    if segments
        .iter()
        .any(|s| s.handle_in.is_none() || s.handle_out.is_none())
    {
        return Err(PathResolveError::MissingHandle);
    }
    let curves = path_curves(segments, closed);
    let length = path_length(&curves);
    if length == 0.0 || length.is_nan() {
        return Ok(None);
    }
    let start = match point_tangent_at(&curves, closed, 0.0) {
        Some(p) => p.point,
        None => return Err(PathResolveError::StartUnavailable),
    };
    Ok(Some(PathInfo {
        curves,
        length,
        closed,
        start,
    }))
}

/// `{baseDx, baseDy, baseRot}` of `_duplicatorModeOffset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ModeOffset {
    pub base_dx: f64,
    pub base_dy: f64,
    pub base_rot: f64,
}

/// Why [`mode_offset`] has no answer where the baseline throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeOffsetError {
    /// The final `else` (path) branch ran with `pathInfo == null`: any mode
    /// other than grid and radial, when `_resolveDuplicatorPath` was not
    /// consulted or returned null.
    PathInfoRequired,
}

/// `Point.angle`: `180 * atan2(y, x) / PI`, and 0 for a tangent whose both
/// components are within 1e-12 of zero (`isZero()` then reads `_angle || 0`).
fn tangent_angle_degrees(t: [f64; 2]) -> f64 {
    let near_zero = |v: f64| (-1e-12..=1e-12).contains(&v);
    if near_zero(t[0]) && near_zero(t[1]) {
        0.0
    } else {
        180.0 * t[1].atan2(t[0]) / PI
    }
}

/// `_duplicatorModeOffset(dup, mode, k, count, cols, pathInfo)`; `count` and
/// `cols` are the values the caller computed with [`duplicator_count`] and
/// [`grid_cols`]. `path` is `Some` only when the mode is `path` and the path
/// resolved.
pub(crate) fn mode_offset(
    spec: &DuplicatorSpec,
    k: u32,
    count: f64,
    cols: f64,
    path: Option<&PathInfo>,
) -> Result<ModeOffset, ModeOffsetError> {
    let kf = k as f64;
    match dup_mode(spec.mode) {
        DupMode::Grid => Ok(ModeOffset {
            base_dx: (kf % cols) * spec.spacing_x.or_default(0.0),
            base_dy: (kf / cols).floor() * spec.spacing_y.or_default(0.0),
            base_rot: 0.0,
        }),
        DupMode::Radial => {
            let span = if spec.end_angle.is_present() {
                spec.end_angle.operand() - spec.start_angle.operand()
            } else {
                360.0
            };
            let ang = spec.start_angle.or_default(0.0) + kf * (span / count);
            let rr = ang * PI / 180.0;
            let radius = spec.radius.or_default(0.0);
            Ok(ModeOffset {
                base_dx: radius * rr.cos(),
                base_dy: radius * rr.sin(),
                base_rot: if spec.radial_orient { ang } else { 0.0 },
            })
        }
        DupMode::Path | DupMode::Other => {
            let info = path.ok_or(ModeOffsetError::PathInfoRequired)?;
            let t = if info.closed {
                kf / count
            } else if count > 1.0 {
                kf / (count - 1.0)
            } else {
                0.0
            };
            let off = js_min(info.length, t * info.length);
            let hit = point_tangent_at(&info.curves, info.closed, off);
            let pt = hit.map(|h| h.point).unwrap_or(info.start);
            let base_rot = match (spec.path_align_tangent, hit.and_then(|h| h.tangent)) {
                (true, Some(tangent)) => tangent_angle_degrees(tangent),
                _ => 0.0,
            };
            Ok(ModeOffset {
                base_dx: pt[0] - info.start[0],
                base_dy: pt[1] - info.start[1],
                base_rot,
            })
        }
    }
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_modes_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_modes_pins.rs"]
mod pins_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_modes.rs"]
mod tests;
