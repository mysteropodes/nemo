//! Same-query, snapshot-pinned range projection for the actual native layer-bar consumer.
//! Does not require drawing LayerTiming or invent an exposure for no-timing layers.
use crate::object_codec::object_deserialize;
use crate::object_codec::object_frame_edit_range_codec::{RangeError, RangeEvaluator};
use crate::object_snapshot::MAX_MESSAGE_BYTES;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Debug)]
struct RangeSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    view: String,
}
object_deserialize!(RangeSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    view: String
});
fn range_error(error: RangeError) -> DispatchErrorCode {
    match error {
        RangeError::LayerMissing => DispatchErrorCode::NotFound,
        RangeError::FrameOutOfRange => DispatchErrorCode::InvalidRequest,
        _ => DispatchErrorCode::Unavailable,
    }
}
impl ObjectSnapshot {
    pub(crate) fn query_layer_range_json(&self, bytes: &[u8]) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, StaleRevision, Unavailable};
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(InvalidRequest);
        }
        let selector: RangeSelector = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if selector.view != "range"
            || selector.at_revision > 9_007_199_254_740_991
            || selector.document_snapshot_id.is_empty()
            || selector.context_id.is_empty()
            || selector.layer_uid.is_empty()
        {
            return Err(InvalidRequest);
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        // The original ordinary bar caller is root-only. Context providers remain separately OPEN.
        if selector.context_id != "scene-root" {
            return Err(Unavailable);
        }
        let document = self.document();
        let evaluator =
            RangeEvaluator::for_document(document, &selector.context_id, selector.frame)
                .map_err(range_error)?;
        let raw = evaluator
            .resolve(&selector.layer_uid, false)
            .map_err(range_error)?;
        let bounded = evaluator
            .resolve(&selector.layer_uid, true)
            .map_err(range_error)?;
        if [
            raw.in_point,
            raw.out_point,
            bounded.in_point,
            bounded.out_point,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err(Unavailable);
        }
        let layer = document
            .layers
            .iter()
            .find(|l| l.layer_uid == selector.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let range = layer.range.value();
        // Same baseline customization test: explicit zero matters; a naturally shortened out edge also tints.
        let custom = range.is_some_and(|r| {
            r.in_point.value().is_some_and(Option::is_some)
                || r.out_point.value().is_some_and(Option::is_some)
        }) || raw.out_point < f64::from(document.total_frames - 1);
        let linked = range
            .and_then(|r| r.time_link.value())
            .is_some_and(Option::is_some);
        let value = json!({"kind":"layerRange","instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "contextId":selector.context_id,"layerUid":selector.layer_uid,"frame":selector.frame,
            "range":{"rawInPoint":raw.in_point,"rawOutPoint":raw.out_point,"inPoint":bounded.in_point,
                "outPoint":bounded.out_point,"hasCustomRange":custom,"hasTimeLink":linked}});
        if serde_json::to_vec(&value).map_err(|_| Unavailable)?.len() > MAX_MESSAGE_BYTES {
            return Err(Unavailable);
        }
        Ok(value)
    }
}
#[cfg(test)]
#[path = "../tests/object_layer_range_snapshot.rs"]
mod tests;
