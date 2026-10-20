//! T12: same-query, snapshot-pinned projection of one layer's video placement,
//! the facts the ordinary Time Remap and footage interpolation controls need to
//! address the placement edit. Key counts and flags only: the projection stays
//! under the read budget however many remap keys the placement carries.
use crate::object_codec::object_deserialize;
use crate::object_document::{AssetKind, AssetTarget};
use crate::object_snapshot::MAX_MESSAGE_BYTES;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Debug)]
struct PlacementSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    view: String,
}
object_deserialize!(PlacementSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    view: String
});
impl ObjectSnapshot {
    pub(crate) fn query_layer_placement_json(
        &self,
        bytes: &[u8],
    ) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision, Unavailable};
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(InvalidRequest);
        }
        let selector: PlacementSelector =
            serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if selector.view != "placement"
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
        // The ordinary footage controls act on the root scene only, like the range read.
        if selector.context_id != "scene-root" {
            return Err(Unavailable);
        }
        let document = self.document();
        if !document
            .layers
            .iter()
            .any(|l| l.layer_uid == selector.layer_uid)
        {
            return Err(NotFound);
        }
        if selector.frame >= document.total_frames {
            return Err(InvalidRequest);
        }
        let inventory = self.project().and_then(|p| p.asset_inventory());
        let video = inventory.and_then(|inventory| {
            inventory.placements().iter().find(|placement| {
                matches!(placement.target(), AssetTarget::Drawing { context_id, layer_uid, .. }
                    if *context_id == selector.context_id && *layer_uid == selector.layer_uid)
                    && inventory
                        .get(placement.asset_id())
                        .is_some_and(|asset| asset.kind() == AssetKind::Video)
            })
        });
        let placement = video.map(|placement| {
            let timing = placement.timing();
            let at = f64::from(selector.frame);
            let remap = timing.time_remap.as_ref().map(|track| {
                json!({"keyCount":track.keys.len(),
                    "keyAtFrame":track.keys.iter().any(|k| k.frame.as_f64() == Some(at))})
            });
            json!({"placementId":placement.id(),"assetId":placement.asset_id().as_str(),
                "assetKind":"video","offsetFrames":placement.offset_frames(),
                "timeRemap":remap,"interpMode":timing.interp_mode,"frameBlend":timing.frame_blend,
                // The decoder's count: only the desktop host's bound session can fill it.
                "sourceFrameCount":null})
        });
        let value = json!({"kind":"layerPlacement","instanceId":self.instance_id(),
            "documentId":self.document_id(),"atRevision":self.content_revision(),
            "documentSnapshotId":self.snapshot_id(),"contextId":selector.context_id,
            "layerUid":selector.layer_uid,"frame":selector.frame,"placement":placement});
        if serde_json::to_vec(&value).map_err(|_| Unavailable)?.len() > MAX_MESSAGE_BYTES {
            return Err(Unavailable);
        }
        Ok(value)
    }
}
// The fixture opens a real project file through the history owner.
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_layer_placement_snapshot.rs"]
mod tests;
