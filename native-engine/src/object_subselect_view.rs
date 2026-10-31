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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) paint_count: Option<usize>,
    pub(crate) matrix: [f64; 6],
    /// Hit-test listing only (#2362): control-point bounds of the painted geometry, in record space.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bounds: Option<[f64; 4]>,
    /// Hit-test listing only: plain | ribbon | fill (a ribbon's linked fill) | shape | brush.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) role: Option<&'static str>,
    /// Hit-test listing only: the record carries any fill paint (solid or gradient), so an interior click hits it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) filled: Option<bool>,
    /// Hit-test listing only (#2372): the authored stroke width in record space (0 for an unstroked record, absent for an image or
    /// text block); Select pads its reach and hit test with half of it scaled by the item matrix, as the baseline hit the real width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stroke_width: Option<f64>,
    /// Hit-test listing only (#2372): the record's stored `boxAngle` in degrees (the rotation of the frame its selection box sits in,
    /// baseline `serP`); absent when it is 0 or the record has none. Select draws its oriented box from it after a frame change,
    /// an undo or a reload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) box_angle: Option<f64>,
    /// Hit-test listing only: a linked fill's ribbon (same cell, same linkedFillId); Select resolves a fill to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) owner: Option<String>,
    /// Hit-test listing only (#2372): a texture brush's `anchor`, one of its stamped `copy` dabs, or a `bitmap` brush's
    /// anchor or companion image. Select moves an anchor with its copies, selects a copy as its anchor and refuses a bitmap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) brush: Option<&'static str>,
    /// Hit-test listing only: the brush group an anchor, its copies and a bitmap brush's companion share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<String>,
    /// Hit-test listing only (#2372): the style a panel shows for the record (fill, stroke, width, opacity), each member absent when the
    /// record has none; an image lists its opacity and a text block its colour and the colour's alpha.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) style: Option<serde_json::Value>,
    /// Hit-test listing only (#2372): a dynamic rectangle, ellipse or star's declared intent (`kind` and the members its kind owns, never the
    /// generator `box`), the very body `object.parametric.set` takes back. Absent for a record without a `paramShape`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) param_shape: Option<serde_json::Value>,
    /// Hit-test listing only (#2376): the vector groups the record sits in, outermost first (innermost four at most): each `{id, name?}`,
    /// the name absent for a group never renamed. Absent for a loose record. The Elements panel builds its group tree from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group_path: Option<Vec<serde_json::Value>>,
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
    hit_test: bool,
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
            hit_test: false,
        }
    }
    /// A hit-test listing (every painted record, with bounds and role) instead of the editable selection.
    pub(crate) fn with_hit_test(mut self, hit_test: bool) -> Self {
        self.hit_test = hit_test;
        self
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
    hit_test: Presence<bool>,
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
    #[serde(default)] preview: Presence<Preview>,
    #[serde(default)] hit_test: Presence<bool>
});
/// #2256: the scene root or the id of a component context. Only the host knows which context is presented (it compares the id with the
/// presented witness's), so this bounds the shape: a context is an id and never text.
fn bounded_context(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
}
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
        let hit_test = v.hit_test.value() == Some(&true);
        if v.kind != "subselect"
            || (!v.hit_test.missing() && (!hit_test || !v.preview.missing()))
            || !bounded_context(&v.context_id)
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
            Err(reason) => return refused(DispatchErrorCode::Unavailable, reason),
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
            || projection.hit_test != hit_test
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
        // A page holds at most `limit` items and, beyond that, as many as the byte limit allows: full-precision matrices, bounds and
        // long (UUID) ids shorten the page (the cursor then advances by the items actually sent) rather than making the whole
        // selection view unavailable.
        let mut end = (v.cursor + v.limit).min(projection.items.len());
        loop {
            let mut response = ResponseEnvelope {
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
            if hit_test {
                if let Some(result) = response.result.as_mut() {
                    result["hitTest"] = json!(true);
                }
            }
            let size = serde_json::to_vec(&response)
                .map_err(|_| DispatchErrorCode::Internal)?
                .len();
            if size <= MAX_MESSAGE_BYTES {
                return Ok(response);
            }
            if end <= v.cursor + 1 {
                // One item alone is over the limit: nothing smaller can be sent, so this one stays unavailable.
                return refused(
                    DispatchErrorCode::Unavailable,
                    "Subselect view exceeds the response byte limit",
                );
            }
            end -= 1;
        }
    }
}
