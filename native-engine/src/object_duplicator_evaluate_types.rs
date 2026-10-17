//! Shared types of the duplicator evaluation join (L04-L, I08/I09): the request,
//! the two ports, the entry shape, the result and the typed errors. No logic.
//!
//! See `object_duplicator_evaluate` for what the join does and in which order.

use crate::evaluation::object_component_kernels::duplicator_placement_apply::{
    ApplyError, DuplicatorCopy, PlacementApply,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    MatrixError, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::PathResolveError;
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::{
    BuildError, SpanBounds,
};
use crate::evaluation::object_component_kernels::object_effector_layers::{
    MotionPort, Unavailable,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::DuplicatorSet;
use crate::object_document::{ObjectLayer, ObjectRecord};

/// The eight Motion properties of a duplicator layer, in the order sampled.
pub(crate) const KEY_OFFSET_POS: &str = "dupOffsetPos";
pub(crate) const KEY_OFFSET_ROT: &str = "dupOffsetRot";
pub(crate) const KEY_OFFSET_SCALE: &str = "dupOffsetScale";
pub(crate) const KEY_OFFSET_OPACITY: &str = "dupOffsetOpacity";
pub(crate) const KEY_OFFSET_HUE: &str = "dupOffsetHue";
pub(crate) const KEY_OFFSET_POS_Z: &str = "dupOffsetPosZ";
pub(crate) const KEY_OFFSET_ROT_X: &str = "dupOffsetRotX";
pub(crate) const KEY_OFFSET_ROT_Y: &str = "dupOffsetRotY";

/// How the baseline reaches `applyLayerDuplicator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    /// `getEffectiveStrokesRendered`: the layer lookup resamples; an editing
    /// source and an empty (non Object-mode) host return the base untouched.
    Rendered { edit_source: bool },
    /// A direct call (a component's own sub-layer, StoryBoard): no guards;
    /// `can_resample` is `opts.resample` being supplied.
    Direct { can_resample: bool },
}

/// The content of other layers of the context (`getEffectiveStrokes`).
pub(crate) trait LayerSources {
    /// `findLayerIndexByUid(uid) >= 0`.
    fn has_layer(&self, uid: &str) -> bool;
    /// `getEffectiveStrokes(layer, frame)`: empty when nothing is drawn there.
    fn strokes_at(&self, uid: &str, frame: f64) -> Vec<ObjectRecord>;
}

/// What the evaluation reaches outside the document.
#[derive(Clone, Copy)]
pub(crate) struct Ports<'a> {
    pub motion: &'a dyn MotionPort,
    pub sources: &'a dyn LayerSources,
}

/// One duplicator layer at one frame.
#[derive(Clone, Copy)]
pub(crate) struct Request<'a> {
    pub context_id: &'a str,
    pub host_uid: &'a str,
    pub set: &'a DuplicatorSet,
    /// The context's layers in layer order (effector layers are among them).
    pub layers: &'a [ObjectLayer],
    pub entry: Entry,
    /// `!!ld.threeD`.
    pub is_3d_layer: bool,
    /// The host's own content at the frame (`getEffectiveStrokes(host, frame)`).
    pub base: &'a [ObjectRecord],
    pub frame_idx: f64,
    pub canvas_w: f64,
    pub canvas_h: f64,
    /// The seed layer's span for the temporal stagger.
    pub span: SpanBounds,
}

/// Why the base came back as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnchangedReason {
    /// The host carries no duplicator record.
    NoDuplicator,
    /// `ld._dupEditSource`: the source shapes are being edited.
    EditingSource,
    /// Shape mode with nothing drawn.
    EmptyBase,
    /// `_duplicatorCount <= 1`.
    SingleClone,
    /// Object mode where every referenced layer is gone or is a duplicator.
    EmptyPool,
    /// Path mode whose path layer is unset, unresolved, a duplicator or unusable.
    BrokenPath,
}

/// One clone's placement as the loop asked for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClonePlacementTrace {
    pub k: u32,
    pub pivot: Pivot,
    pub placement: PlacementApply,
}

/// The copies of a duplicator, with the placement of every clone.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CopySet {
    /// `_duplicatorCount(dup)` (possibly fractional).
    pub count: f64,
    pub copies: Vec<DuplicatorCopy>,
    pub placements: Vec<ClonePlacementTrace>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Evaluation {
    Unchanged(UnchangedReason),
    Copies(CopySet),
}

/// Where the baseline would raise, or a provider is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvaluateError {
    Unavailable(Unavailable),
    /// `valueAtFrame(...)` was `null` for a host property whose `[0]` is read.
    NullValue(&'static str),
    Pivot(MatrixError),
    Path(PathResolveError),
    /// A mode other than grid, radial and path reached the path branch with no path.
    PathInfoRequired,
    /// A pool member could not be recentred.
    Pool(MatrixError),
    Build(BuildError),
    Copy {
        index: u32,
        error: ApplyError,
    },
}
