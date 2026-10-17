//! Duplicator temporal offset and the copy loop (L04-D, I08).
//!
//! Mirrors the "temporal stagger" and stamping loop of `applyLayerDuplicator`
//! (`src/js/app.js` at baseline `fbe4b8a3`), without the Object-mode pool and
//! the mode/placement math, which are the sibling kernels' (the placement of
//! clone `k` is a caller port so this kernel owns no Motion sampling).
//!
//! When the offset is on, copy `k` re-samples the SEED layer's own content at
//! a shifted frame instead of reusing the current one:
//! `shiftedIdx = start + (((frame - start - shift) % span) + span) % span`,
//! which wraps the frame into `[start, start + span)` (a whole-frame shift
//! therefore lands in `[start, start + span - 1]`), with `span = max(1, len)`.
//! `shift` is `k * off` (forward, the default for any other label),
//! `(count - 1 - k) * off` (backward), `round(abs(k - (count - 1) / 2) * off)`
//! (centerOut) or `floor(rng() * count) * off` (random, a stream seeded with
//! `((dup.seed || 0) + k * 104729) >>> 0`, a different prime than the 7919 of
//! the placement stream so the two sequences are independent). The offset is
//! on only when it is enabled, a resample port exists and `(offsetFrames | 0)`
//! is not 0: an `offsetFrames` of `0.5` is OFF while `1.5` is on.
//!
//! A shifted frame that has content replaces the source AND recomputes the
//! pivot from its own bounds; a shifted frame with nothing drawn (a hold-blank
//! span) falls back to the current frame's content and pivot rather than
//! dropping the copy. `count <= 1` places no clone: the base is returned
//! untouched (`None`). The base and the resampled strokes are only read.

use crate::evaluation::object_component_kernels::duplicator_placement_apply::{
    apply_placement, ApplyError, DuplicatorCopy, PlacementApply,
};
use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::{
    js_max, js_round, to_int32,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    bounds_center, MatrixError, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::{
    clone_indices, places_no_clones, JsNumber, GRID_CAP,
};
use crate::evaluation::object_component_kernels::duplicator_placement_rng::{
    stream_seed, SeededRng,
};
use crate::object_document::ObjectRecord;

/// The prime that decorrelates the random temporal stream (`k * 104729`).
pub(crate) const TEMPORAL_STRIDE: f64 = 104_729.0;

/// `timeOffset.direction`; any label but the three below is `Forward`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Forward,
    Backward,
    CenterOut,
    Random,
}

impl Direction {
    pub(crate) fn from_label(label: Option<&str>) -> Direction {
        match label {
            Some("backward") => Direction::Backward,
            Some("centerOut") => Direction::CenterOut,
            Some("random") => Direction::Random,
            _ => Direction::Forward,
        }
    }
}

/// `dup.timeOffset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TemporalOffset<'a> {
    pub enabled: bool,
    pub offset_frames: f64,
    pub direction: Option<&'a str>,
}

/// The seed layer's span as the caller resolved it (`opts.spanStart` or the
/// layer in point; `opts.spanLen` or `outPoint - start + 1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpanBounds {
    pub start: f64,
    pub len: f64,
}

/// Deviations a control can feed through the real loop; the default is the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemporalRules {
    /// Wrap the shifted frame into the span (otherwise it is `frame - shift`).
    pub wrap_into_span: bool,
    /// Recompute the pivot from the shifted content (otherwise keep the base's).
    pub recompute_pivot: bool,
}

pub(crate) const BASELINE_RULES: TemporalRules = TemporalRules {
    wrap_into_span: true,
    recompute_pivot: true,
};

/// `tOff && tOff.enabled && (opts.resample || layerIdx != null) && (offsetFrames | 0) !== 0`.
pub(crate) fn temporal_on(offset: Option<&TemporalOffset<'_>>, can_resample: bool) -> bool {
    match offset {
        Some(t) => t.enabled && can_resample && to_int32(t.offset_frames) != 0,
        None => false,
    }
}

/// `shiftFrames` of clone `k` of `count`.
pub(crate) fn shift_frames(t: &TemporalOffset<'_>, k: u32, count: f64, seed: JsNumber) -> f64 {
    let kf = f64::from(k);
    let off = t.offset_frames;
    match Direction::from_label(t.direction) {
        Direction::Backward => (count - 1.0 - kf) * off,
        Direction::CenterOut => js_round((kf - (count - 1.0) / 2.0).abs() * off),
        Direction::Random => {
            let mut rng = SeededRng::new(stream_seed(seed, k, TEMPORAL_STRIDE));
            (rng.next_f64() * count).floor() * off
        }
        Direction::Forward => kf * off,
    }
}

/// `shiftedIdx` of the baseline.
pub(crate) fn shifted_index(frame_idx: f64, span: &SpanBounds, shift: f64) -> f64 {
    shifted_index_with(frame_idx, span, shift, true)
}

/// [`shifted_index`] with the wrap optional, so a control can drop it.
pub(crate) fn shifted_index_with(
    frame_idx: f64,
    span: &SpanBounds,
    shift: f64,
    wrap_into_span: bool,
) -> f64 {
    if !wrap_into_span {
        return frame_idx - shift;
    }
    let len = js_max(1.0, span.len);
    span.start + (((frame_idx - span.start - shift) % len) + len) % len
}

/// Everything the loop needs besides the strokes and the two ports.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CopyRequest<'a> {
    pub seed: JsNumber,
    /// `_duplicatorCount(dup)` (the capped value, possibly fractional).
    pub count: f64,
    pub frame_idx: f64,
    pub canvas_w: f64,
    pub canvas_h: f64,
    pub temporal: Option<TemporalOffset<'a>>,
    pub span: SpanBounds,
    /// `opts.resample || layerIdx != null`.
    pub can_resample: bool,
}

/// Why the copies could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuildError {
    /// More clone indices than the grid cap: the caller did not pass a capped count.
    CountAboveCap,
    Pivot(MatrixError),
    CopyFailed {
        index: u32,
        error: ApplyError,
    },
}

/// The copies of `base` (`applyLayerDuplicator` minus pool and path), or `None`
/// when `count <= 1` and the base stands as it is.
///
/// `place(k, pivot)` returns the placement descriptor of clone `k` for the
/// pivot of that clone's source strokes; `resample(frame)` returns the seed
/// layer's strokes at a frame (empty when nothing is drawn there).
pub(crate) fn build_copies(
    base: &[ObjectRecord],
    req: &CopyRequest<'_>,
    place: &mut dyn FnMut(u32, Pivot) -> PlacementApply,
    resample: &mut dyn FnMut(f64) -> Vec<ObjectRecord>,
) -> Result<Option<Vec<DuplicatorCopy>>, BuildError> {
    build_copies_with(BASELINE_RULES, base, req, place, resample)
}

/// [`build_copies`] under explicit rules.
pub(crate) fn build_copies_with(
    rules: TemporalRules,
    base: &[ObjectRecord],
    req: &CopyRequest<'_>,
    place: &mut dyn FnMut(u32, Pivot) -> PlacementApply,
    resample: &mut dyn FnMut(f64) -> Vec<ObjectRecord>,
) -> Result<Option<Vec<DuplicatorCopy>>, BuildError> {
    if places_no_clones(req.count) {
        return Ok(None);
    }
    let indices = clone_indices(req.count);
    if indices > GRID_CAP as usize {
        return Err(BuildError::CountAboveCap);
    }
    let pivot = bounds_center(base, req.canvas_w, req.canvas_h).map_err(BuildError::Pivot)?;
    let on = temporal_on(req.temporal.as_ref(), req.can_resample);
    let mut copies = Vec::new();
    for index in 0..indices {
        let k = index as u32;
        let mut shifted: Option<Vec<ObjectRecord>> = None;
        let mut pivot_k = pivot;
        if let (true, Some(t)) = (on, req.temporal.as_ref()) {
            let shift = shift_frames(t, k, req.count, req.seed);
            let at = shifted_index_with(req.frame_idx, &req.span, shift, rules.wrap_into_span);
            let content = resample(at);
            if !content.is_empty() {
                if rules.recompute_pivot {
                    pivot_k = bounds_center(&content, req.canvas_w, req.canvas_h)
                        .map_err(BuildError::Pivot)?;
                }
                shifted = Some(content);
            }
        }
        let placement = place(k, pivot_k);
        let source: &[ObjectRecord] = match shifted.as_deref() {
            Some(content) => content,
            None => base,
        };
        for record in source {
            let copy = apply_placement(record, k, req.seed, &placement, pivot_k)
                .map_err(|error| BuildError::CopyFailed { index: k, error })?;
            copies.push(copy);
        }
    }
    Ok(Some(copies))
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_temporal_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_temporal_codec.rs"]
pub(crate) mod test_codec;
#[cfg(test)]
#[path = "../tests/duplicator_placement_temporal.rs"]
mod tests;
