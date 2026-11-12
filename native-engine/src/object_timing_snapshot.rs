//! Snapshot-pinned timing read; host lifecycle and public routing are separate gates.
use crate::object_codec::{object_deserialize, FrameScopeKind};
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_snapshot::MAX_MESSAGE_BYTES;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::revision::ObjectSnapshot;
use serde::{Deserialize, Serialize};

#[path = "object_layer_placement_snapshot.rs"]
mod layer_placement;
#[path = "object_layer_cells_snapshot.rs"]
mod layer_cells;
#[path = "object_layer_range_snapshot.rs"]
mod layer_range;

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
    /// The parent dispatcher has admitted the complete envelope and owner.
    /// Reuse the immutable selector; a historical snapshot is never acquired here.
    pub(super) fn dispatch_layer_timing(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let payload =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let selected = match request.payload.get("view") {
            Some(view) if view == "placement" => self.query_layer_placement_json(&payload),
            Some(view) if view == "cells" => self.query_layer_cells_json(&payload),
            Some(_) => self.query_layer_range_json(&payload),
            None => self.query_layer_timing_json(&payload).and_then(|read| {
                serde_json::to_value(read).map_err(|_| DispatchErrorCode::Internal)
            }),
        };
        let read = match selected {
            Ok(read) => read,
            Err(code) => {
                return Ok(self.read_failure(
                    request,
                    code,
                    "Native timing selector is invalid or unavailable.",
                ))
            }
        };
        let response = ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(read),
            error: None,
        };
        if !matches!(
            serde_json::to_vec(&response).map(|value| value.len()),
            Ok(0..=MAX_MESSAGE_BYTES)
        ) {
            return Ok(self.read_failure(
                request,
                DispatchErrorCode::Unavailable,
                "Complete native timing response exceeds 4096 bytes.",
            ));
        }
        Ok(response)
    }

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
        {
            return Err(InvalidRequest);
        }
        // The scene root or a component the document declares (#2256): the entered timeline reads the component's own frames.
        let context = ContextSource::of_document(self.document()).context(&selector.context_id);
        if context.is_ok_and(|c| selector.frame >= c.total_frames()) {
            return Err(InvalidRequest);
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        let context = context.map_err(|_| Unavailable)?;
        let layer = context
            .layers()
            .iter()
            .find(|layer| layer.layer_uid == selector.layer_uid)
            .ok_or(NotFound)?;
        let timing = layer.timing.value().ok_or(Unavailable)?;
        let (cell_kind, source_frame, manual_edit) = timing.exposure(selector.frame);
        let has_content = source_frame.is_some_and(|source| {
            context.objects().iter().any(|object| {
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
