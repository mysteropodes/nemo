//! Snapshot pins and frame-query admission of a nested context (L04-J, F14).
//!
//! The pins are the four a native query must carry (instance, document, content
//! revision, snapshot id), checked in the frame packet's order before any lookup.
//! `admit_frame_context` answers whether a frame query is admissible for
//! `scene-root` OR a declared component, against that context's own frame count;
//! it admits only. Producing the frame (component playback, overrides, camera) is
//! the evaluation join (L04-K), so `prepare_object_frame` is NOT routed here: it
//! keeps answering `Unavailable` for every non-root context of a v2 document, which
//! existing tests pin (`object_exposure.rs`, `object_order_evaluation.rs`).
use crate::evaluation::object_component_kernels::object_context_resolver::{
    ContextKind, ContextSource, ResolveError, ResolvedContext, ResolvedTarget,
};
use crate::object_document::{FrameScopeKind, ObjectTarget};
use crate::object_frame_packet::{ObjectFrameError, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;

#[cfg(test)]
#[path = "../tests/object_context_admission.rs"]
mod tests;

/// The four snapshot pins a nested-context query must carry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContextPins<'a> {
    pub(crate) instance_id: &'a str,
    pub(crate) document_id: &'a str,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: &'a str,
}

impl<'a> ContextPins<'a> {
    pub(crate) fn of_snapshot(snapshot: &'a ObjectSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
        }
    }

    /// Same order as the frame packet: instance, document, revision, snapshot.
    pub(crate) fn check(&self, snapshot: &ObjectSnapshot) -> Result<(), ObjectFrameError> {
        if self.instance_id != snapshot.instance_id() {
            return Err(ObjectFrameError::WrongInstance);
        }
        if self.document_id != snapshot.document_id() {
            return Err(ObjectFrameError::WrongDocument);
        }
        if self.content_revision != snapshot.content_revision() {
            return Err(ObjectFrameError::WrongRevision);
        }
        if self.document_snapshot_id != snapshot.snapshot_id() {
            return Err(ObjectFrameError::WrongSnapshot);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotResolveError {
    Pin(ObjectFrameError),
    Resolve(ResolveError),
}

/// Resolves a target inside one pinned snapshot; a wrong pin refuses before any lookup.
pub(crate) fn resolve_in_snapshot<'a>(
    snapshot: &'a ObjectSnapshot,
    pins: &ContextPins<'_>,
    target: &ObjectTarget,
) -> Result<ResolvedTarget<'a>, SnapshotResolveError> {
    pins.check(snapshot).map_err(SnapshotResolveError::Pin)?;
    ContextSource::of_document(snapshot.document())
        .context(&target.context_id)
        .and_then(|context| context.resolve_target(target))
        .map_err(SnapshotResolveError::Resolve)
}

/// A frame query admitted for one context of a pinned snapshot.
#[derive(Clone, Debug)]
pub(crate) struct AdmittedFrame<'a> {
    pub(crate) context: ResolvedContext<'a>,
    pub(crate) scope_kind: FrameScopeKind,
    pub(crate) frame: u32,
}

/// Frame-query admission for `scene-root` AND declared component contexts: the four
/// pins, then the context (`InvalidContext` when undeclared), then the scope (`Unavailable`
/// for a Reference scope in a v2 document or a component, as the packet answers for v2),
/// then the frame against that context's own `totalFrames`. It admits only; producing the
/// frame is the evaluation join's work, so `prepare_object_frame` is not routed here.
pub(crate) fn admit_frame_context<'a>(
    snapshot: &'a ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<AdmittedFrame<'a>, ObjectFrameError> {
    ContextPins {
        instance_id: selector.instance_id,
        document_id: selector.document_id,
        content_revision: selector.content_revision,
        document_snapshot_id: selector.document_snapshot_id,
    }
    .check(snapshot)?;
    let document = snapshot.document();
    let context = ContextSource::of_document(document)
        .context(selector.context_id)
        .map_err(|_| ObjectFrameError::InvalidContext)?;
    let v2 = document.format_version() == 2;
    if (v2 || context.kind() == ContextKind::Component)
        && selector.scope_kind != FrameScopeKind::Authored
    {
        return Err(ObjectFrameError::Unavailable);
    }
    if selector.frame >= context.total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    Ok(AdmittedFrame {
        context,
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
    })
}
