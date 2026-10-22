//! T12: preparation of one linked video record and its drawing placement, sealed to the
//! existing owner. The host probes the canonical file and supplies its facts; the engine
//! never opens a decoder, reads the file or keeps a second document owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::media_timing::object_media_link_edit::{
    check_inventory, MAX_ASSET_RECORDS, MAX_PLACEMENTS,
};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetSource, AssetTarget,
    ProjectEnvelope,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::json;
use std::sync::Arc;

#[path = "object_media_video_link_wire.rs"]
pub(crate) mod wire;
use wire::{VideoIntent, VideoLink, MAX_PATH_BYTES};

/// What the host's probe-then-close established about the canonical file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedVideoFacts {
    /// The canonical path: the stored locator and the deduplication identity.
    pub locator: String,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VideoLinkResult {
    pub(crate) asset_id: String,
    pub(crate) placement_id: String,
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) new_layer: bool,
    pub(crate) facts: LinkedVideoFacts,
}
impl VideoLinkResult {
    pub(crate) fn result(&self) -> serde_json::Value {
        json!({"applied":true,"historyEntriesAdded":1,"assetId":self.asset_id,
            "placementId":self.placement_id,
            "target":{"contextId":self.context_id,"layerUid":self.layer_uid},
            "newLayer":self.new_layer,"sourceWidth":self.facts.width,
            "sourceHeight":self.facts.height,"sourceFrameCount":self.facts.frame_count})
    }
}

#[derive(Debug)]
pub(crate) struct PreparedVideoLink {
    origin: ObjectSnapshot,
    project: ProjectEnvelope,
    result: VideoLinkResult,
}
impl PreparedVideoLink {
    pub(crate) fn result(&self) -> &VideoLinkResult {
        &self.result
    }
}

fn refusal() -> DispatchErrorCode {
    DispatchErrorCode::Unavailable
}
/// File basename without its extension, bounded to the record name rule.
fn display_name(locator: &str) -> String {
    let stem = std::path::Path::new(locator)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("Video");
    let mut end = stem.len().min(255);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    stem[..end].to_owned()
}

impl ObjectFillOwner {
    /// Project and replace-target admission before the host probes the file.
    pub(crate) fn video_link_preflight(&self, args: &VideoLink) -> Result<(), DispatchErrorCode> {
        let origin = self.current_snapshot();
        let project = origin.project().ok_or_else(refusal)?;
        if let VideoIntent::Replace { placement_id } = &args.intent {
            let inventory = project
                .asset_inventory()
                .ok_or(DispatchErrorCode::NotFound)?;
            let old = inventory
                .placements()
                .iter()
                .find(|placement| placement.id() == placement_id)
                .ok_or(DispatchErrorCode::NotFound)?;
            let kind = inventory.get(old.asset_id()).map(|record| record.kind());
            if !matches!(old.target(), AssetTarget::Drawing { .. })
                || kind != Some(AssetKind::Video)
            {
                return Err(refusal());
            }
        }
        Ok(())
    }

    pub(crate) fn prepare_video_link(
        &self,
        request_id: &str,
        args: &VideoLink,
        facts: &LinkedVideoFacts,
    ) -> Result<PreparedVideoLink, DispatchErrorCode> {
        let origin = self.current_snapshot();
        if args.document_snapshot_id != origin.snapshot_id() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.video_link_preflight(args)?;
        if facts.width == 0 || facts.height == 0 || facts.frame_count == 0 {
            return Err(refusal());
        }
        let source =
            AssetSource::linked(facts.locator.clone(), MAX_PATH_BYTES).map_err(|_| refusal())?;
        let project = origin.project().ok_or_else(refusal)?;
        let mut records = project
            .asset_inventory()
            .map(|a| a.records().to_vec())
            .unwrap_or_default();
        let mut placements = project
            .asset_inventory()
            .map(|a| a.placements().to_vec())
            .unwrap_or_default();
        // Identity is the canonical path: an existing linked video record is reused as is.
        let asset_id = match records.iter().find(|record| {
            record.kind() == AssetKind::Video
                && record.source().locator() == Some(facts.locator.as_str())
        }) {
            Some(existing) => existing.id().clone(),
            None => {
                let id = AssetId::new(format!("asset-{request_id}")).map_err(|_| refusal())?;
                let record = AssetRecord::new(
                    id.clone(),
                    display_name(&facts.locator),
                    AssetKind::Video,
                    source,
                )
                .map_err(|_| refusal())?;
                records.push(Arc::new(record));
                id
            }
        };
        let (drawing, placement, new_layer) = match &args.intent {
            VideoIntent::NewLayer {} => {
                let bytes = serde_json::to_vec(&json!({"command":"object.layer.create",
                    "name":display_name(&facts.locator)}))
                .map_err(|_| refusal())?;
                let layer = self.prepare_layer_create_json(&bytes)?;
                let target = AssetTarget::Drawing {
                    context_id: "scene-root".into(),
                    layer_uid: layer.layer_uid.clone(),
                    stroke_id: None,
                };
                let placement =
                    AssetPlacement::new(format!("placement-{request_id}"), asset_id, target, 0)
                        .map_err(|_| refusal())?;
                placements.push(placement.clone());
                (layer.document, placement, true)
            }
            VideoIntent::Replace { placement_id } => {
                let at = placements
                    .iter()
                    .position(|placement| placement.id() == placement_id)
                    .ok_or(DispatchErrorCode::NotFound)?;
                let old = &placements[at];
                if old.asset_id() == &asset_id {
                    // Re-linking the very same file would publish an identical document.
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                // Same id, target, offset and stored timing (including time-remap keys).
                let next = AssetPlacement::new_number(
                    old.id().to_owned(),
                    asset_id,
                    old.target().clone(),
                    old.offset_frames().clone(),
                )
                .and_then(|next| next.with_timing(old.timing().clone()))
                .map_err(|_| refusal())?;
                placements[at] = next.clone();
                (origin.document().clone(), next, false)
            }
        };
        let AssetTarget::Drawing {
            context_id,
            layer_uid,
            ..
        } = placement.target().clone()
        else {
            return Err(refusal());
        };
        let inventory = AssetInventory::new(records, placements, MAX_ASSET_RECORDS, MAX_PLACEMENTS)
            .map_err(|_| refusal())?;
        check_inventory(&drawing, &inventory).map_err(|_| refusal())?;
        if crate::object_document::item_asset_issue(&drawing, Some(&inventory)).is_some() {
            return Err(refusal());
        }
        let next = project
            .with_asset_inventory(Some(inventory))
            .with_drawing(drawing);
        ObjectSnapshot::check_candidate_saveable(&next)?;
        Ok(PreparedVideoLink {
            origin,
            project: next,
            result: VideoLinkResult {
                asset_id: placement.asset_id().as_str().to_owned(),
                placement_id: placement.id().to_owned(),
                context_id,
                layer_uid,
                new_layer,
                facts: facts.clone(),
            },
        })
    }

    pub(crate) fn commit_video_link(
        &mut self,
        candidate: PreparedVideoLink,
    ) -> Result<u64, DispatchErrorCode> {
        let now = self.current_snapshot();
        if let Some(code) =
            crate::commands::object_fill_owner::object_raster_asset_owner::origin_refusal(
                &candidate.origin,
                &now,
            )
        {
            return Err(code);
        }
        self.revisions
            .publish_linked_video(candidate.project)
            .map_err(|_| refusal())
    }
}
