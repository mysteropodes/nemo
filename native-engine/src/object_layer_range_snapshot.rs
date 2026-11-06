//! Same-query, snapshot-pinned range projection for the actual native layer-bar consumer.
//! Does not require drawing LayerTiming or invent an exposure for no-timing layers.
use crate::object_codec::object_deserialize;
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
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
/// Bytes each optional snap list may take of the 4 KiB read; a longer list is cut at the last entry that fits.
const SNAP_LIST_BYTES: usize = 1536;
const MAX_MARKER_NAME: usize = 32;
/// The layer's own markers (`ld.markers`, frame order) as `{frame, name?}`, the baseline snap targets "Repère calque".
fn marker_list(layer: &crate::object_document::ObjectLayer, total: u32) -> Vec<Value> {
    let mut list: Vec<(i64, Option<String>)> = layer
        .range
        .value()
        .and_then(|r| r.markers.value())
        .map(|markers| {
            markers
                .iter()
                .filter(|m| m.frame >= 0 && m.frame < i64::from(total))
                .map(|m| {
                    let name = m.name.value().filter(|n| !n.is_empty());
                    (
                        m.frame,
                        name.map(|n| n.chars().take(MAX_MARKER_NAME).collect()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    list.sort_by_key(|(frame, _)| *frame);
    let mut bytes = 0;
    let mut out = Vec::new();
    for (frame, name) in list {
        let entry = match name {
            Some(name) => json!({"frame":frame,"name":name}),
            None => json!({"frame":frame}),
        };
        bytes += entry.to_string().len() + 1;
        if bytes > SNAP_LIST_BYTES {
            break;
        }
        out.push(entry);
    }
    out
}
/// Every frame carrying a key on any parameter track of the layer's effect stack, ascending and unique.
fn effect_key_frames(layer: &crate::object_document::ObjectLayer, total: u32) -> Vec<i64> {
    let mut frames: Vec<i64> = layer
        .effects
        .value()
        .into_iter()
        .flatten()
        .filter_map(|effect| effect.keys.value().and_then(Option::as_ref))
        .flat_map(|tracks| {
            tracks
                .entries()
                .iter()
                .filter_map(|(_, track)| track.as_ref())
        })
        .flat_map(|track| track.keys.iter())
        .filter_map(|key| key.frame.as_f64().filter(|f| f.fract() == 0.0))
        .map(|f| f as i64)
        .filter(|f| *f >= 0 && *f < i64::from(total))
        .collect();
    frames.sort_unstable();
    frames.dedup();
    let mut bytes = 0;
    frames.retain(|f| {
        bytes += f.to_string().len() + 1;
        bytes <= SNAP_LIST_BYTES
    });
    frames
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
        // The scene root or a declared component (#2256): the evaluator reads that context's own layers and records.
        let document = self.document();
        let context = ContextSource::of_document(document)
            .context(&selector.context_id)
            .map_err(|_| Unavailable)?;
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
        let layer = context
            .layers()
            .iter()
            .find(|l| l.layer_uid == selector.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let range = layer.range.value();
        // Same baseline customization test: explicit zero matters; a naturally shortened out edge also tints.
        let custom = range.is_some_and(|r| {
            r.in_point.value().is_some_and(Option::is_some)
                || r.out_point.value().is_some_and(Option::is_some)
        }) || raw.out_point < f64::from(context.total_frames() - 1);
        let linked = range
            .and_then(|r| r.time_link.value())
            .is_some_and(Option::is_some);
        let mut value = json!({"kind":"layerRange","instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "contextId":selector.context_id,"layerUid":selector.layer_uid,"frame":selector.frame,
            "range":{"rawInPoint":raw.in_point,"rawOutPoint":raw.out_point,"inPoint":bounded.in_point,
                "outPoint":bounded.out_point,"hasCustomRange":custom,"hasTimeLink":linked}});
        // The snap targets of the baseline `snapMotionFrame`, only when the layer has any (a plain layer keeps the six-field range).
        let markers = marker_list(layer, context.total_frames());
        if !markers.is_empty() {
            value["range"]["markers"] = Value::Array(markers);
        }
        let effect_keys = effect_key_frames(layer, context.total_frames());
        if !effect_keys.is_empty() {
            value["range"]["effectKeys"] = json!(effect_keys);
        }
        if serde_json::to_vec(&value).map_err(|_| Unavailable)?.len() > MAX_MESSAGE_BYTES {
            return Err(Unavailable);
        }
        Ok(value)
    }
}
#[cfg(test)]
#[path = "../tests/object_layer_range_snapshot.rs"]
mod tests;
