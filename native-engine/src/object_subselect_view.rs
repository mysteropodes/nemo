//! Bounded source-selection read. Only the retained paint plan can create its geometry mapping.
use crate::object_codec::subselect::record::Preview;
use crate::object_codec::{object_deserialize, ObjectTarget, Presence};
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubselectReadItem {
    pub(crate) stable_target: ObjectTarget,
    pub(crate) closed: bool,
    pub(crate) count: usize,
    pub(crate) matrix: [f64; 6],
}
#[derive(Debug, Clone)]
pub struct SubselectReadProjection {
    instance: String,
    document: String,
    revision: u64,
    snapshot: String,
    frame: u32,
    layer: String,
    work: String,
    generation: u64,
    items: Vec<SubselectReadItem>,
    preview: Option<Preview>,
}
impl SubselectReadProjection {
    pub(crate) fn new(
        snapshot: &ObjectSnapshot,
        frame: u32,
        layer: &str,
        items: Vec<SubselectReadItem>,
        preview: Option<Preview>,
    ) -> Self {
        Self {
            instance: snapshot.instance_id().into(),
            document: snapshot.document_id().into(),
            revision: snapshot.content_revision(),
            snapshot: snapshot.snapshot_id().into(),
            frame,
            layer: layer.into(),
            work: String::new(),
            generation: 0,
            items,
            preview,
        }
    }
    pub fn with_presentation(
        mut self,
        work: String,
        generation: u64,
    ) -> Result<Self, DispatchErrorCode> {
        if !crate::object_snapshot::envelope_identifier(&work)
            || !(1..=9_007_199_254_740_991).contains(&generation)
        {
            return Err(DispatchErrorCode::Unavailable);
        }
        self.work = work;
        self.generation = generation;
        Ok(self)
    }
}
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
    layer_uid: String,
    frame: u32,
    work_id: String,
    view_generation: u64,
    cursor: usize,
    limit: usize,
    preview: Presence<Preview>,
}
object_deserialize!(View {
    kind: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    work_id: String,
    view_generation: u64,
    cursor: usize,
    limit: usize,
    #[serde(default)] preview: Presence<Preview>
});
impl ObjectSnapshot {
    pub fn dispatch_with_subselect_view(
        &self,
        request: OpacityRequest,
        projection: Result<&SubselectReadProjection, &str>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let ReadAdmission::Refused(response) = self.read_admission(&request)? {
            return Ok(response);
        }
        let refused = |code, msg| Ok(self.read_failure(&request, code, msg));
        if request.operation != "query.document.object" {
            return refused(
                DispatchErrorCode::Unavailable,
                "Subselect read operation is unavailable",
            );
        }
        let Ok(p) = serde_json::from_value::<Payload>(request.payload.clone()) else {
            return refused(DispatchErrorCode::InvalidRequest, "Invalid subselect view");
        };
        let v = p.view;
        if v.kind != "subselect"
            || v.context_id != "scene-root"
            || v.limit == 0
            || v.limit > 8
            || v.cursor > 1024
        {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Invalid subselect view selector",
            );
        }
        if p.at_revision != self.content_revision() || p.document_snapshot_id != self.snapshot_id()
        {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Subselect view origin changed",
            );
        }
        let projection = match projection {
            Ok(v) => v,
            Err(_) => {
                return refused(
                    DispatchErrorCode::Unavailable,
                    "Subselect requires an available native presentation",
                )
            }
        };
        if projection.instance != self.instance_id()
            || projection.document != self.document_id()
            || projection.revision != p.at_revision
            || projection.snapshot != p.document_snapshot_id
            || projection.frame != v.frame
            || projection.layer != v.layer_uid
            || projection.work != v.work_id
            || projection.generation != v.view_generation
            || projection.work.is_empty()
            || projection.preview.as_ref() != v.preview.value()
        {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Subselect presentation changed",
            );
        }
        if v.cursor > projection.items.len() {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Subselect cursor is outside the selection",
            );
        }
        let end = (v.cursor + v.limit).min(projection.items.len());
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(
                json!({"kind":"subselect","atRevision":p.at_revision,"documentSnapshotId":p.document_snapshot_id,"contextId":v.context_id,"layerUid":v.layer_uid,"frame":v.frame,"workId":v.work_id,"viewGeneration":v.view_generation,"cursor":v.cursor,"total":projection.items.len(),"nextCursor":(end<projection.items.len()).then_some(end),"preview":projection.preview.is_some(),"items":projection.items[v.cursor..end]}),
            ),
        };
        if serde_json::to_vec(&response)
            .map_err(|_| DispatchErrorCode::Internal)?
            .len()
            > MAX_MESSAGE_BYTES
        {
            return refused(
                DispatchErrorCode::Unavailable,
                "Subselect view exceeds the response byte limit",
            );
        }
        Ok(response)
    }
}
