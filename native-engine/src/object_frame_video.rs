//! Video placement selection (T12 / T28.media) from the same immutable source and
//! layer gate as the other families of the frame packet.
//!
//! A `Video` asset placement of the pinned project inventory on a drawing layer of
//! the selected context is selected when that layer is visible and present at the
//! packet frame: exactly the gate a raster item of that layer passes
//! (`source_frames[layer]` and `visible`). The record carries ids only; decoding,
//! source-frame mapping and placement are the consumers'. Placements stay in
//! inventory order, which is deterministic.
use crate::object_document::{AssetKind, AssetTarget, ObjectLayer};
use crate::object_frame_packet::{
    sources, FrameContext, ObjectFrameError, ObjectFramePacket, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;

#[derive(Debug, Clone)]
pub(super) struct VideoIndex {
    layer: usize,
    placement: usize,
}

/// One selected video placement.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectFrameVideoRecord<'a> {
    layer: usize,
    placement: &'a crate::object_document::AssetPlacement,
}
impl<'a> ObjectFrameVideoRecord<'a> {
    pub(crate) fn layer_index(self) -> usize {
        self.layer
    }
    pub(crate) fn placement_id(self) -> &'a str {
        self.placement.id()
    }
    pub(crate) fn asset_id(self) -> &'a str {
        self.placement.asset_id().as_str()
    }
    /// The placement names one element of the layer instead of the whole layer.
    pub(crate) fn element_bound(self) -> bool {
        matches!(
            self.placement.target(),
            AssetTarget::Drawing {
                stroke_id: Some(_),
                ..
            }
        )
    }
}

pub(super) fn select(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    context_layers: &[ObjectLayer],
    source_frames: &[Option<u32>],
) -> Result<Vec<VideoIndex>, ObjectFrameError> {
    let Some(inventory) = snapshot
        .project()
        .and_then(|project| project.asset_inventory())
    else {
        return Ok(Vec::new());
    };
    let mut selected = Vec::new();
    for (placement, record) in inventory.placements().iter().enumerate() {
        let AssetTarget::Drawing {
            context_id,
            layer_uid,
            ..
        } = record.target()
        else {
            continue;
        };
        let video = inventory
            .get(record.asset_id())
            .is_some_and(|asset| asset.kind() == AssetKind::Video);
        if !video || context_id != selector.context_id {
            continue;
        }
        let layer = context_layers
            .iter()
            .position(|layer| layer.layer_uid() == layer_uid)
            .ok_or(ObjectFrameError::InvalidSource)?;
        if source_frames[layer].is_some() && context_layers[layer].visible() {
            selected.push(VideoIndex { layer, placement });
        }
    }
    Ok(selected)
}

/// The drawn video placement ids at `selector` for the media preflight, through
/// the packet's own context admission and layer source gate but without its
/// Motion consumption gate: Motion never changes a layer's presence or
/// visibility, and the motion-bound packet that is actually drawn still refuses
/// whatever it cannot evaluate.
pub(crate) fn drawn_placement_ids(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    exposure: bool,
) -> Result<Vec<String>, ObjectFrameError> {
    let Some(inventory) = snapshot
        .project()
        .and_then(|project| project.asset_inventory())
    else {
        return Ok(Vec::new());
    };
    let context = FrameContext::admit(snapshot, selector, exposure)?;
    let layers = context.layers(snapshot.document());
    let component = context.is_component();
    let sources = sources::select(snapshot, selector, layers, exposure || component, component)?;
    Ok(select(snapshot, selector, layers, &sources)?
        .into_iter()
        .map(|index| inventory.placements()[index.placement].id().to_owned())
        .collect())
}

impl ObjectFramePacket {
    pub(crate) fn video_records(
        &self,
    ) -> impl ExactSizeIterator<Item = ObjectFrameVideoRecord<'_>> + '_ {
        let placements = self
            .snapshot
            .project()
            .and_then(|project| project.asset_inventory())
            .map(|inventory| inventory.placements())
            .unwrap_or(&[]);
        self.video_records
            .iter()
            .map(move |index| ObjectFrameVideoRecord {
                layer: index.layer,
                placement: &placements[index.placement],
            })
    }
}
