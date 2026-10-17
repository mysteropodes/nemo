//! Read-queries facade for the rig pose evaluation join (I13); no state, no publication authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`,
//! `read-queries` allows `evaluation`), so the commit-pose transaction reaches the one pose evaluation
//! of L03-P (`evaluate_rig_pose`: stored rig + one pinned snapshot + one frame -> detached posed
//! geometry) through the single function below, exactly like the rig edit facade reaches its kernels.
//!
//! The answer is plain numbers and plain classes: the posed vertices of every stroke a surviving bind
//! drives, and one [`PoseBind`] class per bind of the stored rig. Nothing here owns a rig, a mesh store
//! or a deformation: an evaluation that cannot run is a typed [`PoseRefusal`], never a defaulted pose.
//! Without the `evaluation` feature there is no kernel and every call reports
//! [`PoseRefusal::Unavailable`]. The mesh provider (store and raster rect) is not wired here, so a mesh
//! bind is [`PoseBind::Unavailable`] and the caller decides what that means for its transaction.
use crate::revision::ObjectSnapshot;

/// One posed vertex: the anchor and both relative handle vectors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PoseVertex {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

/// The posed geometry of one stroke after every bind that drives it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PoseStroke {
    pub(crate) stroke_id: String,
    pub(crate) closed: bool,
    pub(crate) vertices: Vec<PoseVertex>,
}

/// What the evaluation did with one bind of the stored rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseBind {
    /// Deformed and merged into the output.
    Posed,
    /// The kernel had no live target for it; nothing was written.
    Skipped,
    /// The baseline `relinkRigBinds` would have dropped it (reason kept by the evaluation, not here).
    Unlinked,
    /// A provider the bind needs is not wired (the mesh store or the raster rect); nothing was guessed.
    Unavailable,
}

/// The detached result of one evaluation, pinned to the snapshot it was asked of.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayerPose {
    /// The authored frame the strokes were read from (the held key for a held frame).
    pub(crate) source_frame: u32,
    /// One class per bind of the rig record, in record order.
    pub(crate) binds: Vec<PoseBind>,
    /// Posed strokes in first-bind order; a stroke no surviving bind drives is not listed.
    pub(crate) strokes: Vec<PoseStroke>,
}

/// Why no evaluation was produced at all (nothing partial is ever returned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseRefusal {
    /// The evaluation layer is not part of this build.
    Unavailable,
    FrameOutOfRange,
    LayerAbsent,
    /// The layer exists but never had a rig.
    NoRig,
    /// The layer timing exposes no drawing at this frame.
    NotExposed,
    /// The deformation could not be computed (the baseline `applyRigDeform` threw).
    Deform,
    /// The pin of the snapshot did not match itself (an engine fault, never a caller error).
    Pin,
}

/// Evaluate the rig of `layer_uid` at `frame` against one pinned snapshot.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn evaluate_layer_pose(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
) -> Result<LayerPose, PoseRefusal> {
    #[cfg(feature = "evaluation")]
    {
        evaluate(snapshot, layer_uid, frame)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(PoseRefusal::Unavailable)
    }
}

#[cfg(feature = "evaluation")]
fn evaluate(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
) -> Result<LayerPose, PoseRefusal> {
    use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
        evaluate_rig_pose, BindOutcome, RigPosePin, RigPoseRefusal,
    };
    let pin = RigPosePin::of(snapshot);
    let evaluation = evaluate_rig_pose(snapshot, &pin, layer_uid, frame, None).map_err(
        |refusal| match refusal {
            RigPoseRefusal::FrameOutOfRange => PoseRefusal::FrameOutOfRange,
            RigPoseRefusal::LayerAbsent => PoseRefusal::LayerAbsent,
            RigPoseRefusal::NoRig => PoseRefusal::NoRig,
            RigPoseRefusal::NotExposed => PoseRefusal::NotExposed,
            RigPoseRefusal::Deform { .. } => PoseRefusal::Deform,
            RigPoseRefusal::WrongInstance
            | RigPoseRefusal::WrongDocument
            | RigPoseRefusal::WrongRevision
            | RigPoseRefusal::WrongSnapshot => PoseRefusal::Pin,
        },
    )?;
    let binds = evaluation
        .binds
        .iter()
        .map(|outcome| match outcome {
            BindOutcome::Posed => PoseBind::Posed,
            BindOutcome::Skipped => PoseBind::Skipped,
            BindOutcome::Unlinked(_) => PoseBind::Unlinked,
            BindOutcome::Unavailable(_) => PoseBind::Unavailable,
        })
        .collect();
    let strokes = evaluation
        .strokes
        .iter()
        .map(|stroke| PoseStroke {
            stroke_id: stroke.stroke_id.clone(),
            closed: stroke.closed,
            vertices: stroke
                .segments
                .iter()
                .map(|segment| PoseVertex {
                    point: segment.point,
                    handle_in: segment.handle_in,
                    handle_out: segment.handle_out,
                })
                .collect(),
        })
        .collect();
    Ok(LayerPose {
        source_frame: evaluation.source_frame,
        binds,
        strokes,
    })
}
