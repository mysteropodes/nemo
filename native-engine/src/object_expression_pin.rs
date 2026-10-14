//! Private, read-only request pins for a future native Order expression runtime.
//! This module neither executes code nor publishes an evaluated value.
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
#[cfg(feature = "object_expression_runtime")]
use crate::object_expression_runtime::{DetachedCandidate, ExpressionRefusal};
use crate::object_frame_packet::{
    raw_expression_numeric, raw_expression_order, ObjectFrameError, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;

pub(crate) struct ExpressionSelector<'a> {
    pub(crate) frame: ObjectFrameSelector<'a>,
    pub(crate) source_frame: u32,
    pub(crate) layer_uid: &'a str,
    /// None selects layer Order; Some selects one actual scoped stroke's Order.
    pub(crate) stroke_id: Option<&'a str>,
}

/// Its snapshot is retained for read-only provenance, never a writable owner.
pub(crate) struct ExpressionRequest {
    snapshot: ObjectSnapshot,
    context_id: String,
    scope_kind: FrameScopeKind,
    source_frame: u32,
    evaluated_frame: u32,
    layer_uid: String,
    stroke_id: Option<String>,
    code: String,
    raw_numeric: f64,
}

impl ExpressionRequest {
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    pub(crate) fn context_id(&self) -> &str {
        &self.context_id
    }
    pub(crate) fn scope_kind(&self) -> &FrameScopeKind {
        &self.scope_kind
    }
    pub(crate) fn source_frame(&self) -> u32 {
        self.source_frame
    }
    pub(crate) fn evaluated_frame(&self) -> u32 {
        self.evaluated_frame
    }
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub(crate) fn stroke_id(&self) -> Option<&str> {
        self.stroke_id.as_deref()
    }
    pub(crate) fn code(&self) -> &str {
        &self.code
    }
    pub(crate) fn raw_numeric(&self) -> f64 {
        self.raw_numeric
    }

    /// Mandatory final check immediately before a future runtime publishes.
    /// A retained old snapshot is never authority for the current owner head.
    pub(crate) fn revalidate(&self, owner: &NativeObjectHistory) -> Result<(), ObjectFrameError> {
        let current = owner
            .acquire_snapshot(owner.content_revision())
            .ok_or(ObjectFrameError::Unavailable)?;
        verify_owner_snapshot(owner, &current, &self.snapshot)
    }

    /// Only this finalizer can extract a scalar from a fully retired request.
    /// A real owner advance during engine work invalidates even a finite result.
    #[cfg(feature = "object_expression_runtime")]
    pub(crate) fn finalize(
        self,
        owner: &NativeObjectHistory,
        candidate: DetachedCandidate,
    ) -> Result<f64, ExpressionRefusal> {
        self.revalidate(owner)?;
        candidate.into_outcome()
    }
}

pub(super) fn verify_owner_snapshot(
    owner: &NativeObjectHistory,
    current: &ObjectSnapshot,
    supplied: &ObjectSnapshot,
) -> Result<(), ObjectFrameError> {
    if supplied.instance_id() != owner.instance_id() {
        return Err(ObjectFrameError::WrongInstance);
    }
    if supplied.document_id() != owner.document_id() {
        return Err(ObjectFrameError::WrongDocument);
    }
    if supplied.content_revision() != owner.content_revision() {
        return Err(ObjectFrameError::WrongRevision);
    }
    if supplied.snapshot_id() != current.snapshot_id()
        || !std::ptr::eq(supplied.document(), current.document())
    {
        return Err(ObjectFrameError::WrongSnapshot);
    }
    Ok(())
}

pub(crate) fn prepare_expression_request(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ExpressionSelector<'_>,
) -> Result<ExpressionRequest, ObjectFrameError> {
    let current = owner
        .acquire_snapshot(owner.content_revision())
        .ok_or(ObjectFrameError::Unavailable)?;
    verify_owner_snapshot(owner, &current, snapshot)?;
    let frame = &selector.frame;
    if frame.instance_id != snapshot.instance_id() {
        return Err(ObjectFrameError::WrongInstance);
    }
    if frame.document_id != snapshot.document_id() {
        return Err(ObjectFrameError::WrongDocument);
    }
    if frame.content_revision != snapshot.content_revision() {
        return Err(ObjectFrameError::WrongRevision);
    }
    if frame.document_snapshot_id != snapshot.snapshot_id() {
        return Err(ObjectFrameError::WrongSnapshot);
    }
    if frame.frame >= snapshot.document().total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    let order = raw_expression_order(
        snapshot,
        frame.context_id,
        &frame.scope_kind,
        selector.source_frame,
        selector.layer_uid,
        selector.stroke_id,
    )?;
    let expression = order
        .expression
        .value()
        .filter(|expression| expression.enabled && !expression.code.is_empty())
        .ok_or(ObjectFrameError::Unavailable)?;
    let raw_numeric = raw_expression_numeric(order, frame.frame)?;
    Ok(ExpressionRequest {
        snapshot: snapshot.clone(),
        context_id: frame.context_id.to_owned(),
        scope_kind: frame.scope_kind.clone(),
        source_frame: selector.source_frame,
        evaluated_frame: frame.frame,
        layer_uid: selector.layer_uid.to_owned(),
        stroke_id: selector.stroke_id.map(str::to_owned),
        code: expression.code.clone(),
        raw_numeric,
    })
}
