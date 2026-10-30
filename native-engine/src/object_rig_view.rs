//! #2255: the bounded, paged rig presentation view of the existing object read (`view.kind` `rig`).
//!
//! The host proves that the request names its actual successful presentation (work, generation, context and frame); the facts
//! themselves come from the immutable snapshot (`object_rig_presentation.rs`). A request names one layer and one part of its
//! rig; `bind` and `vertex` select the vertex of `weights` and are zero for every other part.
use crate::object_codec::object_deserialize;
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde::Deserialize;
use serde_json::json;

struct Payload {
    at_revision: u64,
    document_snapshot_id: String,
    view: View,
}
object_deserialize!(Payload {
    at_revision: u64,
    document_snapshot_id: String,
    view: View
});
struct View {
    kind: String,
    context_id: String,
    frame: u32,
    work_id: String,
    view_generation: u64,
    layer_uid: String,
    part: String,
    bind: usize,
    vertex: usize,
    cursor: usize,
    limit: usize,
}
object_deserialize!(View {
    kind: String,
    context_id: String,
    frame: u32,
    work_id: String,
    view_generation: u64,
    layer_uid: String,
    part: String,
    bind: usize,
    vertex: usize,
    cursor: usize,
    limit: usize
});

impl ObjectSnapshot {
    /// `presented` is the host's proof: `(workId, viewGeneration, contextId, frame)` of its actual presentation.
    pub fn dispatch_with_rig_view(
        &self,
        request: OpacityRequest,
        presented: Result<(&str, u64, &str, u32), &str>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let ReadAdmission::Refused(response) = self.read_admission(&request)? {
            return Ok(response);
        }
        let refused = |code, message| Ok(self.read_failure(&request, code, message));
        let Ok(p) = serde_json::from_value::<Payload>(request.payload.clone()) else {
            return refused(DispatchErrorCode::InvalidRequest, "Invalid rig view");
        };
        let v = p.view;
        let cap = crate::object_snapshot::rig_presentation::PARTS
            .iter()
            .find(|(name, _)| *name == v.part)
            .map(|(_, cap)| *cap);
        let selects = v.part == "weights";
        if request.operation != "query.document.object"
            || v.kind != "rig"
            || v.context_id != "scene-root"
            || v.layer_uid.is_empty()
            || v.layer_uid.len() > 128
            || cap.is_none_or(|cap| v.limit == 0 || v.limit > cap)
            || (!selects && (v.bind != 0 || v.vertex != 0))
            || v.bind > 4_294_967_295
            || v.vertex > 4_294_967_295
            || v.cursor > 100_000
        {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Invalid rig view selector",
            );
        }
        if p.at_revision != self.content_revision() || p.document_snapshot_id != self.snapshot_id()
        {
            return refused(DispatchErrorCode::StaleRevision, "Rig view origin changed");
        }
        let (work, generation, context, frame) = match presented {
            Ok(proof) => proof,
            Err(reason) => return refused(DispatchErrorCode::Unavailable, reason),
        };
        if work != v.work_id
            || generation != v.view_generation
            || context != v.context_id
            || frame != v.frame
        {
            return refused(DispatchErrorCode::StaleRevision, "Rig presentation changed");
        }
        let presented =
            match self.rig_presentation_items(&v.layer_uid, &v.part, v.frame, v.bind, v.vertex) {
                Ok(presented) => presented,
                Err(code) => return refused(code, "Rig presentation is unavailable"),
            };
        let items = presented.items;
        if v.cursor > items.len() {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Rig cursor is outside the list",
            );
        }
        let end = (v.cursor + v.limit).min(items.len());
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(json!({"kind":"rig","atRevision":p.at_revision,
                "documentSnapshotId":p.document_snapshot_id,"contextId":v.context_id,"frame":v.frame,
                "workId":v.work_id,"viewGeneration":v.view_generation,"layerUid":v.layer_uid,
                "part":v.part,"bind":v.bind,"vertex":v.vertex,"cursor":v.cursor,"total":items.len(),
                "nextCursor":(end < items.len()).then_some(end),"unavailable":presented.unavailable,
                "items":&items[v.cursor..end]})),
        };
        if serde_json::to_vec(&response)
            .map_err(|_| DispatchErrorCode::Internal)?
            .len()
            > MAX_MESSAGE_BYTES
        {
            return refused(
                DispatchErrorCode::Unavailable,
                "Rig view exceeds the response byte limit",
            );
        }
        Ok(response)
    }
}
