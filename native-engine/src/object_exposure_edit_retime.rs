//! Sealed key-retime transaction (F10): the dense editor's `moveKeyframe` (drag a key to another frame; every
//! later key ripples by the same delta, clamped so the last key stays inside; the in-between spans whose end
//! keys moved are retimed), prepared against one retained snapshot and published through the existing revision
//! owner (src/js/timeline.js `moveKeyframe`, `captureTweenInbetweens`, `retimeTweenSpans` at fbe4b8a3). The
//! per-layer arithmetic and the content move are L05-A's `LayerTiming::retime_key` and the document half
//! (`object_exposure_edit_retime_document.rs`, a child of L05-B's kernel); this file adds the strict payload, the
//! lock policy and the sealed prepare/commit pair. History depth, request replay and dispatch belong to the
//! history owner (`object_exposure_edit_retime_history.rs`) and are not duplicated here.
//!
//! Baseline rules kept on purpose: a drag onto the same frame changes nothing and publishes nothing; a locked
//! layer refuses (typed `Unavailable`, the baseline returns silently); a source that is not a key and a target
//! outside the timeline refuse (`InvalidRequest`, the baseline returns silently); a ripple that clamps to a zero
//! move is a semantic no-op. The consumer port for the motion-arc owner is `PreparedExposureRetime::pairs`: the
//! baseline hands every consecutive key pair, old and new frames, to `rekeyTweenPairData` (src/js/tweens.js),
//! and that bookkeeping has no native owner yet, so this file reports the pairs and rewrites nothing. Markers,
//! in/out points, time links, Motion tracks and component layers (`blankOverride`, I14) have no native records
//! here; a timing of another family or context is refused `Unavailable`, never invented.
use crate::commands::object_fill_owner::object_frame_edit_document::{
    retime_key, FrameEditError, RetimeOutcome,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ObjectDocument, RetimePair};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

const MAX_PAYLOAD_BYTES: usize = 4096;
const COMMAND: &str = "object.layer.key.move";

#[derive(Debug)]
struct RetimePayload {
    command: String,
    layer_uid: String,
    from: u32,
    to: u32,
}
object_deserialize!(RetimePayload {
    command: String,
    layer_uid: String,
    from: u32,
    to: u32
});

fn refusal(error: FrameEditError) -> DispatchErrorCode {
    match error {
        FrameEditError::Invalid => DispatchErrorCode::InvalidRequest,
        FrameEditError::UnknownLayer => DispatchErrorCode::NotFound,
        FrameEditError::Unsupported
        | FrameEditError::MissingTiming
        | FrameEditError::PreservedOutOfRange => DispatchErrorCode::Unavailable,
    }
}

/// The checks in the baseline's own order, then the kernel: the same frame is a no-op before anything else is
/// asked, a locked layer refuses, then range and key-ness (the kernel's `Invalid`).
fn plan(
    document: &ObjectDocument,
    payload: &RetimePayload,
) -> Result<RetimeOutcome, DispatchErrorCode> {
    let layer = payload.layer_uid.as_str();
    let Some(found) = document
        .layers
        .iter()
        .find(|candidate| candidate.layer_uid == layer)
    else {
        return Err(DispatchErrorCode::NotFound);
    };
    if payload.from == payload.to {
        return Ok(RetimeOutcome::NoChange);
    }
    // Absent reads as the baseline `false`; presence is never authored by an edit that does not change it.
    if found.locked.value().copied().unwrap_or(false) {
        return Err(DispatchErrorCode::Unavailable);
    }
    retime_key(document, layer, payload.from, payload.to).map_err(refusal)
}

/// Detached, non-Clone candidate sealed to the snapshot it was prepared from; only this file can build one.
/// `document` is `None` for a semantic no-op, which publishes nothing and keeps redo.
#[derive(Debug)]
pub(crate) struct PreparedExposureRetime {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    applied_to: Option<u32>,
    pairs: Vec<RetimePair>,
}
impl PreparedExposureRetime {
    /// The frame the dragged key lands on after the ripple clamp; `None` for a semantic no-op. Consumer port: the
    /// adapter join reads it, no production caller does yet.
    #[allow(dead_code)]
    pub(crate) fn applied_to(&self) -> Option<u32> {
        self.applied_to
    }
    /// Every consecutive key pair of the old timing with its new frames: the consumer port for the motion-arc
    /// owner's `rekeyTweenPairData`. Empty for a semantic no-op.
    #[allow(dead_code)]
    pub(crate) fn pairs(&self) -> &[RetimePair] {
        self.pairs.as_slice()
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_exposure_retime_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedExposureRetime, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: RetimePayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != COMMAND || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let (document, applied_to, pairs) = match plan(origin.document(), &payload)? {
            RetimeOutcome::Changed(done) => {
                (Some(done.document), Some(done.applied_to), done.pairs)
            }
            RetimeOutcome::NoChange => (None, None, Vec::new()),
        };
        Ok(PreparedExposureRetime {
            origin,
            document,
            applied_to,
            pairs,
        })
    }

    pub(crate) fn commit_exposure_retime(
        &mut self,
        candidate: PreparedExposureRetime,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_exposure_edit_retime.rs"]
mod tests;
