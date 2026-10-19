//! Strict preparation/commit for F11 through the sole existing revision owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::SelectionOutcome;
use crate::object_document::{object_deserialize, AssetTarget, ObjectDocument};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_layer_playhead_cut.rs"]
mod plan;
use plan::{plan_cut, CutError, CutPlan};
const MAX_PAYLOAD_BYTES: usize = 4096;
#[derive(Debug)]
struct Payload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
}
object_deserialize!(Payload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32
});

#[derive(Debug, Clone)]
pub(crate) struct PreparedLayerPlayheadCut {
    origin: ObjectSnapshot,
    plan: CutPlan,
}
impl PreparedLayerPlayheadCut {
    pub(crate) fn changed(&self) -> bool {
        self.plan.changed
    }
    pub(crate) fn created(&self) -> Option<&str> {
        self.plan.created.as_deref()
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.plan.document
    }
    pub(crate) fn selection(&self) -> Option<&SelectionOutcome> {
        self.plan.selection.as_ref()
    }
}
fn code(error: CutError) -> DispatchErrorCode {
    match error {
        CutError::InvalidRequest => DispatchErrorCode::InvalidRequest,
        CutError::MissingLayer => DispatchErrorCode::NotFound,
        CutError::Unavailable => DispatchErrorCode::Unavailable,
        CutError::Internal => DispatchErrorCode::Internal,
    }
}
impl ObjectFillOwner {
    pub(crate) fn prepare_layer_playhead_cut_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerPlayheadCut, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: Payload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.cut"
            || payload.layer_uid.is_empty()
            || payload.layer_uid.len() > 128
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if payload.context_id != "scene-root" {
            return Err(DispatchErrorCode::Unavailable);
        }
        let origin = self.current_snapshot();
        let next = origin
            .content_revision()
            .checked_add(1)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let plan =
            plan_cut(origin.document(), next, &payload.layer_uid, payload.frame).map_err(code)?;
        if plan.changed {
            // Linked video/image/font placement is owned by the project envelope,
            // outside ObjectDocument. Drawing-only publish would retain its old
            // target and leave the new half blank. Until an atomic drawing+inventory
            // copy owner is admitted, refuse this source before seal/publication.
            let placed_source = origin
                .project()
                .and_then(|project| project.asset_inventory())
                .is_some_and(|assets| {
                    assets
                        .placements()
                        .iter()
                        .any(|placement| match placement.target() {
                            AssetTarget::Drawing {
                                context_id,
                                layer_uid,
                                ..
                            }
                            | AssetTarget::Text {
                                context_id,
                                layer_uid,
                                ..
                            } => context_id == "scene-root" && layer_uid == &payload.layer_uid,
                            AssetTarget::AudioTrack { .. } => false,
                        })
                });
            if placed_source {
                return Err(DispatchErrorCode::Unavailable);
            }
            ObjectSnapshot::validate_drawing(&plan.document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        Ok(PreparedLayerPlayheadCut { origin, plan })
    }
    pub(crate) fn commit_layer_playhead_cut(
        &mut self,
        candidate: PreparedLayerPlayheadCut,
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
        if !candidate.plan.changed {
            return Ok(current.content_revision());
        }
        self.revisions
            .publish(candidate.plan.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
