//! Rig deform anchors (I12): bones as the deform reads them, and the resolution of one
//! weight entry to the point it anchors on, in BOTH anchoring models.
//!
//! Mirrors `anchorFor` and `bonePaths` (nested in `applyRigDeform`) and `_boneSegsToPath`
//! of the baseline at fbe4b8a3 (`src/js/app.js`):
//! - an entry with a `ptIndex` anchors on a CONTROL POINT; that branch is the
//!   `rig_pose_weights::anchor_for` kernel, unchanged;
//! - an entry without one anchors on the bone CURVE at the arc-length `offset`: the rest
//!   and the live bone become two Paper paths (`closed` from the bone, every handle
//!   defaulting to `[0,0]`), `getLocationAt(offset)` is read on each, and the anchor is the
//!   pair of points plus `da = atan2(cur tangent) - atan2(rest tangent)`. That angle is NOT
//!   wrapped into `[-pi, pi)` (the control-point model wraps, this one never did);
//! - the location comes from `property_follow_path_sample` (D30): Paper's curve lengths,
//!   `getTimeAt` and `CurveLocation` hand-over, bit-exact with the shipped build;
//! - the curves of a bone are built once per resolver, like the baseline's `boneCache`,
//!   rest first then live, so the first failure reported is the one the baseline throws.
//!
//! Semantics kept on purpose: `getLocationAt` is null (the entry is skipped, not folded into
//! the weight sum) for a NaN offset, an offset past the end of EITHER path (the live bone can
//! be longer or shorter than the rest bone) and a path without a curve (zero segments, or one
//! open segment); a negative offset counts from the start of the first curve as Paper does;
//! a closed path runs back to its first segment.
//!
//! Deliberate differences, all typed: every baseline `TypeError` (a bone missing from the
//! rig, a bone without `restSegments`/`segments`, a segment without `.point`, a control-point
//! segment without `.point`) is a [`DeformError`] and nothing is produced; an `offset` of
//! `None` models an absent field (`getLocationAt(undefined)` is null). A stored JSON `null`
//! offset (`getLocationAt(null)` reads as `0`) is not something any baseline writer emits and
//! is for the record leaf to refuse at admission.

use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    anchor_for, Anchor, AnchorError, ControlAnchor, Point2, PointSource, WeightBone, WeightEntry,
};
use crate::evaluation::property_follow_path_sample::{path_curves, point_tangent_at, SegmentIn};

/// One stored bone segment: `{point, handleIn, handleOut}`. `point` is `None` for a segment
/// that carries none (the baseline throws when it must read it); a missing handle reads `[0,0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeformSegment {
    pub(crate) point: Option<Point2>,
    pub(crate) handle_in: Option<Point2>,
    pub(crate) handle_out: Option<Point2>,
}

/// A bone as the deform reads it. `None` for a list is an absent property (distinct from an
/// empty list); `rest_segments` is the bind pose, `segments` the live pose.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeformBone {
    pub(crate) id: String,
    pub(crate) closed: bool,
    pub(crate) segments: Option<Vec<DeformSegment>>,
    pub(crate) rest_segments: Option<Vec<DeformSegment>>,
}

/// Where the baseline would throw while resolving an anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeformError {
    /// The control-point branch (`rig_pose_weights::anchor_for`) refused the entry.
    Anchor(AnchorError),
    /// Building a bone's curve path: the list is absent (`index` `None`) or the segment at
    /// `index` has no `.point`.
    BonePath {
        bone_id: String,
        source: PointSource,
        index: Option<usize>,
    },
    /// A location resolved but gave no tangent (not reachable from Paper's evaluator).
    TangentUnavailable,
}

/// The rest and live paths of one bone as Paper curves.
struct BoneCurves {
    closed: bool,
    rest: Vec<[f64; 8]>,
    cur: Vec<[f64; 8]>,
}

/// Resolves weight entries against one rig, caching each bone's curves.
pub(crate) struct AnchorResolver<'a> {
    bones: &'a [DeformBone],
    weight_bones: Vec<WeightBone>,
    cache: Vec<Option<Result<BoneCurves, DeformError>>>,
}

fn points_of(list: &Option<Vec<DeformSegment>>) -> Option<Vec<Option<Point2>>> {
    list.as_ref()
        .map(|segs| segs.iter().map(|s| s.point).collect())
}

fn list_curves(
    bone: &DeformBone,
    source: PointSource,
    list: &Option<Vec<DeformSegment>>,
) -> Result<Vec<[f64; 8]>, DeformError> {
    let error = |index: Option<usize>| DeformError::BonePath {
        bone_id: bone.id.clone(),
        source,
        index,
    };
    let segs = list.as_ref().ok_or_else(|| error(None))?;
    let mut path = Vec::with_capacity(segs.len());
    for (i, s) in segs.iter().enumerate() {
        let point = s.point.ok_or_else(|| error(Some(i)))?;
        path.push(SegmentIn {
            point,
            handle_in: s.handle_in,
            handle_out: s.handle_out,
        });
    }
    Ok(path_curves(&path, bone.closed))
}

/// `bonePaths(bid)`: the rest path first, then the live path.
fn build_curves(bone: &DeformBone) -> Result<BoneCurves, DeformError> {
    let rest = list_curves(bone, PointSource::Rest, &bone.rest_segments)?;
    let cur = list_curves(bone, PointSource::Live, &bone.segments)?;
    Ok(BoneCurves {
        closed: bone.closed,
        rest,
        cur,
    })
}

impl<'a> AnchorResolver<'a> {
    pub(crate) fn new(bones: &'a [DeformBone]) -> AnchorResolver<'a> {
        let weight_bones = bones
            .iter()
            .map(|b| WeightBone {
                id: b.id.clone(),
                radius: None,
                segments: points_of(&b.segments),
                rest_segments: points_of(&b.rest_segments),
            })
            .collect();
        AnchorResolver {
            bones,
            weight_bones,
            cache: bones.iter().map(|_| None).collect(),
        }
    }

    /// `anchorFor(wentry)`: `Ok(None)` is the baseline's `null` (skip the entry).
    pub(crate) fn resolve(
        &mut self,
        entry: &WeightEntry,
    ) -> Result<Option<ControlAnchor>, DeformError> {
        match anchor_for(&self.weight_bones, entry).map_err(DeformError::Anchor)? {
            Anchor::Control(a) => Ok(Some(a)),
            Anchor::Unresolved => Ok(None),
            Anchor::OffsetModel { offset } => {
                let bone_id = match entry {
                    WeightEntry::Offset(o) => o.bone_id.as_str(),
                    WeightEntry::Control(c) => c.bone_id.as_str(),
                };
                self.offset_anchor(bone_id, offset)
            }
        }
    }

    fn curves(&mut self, index: usize) -> Result<&BoneCurves, DeformError> {
        let bone = &self.bones[index];
        let built = self.cache[index].get_or_insert_with(|| build_curves(bone));
        built.as_ref().map_err(|e| e.clone())
    }

    fn offset_anchor(
        &mut self,
        bone_id: &str,
        offset: Option<f64>,
    ) -> Result<Option<ControlAnchor>, DeformError> {
        let index = self
            .bones
            .iter()
            .position(|b| b.id == bone_id)
            .ok_or(DeformError::Anchor(AnchorError::OffsetBoneMissing))?;
        let bc = self.curves(index)?;
        let Some(offset) = offset else {
            return Ok(None);
        };
        let cur = point_tangent_at(&bc.cur, bc.closed, offset);
        let rest = point_tangent_at(&bc.rest, bc.closed, offset);
        let (Some(cur), Some(rest)) = (cur, rest) else {
            return Ok(None);
        };
        let (Some(ct), Some(rt)) = (cur.tangent, rest.tangent) else {
            return Err(DeformError::TangentUnavailable);
        };
        Ok(Some(ControlAnchor {
            rest_x: rest.point[0],
            rest_y: rest.point[1],
            cur_x: cur.point[0],
            cur_y: cur.point[1],
            da: ct[1].atan2(ct[0]) - rt[1].atan2(rt[0]),
        }))
    }
}
