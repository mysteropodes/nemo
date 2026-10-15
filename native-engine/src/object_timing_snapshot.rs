//! Snapshot-pinned timing read; host lifecycle and public routing are separate gates.
use super::ObjectSnapshot;
use crate::object_document::{object_deserialize, FrameScopeKind};
use crate::request_receipts::DispatchErrorCode;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct TimingSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
}
object_deserialize!(TimingSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerTimingReadResponse {
    pub instance_id: String,
    pub document_id: String,
    pub at_revision: u64,
    pub document_snapshot_id: String,
    pub context_id: String,
    pub layer_uid: String,
    pub frame: u32,
    pub cell_kind: &'static str,
    pub source_frame: Option<u32>,
    pub has_content: bool,
    pub manual_edit: bool,
}
impl ObjectSnapshot {
    /// Original-byte core selector. A missing timing model is not a blank key.
    /// The caller holds this immutable view; this grants no live UI epoch or API availability.
    pub fn query_layer_timing_json(
        &self,
        bytes: &[u8],
    ) -> Result<LayerTimingReadResponse, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision, Unavailable};
        if bytes.len() > 4096 {
            return Err(InvalidRequest);
        }
        let selector: TimingSelector = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if selector.at_revision > 9_007_199_254_740_991
            || selector.document_snapshot_id.is_empty()
            || selector.context_id.is_empty()
            || selector.layer_uid.is_empty()
            || selector.frame >= self.document().total_frames
        {
            return Err(InvalidRequest);
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        if selector.context_id != "scene-root" {
            return Err(Unavailable);
        }
        let layer = self
            .document()
            .layers
            .iter()
            .find(|layer| layer.layer_uid == selector.layer_uid)
            .ok_or(NotFound)?;
        let timing = layer.timing.value().ok_or(Unavailable)?;
        let (cell_kind, source_frame, manual_edit) = timing.exposure(selector.frame);
        let has_content = source_frame.is_some_and(|source| {
            self.document().objects.iter().any(|object| {
                object.target.context_id == selector.context_id
                    && object.target.layer_uid == selector.layer_uid
                    && object.target.frame_scope.kind == FrameScopeKind::Authored
                    && object.target.frame_scope.frame == source
            })
        });
        let response = LayerTimingReadResponse {
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            at_revision: self.content_revision(),
            document_snapshot_id: self.snapshot_id().into(),
            context_id: selector.context_id,
            layer_uid: selector.layer_uid,
            frame: selector.frame,
            cell_kind,
            source_frame,
            has_content,
            manual_edit,
        };
        if serde_json::to_vec(&response)
            .map_err(|_| Unavailable)?
            .len()
            > 4096
        {
            return Err(Unavailable);
        }
        Ok(response)
    }
}
