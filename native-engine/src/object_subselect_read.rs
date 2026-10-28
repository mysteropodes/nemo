//! Paged immutable subselect geometry and detached preview over the existing object read route.
use crate::object_codec::subselect::{record, Anchor, Intent, Refusal};
use crate::object_codec::{object_deserialize, ObjectTarget, Presence};
use crate::object_snapshot::{ObjectSnapshot, MAX_MESSAGE_BYTES};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde::Deserialize;
use serde_json::json;
struct Payload {
    at_revision: u64,
    document_snapshot_id: String,
    stable_target: ObjectTarget,
    subselect: Page,
}
object_deserialize!(Payload {
    at_revision: u64,
    document_snapshot_id: String,
    stable_target: ObjectTarget,
    subselect: Page
});
struct Page {
    cursor: usize,
    limit: usize,
    preview: Presence<Intent>,
    paint: Presence<bool>,
}
object_deserialize!(Page {cursor:usize,limit:usize,#[serde(default)] preview:Presence<Intent>,#[serde(default)] paint:Presence<bool>});
impl ObjectSnapshot {
    pub(super) fn dispatch_subselect_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let refused = |code, message| Ok(self.read_failure(request, code, message));
        let payload: Payload = match serde_json::from_value(request.payload.clone()) {
            Ok(p) => p,
            Err(_) => {
                return refused(
                    DispatchErrorCode::InvalidRequest,
                    "Invalid subselect read selector.",
                )
            }
        };
        if payload.at_revision != self.content_revision()
            || payload.document_snapshot_id != self.snapshot_id()
        {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Subselect read origin changed.",
            );
        }
        if payload.subselect.limit == 0 || payload.subselect.limit > 16 {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Subselect page limit is outside 1..16.",
            );
        }
        let paint = !payload.subselect.paint.missing();
        if paint && payload.subselect.paint.value() != Some(&true) {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Invalid subselect paint selector.",
            );
        }
        let Some(stored) = self
            .document()
            .objects
            .iter()
            .find(|r| r.target == payload.stable_target)
        else {
            return refused(DispatchErrorCode::NotFound, "Subselect target is absent.");
        };
        let planned = record::admit(self.document(), stored).and_then(|_| {
            match payload.subselect.preview.value() {
                Some(intent) => record::plan(stored, intent),
                None => Ok(stored.clone()),
            }
        });
        let geometry = planned.and_then(|r| {
            let anchors = record::anchors(&r)?;
            if paint {
                if !record::has_pressure(&r) {
                    return Err(Refusal::Unsupported);
                }
                Ok((r.geometry.closed, record::paint_anchors(&r)?))
            } else {
                Ok((record::closed(&r), anchors))
            }
        });
        let (closed, anchors): (bool, Vec<Anchor>) = match geometry {
            Ok(v) => v,
            Err(Refusal::Invalid) => {
                return refused(
                    DispatchErrorCode::InvalidRequest,
                    "Invalid subselect geometry intent.",
                )
            }
            Err(_) => {
                return refused(
                    DispatchErrorCode::Unavailable,
                    "Subselect geometry consumer is unavailable.",
                )
            }
        };
        let cursor = payload.subselect.cursor;
        if cursor > anchors.len() {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Subselect cursor is outside the geometry.",
            );
        }
        let end = (cursor + payload.subselect.limit).min(anchors.len());
        let mut response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(
                json!({"atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),"subselect":{
                    "stableTarget":payload.stable_target,"closed":closed,"total":anchors.len(),"cursor":cursor,
                    "nextCursor":(end<anchors.len()).then_some(end),"segments":anchors[cursor..end],"preview":!payload.subselect.preview.missing(),
                }}),
            ),
        };
        if paint {
            response.result.as_mut().unwrap()["subselect"]["paint"] = json!(true);
        }
        if serde_json::to_vec(&response)
            .map_err(|_| DispatchErrorCode::Internal)?
            .len()
            > MAX_MESSAGE_BYTES
        {
            return refused(
                DispatchErrorCode::Unavailable,
                "Subselect response exceeds the existing byte limit.",
            );
        }
        Ok(response)
    }
}

// Its support module (object_vector_edit_support.rs) drives the real history owner.
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_subselect_read.rs"]
mod tests;
