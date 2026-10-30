//! Pure layer-batch geometry projection plus an explicitly selected stored-camera transform.
//! Root's common producer owns snapshot/source selection and captured evaluated poses.
//! No evaluator, raw-track fallback, UI state, paint writer or retained MotionPort exists here.
use crate::evaluation::motion_parent_chain::{motion_mat_from_values, transform_segments, Segment};
use crate::evaluation::object_vector_kernels::render_vector_eval::{EvalSegment, EvaluatedVector};
use crate::evaluation::property_projection_3d::{
    make_projector, project_segments, ExtraDelta, Pose3D, Seg,
};
use crate::object_frame_packet::{
    FrameScopeKind, MotionValues, ObjectFrameError, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;

#[path = "object_frame_projection_geometry.rs"]
mod geometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionError {
    Frame(ObjectFrameError),
    UnsupportedContext,
    InvalidCanvas,
    NonFinite,
    MismatchedExtras,
    CameraUnavailable,
    FrameMismatch,
}

fn canvas_valid(canvas: [f64; 2]) -> Result<(), ProjectionError> {
    if canvas.iter().all(|n| n.is_finite() && *n > 0.) {
        Ok(())
    } else {
        Err(ProjectionError::InvalidCanvas)
    }
}

/// The same true cubic union used by own-layer Motion and ancestor pivots.
pub(crate) fn layer_bounds(
    vectors: &[EvaluatedVector],
) -> Result<Option<crate::evaluation::property_projection_3d::Bounds>, ProjectionError> {
    geometry::bounds(vectors)
}

/// The exact own-layer transform, shared by paint and immutable edit projections.
pub(crate) fn layer_chain_entry(
    vectors: &[EvaluatedVector],
    motion: &MotionValues,
) -> Result<Option<crate::evaluation::motion_parent_chain::ChainEntry>, ProjectionError> {
    if motion
        .position
        .iter()
        .chain(&motion.anchor)
        .chain(&motion.scale)
        .chain([&motion.rotation, &motion.opacity])
        .any(|n| !n.is_finite())
    {
        return Err(ProjectionError::NonFinite);
    }
    let bounds = geometry::bounds(vectors)?;
    let Some(mat) = motion_mat_from_values(
        motion.position,
        motion.anchor,
        motion.rotation,
        motion.scale,
        motion.opacity,
    ) else {
        return Ok(None);
    };
    let Some(bounds) = bounds else {
        return Ok(None);
    };
    let pivot = [
        bounds.x + bounds.width / 2. + mat.ax,
        bounds.y + bounds.height / 2. + mat.ay,
    ];
    if pivot.iter().any(|n| !n.is_finite()) {
        return Err(ProjectionError::NonFinite);
    }
    Ok(Some(crate::evaluation::motion_parent_chain::ChainEntry {
        mat,
        pivot,
    }))
}

/// Whole-layer Motion, after the caller's existing pathFX/rebuild/element stages.
/// Opacity and stroke-scale paint semantics remain with the actual paint producer.
pub(crate) fn transform_layer_2d(
    vectors: &[EvaluatedVector],
    motion: &MotionValues,
) -> Result<Vec<EvaluatedVector>, ProjectionError> {
    let entry = layer_chain_entry(vectors, motion)?;
    apply_layer_entry(vectors, entry)
}

/// The geometry half of `transform_layer_2d`, for a caller that also needs the entry itself (paint coordinates).
pub(crate) fn apply_layer_entry(
    vectors: &[EvaluatedVector],
    entry: Option<crate::evaluation::motion_parent_chain::ChainEntry>,
) -> Result<Vec<EvaluatedVector>, ProjectionError> {
    let Some(entry) = entry else {
        return Ok(vectors.to_vec());
    };
    let mat = entry.mat;
    let pivot = entry.pivot;
    vectors
        .iter()
        .map(|vector| {
            let input: Vec<_> = vector
                .segments
                .iter()
                .map(|s| Segment {
                    point: s.point,
                    handle_in: Some(s.handle_in),
                    handle_out: Some(s.handle_out),
                })
                .collect();
            let result = EvaluatedVector {
                source_object_index: vector.source_object_index,
                closed: vector.closed,
                segments: transform_segments(&input, pivot, &mat)
                    .into_iter()
                    .map(|s| EvalSegment {
                        point: s.point,
                        handle_in: s.handle_in.unwrap_or([0., 0.]),
                        handle_out: s.handle_out.unwrap_or([0., 0.]),
                    })
                    .collect(),
            };
            geometry::finite(&result)?;
            Ok(result)
        })
        .collect()
}

/// ThreeD REPLACES layer 2D Motion: Pose3D already contains position/anchor/rotation/scale.
/// `extras` is one exact placement/parent contribution per vector, in the same encounter order.
/// The shared bound includes every supplied actual layer shape, not each clone/record separately.
pub(crate) fn project_layer_3d(
    vectors: &[EvaluatedVector],
    pose: &Pose3D,
    canvas: [f64; 2],
    extras: &[Option<ExtraDelta>],
) -> Result<Vec<EvaluatedVector>, ProjectionError> {
    canvas_valid(canvas)?;
    if extras.len() != vectors.len() {
        return Err(ProjectionError::MismatchedExtras);
    }
    if pose
        .position
        .iter()
        .chain(&pose.anchor)
        .chain(&pose.scale)
        .chain([
            &pose.rotation,
            &pose.position_z,
            &pose.rotation_x,
            &pose.rotation_y,
        ])
        .any(|n| !n.is_finite())
        || extras.iter().flatten().any(|e| {
            [e.dx, e.dy, e.drot, e.dsx_pct, e.dsy_pct, e.dz, e.drx, e.dry]
                .into_iter()
                .flatten()
                .any(|n| !n.is_finite())
        })
    {
        return Err(ProjectionError::NonFinite);
    }
    let Some(bounds) = geometry::bounds(vectors)? else {
        return Ok(vectors.to_vec());
    };
    vectors
        .iter()
        .zip(extras)
        .map(|(vector, extra)| {
            let projector = make_projector(pose, &bounds, canvas[0], canvas[1], extra.as_ref());
            let segments: Vec<_> = vector
                .segments
                .iter()
                .map(|s| Seg {
                    point: s.point,
                    handle_in: Some(s.handle_in),
                    handle_out: Some(s.handle_out),
                })
                .collect();
            let result = EvaluatedVector {
                source_object_index: vector.source_object_index,
                closed: vector.closed,
                segments: project_segments(&segments, &projector)
                    .into_iter()
                    .map(|s| EvalSegment {
                        point: s.point,
                        handle_in: s.handle_in.unwrap_or([0., 0.]),
                        handle_out: s.handle_out.unwrap_or([0., 0.]),
                    })
                    .collect(),
            };
            geometry::finite(&result)?;
            Ok(result)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CameraView {
    /// Stored camera is dormant in an unlocked canvas session.
    Canvas,
    /// Export, or an explicitly camera-locked viewport. Never persisted as cameraView.
    StoredCamera,
}

#[derive(Debug, Clone)]
pub(crate) struct CameraProjection {
    source: ObjectSnapshot,
    frame: u32,
    matrix: Option<[f64; 6]>,
}

fn verify(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<(), ProjectionError> {
    let error = if selector.instance_id != snapshot.instance_id() {
        Some(ObjectFrameError::WrongInstance)
    } else if selector.document_id != snapshot.document_id() {
        Some(ObjectFrameError::WrongDocument)
    } else if selector.content_revision != snapshot.content_revision() {
        Some(ObjectFrameError::WrongRevision)
    } else if selector.document_snapshot_id != snapshot.snapshot_id() {
        Some(ObjectFrameError::WrongSnapshot)
    } else if selector.frame >= snapshot.document().total_frames() {
        Some(ObjectFrameError::FrameOutOfRange)
    } else {
        None
    };
    if let Some(error) = error {
        return Err(ProjectionError::Frame(error));
    }
    if selector.context_id != "scene-root" || selector.scope_kind != FrameScopeKind::Authored {
        return Err(ProjectionError::UnsupportedContext);
    }
    Ok(())
}

impl CameraProjection {
    pub(crate) fn for_snapshot(
        snapshot: &ObjectSnapshot,
        selector: &ObjectFrameSelector<'_>,
        view: CameraView,
    ) -> Result<Self, ProjectionError> {
        verify(snapshot, selector)?;
        let matrix = match view {
            CameraView::Canvas => None,
            CameraView::StoredCamera => snapshot
                .sampled_camera_matrix(f64::from(selector.frame))
                .map_err(|_| ProjectionError::CameraUnavailable)?,
        };
        Ok(Self {
            source: snapshot.clone(),
            frame: selector.frame,
            matrix,
        })
    }

    /// Retain for the actual whole-paint affine/clip consumer. Geometry alone does not
    /// implement camera stroke/fill/effect scaling or canvas clipping.
    pub(crate) fn matrix(&self) -> Option<[f64; 6]> {
        self.matrix
    }

    pub(crate) fn apply_vectors(
        &self,
        snapshot: &ObjectSnapshot,
        selector: &ObjectFrameSelector<'_>,
        vectors: &[EvaluatedVector],
    ) -> Result<Vec<EvaluatedVector>, ProjectionError> {
        verify(snapshot, selector)?;
        if self.source.instance_id() != snapshot.instance_id() {
            return Err(ProjectionError::Frame(ObjectFrameError::WrongInstance));
        }
        if self.source.document_id() != snapshot.document_id() {
            return Err(ProjectionError::Frame(ObjectFrameError::WrongDocument));
        }
        if self.source.content_revision() != snapshot.content_revision() {
            return Err(ProjectionError::Frame(ObjectFrameError::WrongRevision));
        }
        if self.source.snapshot_id() != snapshot.snapshot_id()
            || !std::ptr::eq(self.source.document(), snapshot.document())
        {
            return Err(ProjectionError::Frame(ObjectFrameError::WrongSnapshot));
        }
        if self.frame != selector.frame {
            return Err(ProjectionError::FrameMismatch);
        }
        for vector in vectors {
            geometry::finite(vector)?;
        }
        let Some([a, b, c, d, tx, ty]) = self.matrix else {
            return Ok(vectors.to_vec());
        };
        vectors
            .iter()
            .map(|vector| {
                geometry::mapped(vector, |p| {
                    [a * p[0] + c * p[1] + tx, b * p[0] + d * p[1] + ty]
                })
            })
            .collect()
    }
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_frame_projection.rs"]
mod tests;
