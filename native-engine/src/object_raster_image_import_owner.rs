//! L06-M2: preparation of one asset and its still placement, sealed to the existing owner.
//! This private candidate is never another document owner and cannot publish either half.
use crate::commands::object_fill_owner::item_commands::{
    allocate_id, check_layer, check_target, lists, order_insert, store,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{
    AssetInventory, AssetKind, AssetRecord, FrameScope, FrameScopeKind, ObjectTarget,
    ProjectEnvelope, RasterEntry,
};
use crate::object_snapshot::{AssetEdit, ObjectSnapshot};
use crate::request_receipts::DispatchErrorCode;
use serde_json::json;
use std::sync::Arc;

#[path = "object_raster_image_import_wire.rs"]
pub(crate) mod wire;
use wire::{ImageImport, ImagePlacement};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageImportResult {
    pub(crate) asset_id: String,
    pub(crate) natural_width: u32,
    pub(crate) natural_height: u32,
    pub(crate) display_width: f64,
    pub(crate) display_height: f64,
    pub(crate) target: ObjectTarget,
    pub(crate) new_layer: bool,
}
impl ImageImportResult {
    pub(crate) fn result(&self) -> serde_json::Value {
        json!({"applied":true,"historyEntriesAdded":1,"assetId":self.asset_id,
            "naturalWidth":self.natural_width,"naturalHeight":self.natural_height,
            "displayWidth":self.display_width,"displayHeight":self.display_height,
            "target":self.target,"newLayer":self.new_layer})
    }
}

#[derive(Debug)]
pub(crate) struct PreparedImageImport {
    origin: ObjectSnapshot,
    project: ProjectEnvelope,
    added: Option<Arc<AssetRecord>>,
    result: ImageImportResult,
}
impl PreparedImageImport {
    pub(crate) fn result(&self) -> &ImageImportResult {
        &self.result
    }
}

fn refusal() -> DispatchErrorCode {
    DispatchErrorCode::Unavailable
}
pub(crate) fn canonical(
    origin: &ObjectSnapshot,
    edit: AssetEdit,
    bytes: &[u8],
) -> Result<(Arc<AssetRecord>, Option<Arc<AssetRecord>>), DispatchErrorCode> {
    match edit {
        AssetEdit::Add(record) => Ok((Arc::clone(&record), Some(record))),
        AssetEdit::Unchanged => {
            let existing = origin
                .project()
                .and_then(|p| p.asset_inventory())
                .and_then(|inventory| {
                    inventory.records().iter().find(|record| {
                        record.kind() == AssetKind::Image
                            && record.source().media_type() == Some("image/png")
                            && record
                                .source()
                                .encoded_allocation()
                                .is_some_and(|b| b.as_slice() == bytes)
                    })
                })
                .ok_or_else(refusal)?;
            Ok((Arc::clone(existing), None))
        }
        AssetEdit::Remove(_) => Err(DispatchErrorCode::InvalidRequest),
    }
}

impl ObjectFillOwner {
    /// Target/codec admission before host I/O and again before any native decode.
    pub(crate) fn image_import_preflight(
        &self,
        args: &ImageImport,
    ) -> Result<(), DispatchErrorCode> {
        let origin = self.current_snapshot();
        if origin.project().is_none() || args.media_type != "image/png" {
            return Err(refusal());
        }
        if let ImagePlacement::AuthoredFrame { layer_uid, frame } = &args.placement {
            let target = ObjectTarget {
                context_id: "scene-root".into(),
                layer_uid: layer_uid.clone(),
                stroke_id: "native-image-preflight".into(),
                frame_scope: FrameScope {
                    kind: FrameScopeKind::Authored,
                    frame: *frame,
                },
            };
            check_target(origin.document(), &target)?;
            check_layer(origin.document(), &target)?;
            let layer = origin
                .document()
                .layers()
                .iter()
                .find(|layer| layer.layer_uid() == layer_uid)
                .ok_or(DispatchErrorCode::NotFound)?;
            if layer
                .timing
                .value()
                .is_some_and(|timing| timing.exposure(*frame).0 != "key")
            {
                return Err(refusal());
            }
        }
        Ok(())
    }

    pub(crate) fn prepare_image_import(
        &self,
        request_id: &str,
        payload: &ImageImport,
        bytes: &[u8],
    ) -> Result<PreparedImageImport, DispatchErrorCode> {
        let origin = self.current_snapshot();
        if payload.document_snapshot_id != origin.snapshot_id() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.image_import_preflight(payload)?;
        let project = origin.project().ok_or_else(refusal)?;
        let prepared = origin
            .prepare_asset_attachment(request_id, &payload.name, &payload.media_type, bytes)
            .map_err(|error| error.code())?;
        let (record, added) = canonical(&origin, prepared.into_edit(), bytes)?;
        // Admission already validated the complete PNG, including the native decoder's dimensions.
        // Reading its fixed IHDR fields repeats no decode and never consults JavaScript metadata.
        let dimensions = bytes.get(16..24).ok_or_else(refusal)?;
        let width = u32::from_be_bytes(dimensions[0..4].try_into().map_err(|_| refusal())?);
        let height = u32::from_be_bytes(dimensions[4..8].try_into().map_err(|_| refusal())?);
        if width == 0 || height == 0 {
            return Err(refusal());
        }
        let mut records = project
            .asset_inventory()
            .map(|a| a.records().to_vec())
            .unwrap_or_default();
        if let Some(added) = &added {
            records.push(Arc::clone(added));
        }
        let placements = project
            .asset_inventory()
            .map(|a| a.placements().to_vec())
            .unwrap_or_default();
        let inventory = AssetInventory::new(records, placements.clone(), 1024, placements.len())
            .map_err(|_| refusal())?;
        let (mut drawing, layer_uid, frames, new_layer) = match &payload.placement {
            ImagePlacement::NewLayer {} => {
                let bytes = serde_json::to_vec(
                    &json!({"command":"object.layer.create","name":payload.name}),
                )
                .map_err(|_| refusal())?;
                let layer = self.prepare_layer_create_json(&bytes)?;
                // Native New-layer timing holds its authored frame zero for the full duration.
                let frames = 0..1;
                (layer.document, layer.layer_uid, frames, true)
            }
            ImagePlacement::AuthoredFrame { layer_uid, frame } => {
                let end = frame
                    .checked_add(1)
                    .ok_or(DispatchErrorCode::InvalidRequest)?;
                (
                    origin.document().clone(),
                    layer_uid.clone(),
                    *frame..end,
                    false,
                )
            }
        };
        let stroke_id = allocate_id(&drawing, origin.content_revision(), "native-raster")?;
        let target = ObjectTarget {
            context_id: "scene-root".into(),
            layer_uid,
            frame_scope: FrameScope {
                kind: FrameScopeKind::Authored,
                frame: frames.start,
            },
            stroke_id,
        };
        check_target(&drawing, &target)?;
        check_layer(&drawing, &target)?;
        if !new_layer {
            let layer = drawing
                .layers()
                .iter()
                .find(|layer| layer.layer_uid() == target.layer_uid())
                .ok_or(DispatchErrorCode::NotFound)?;
            if layer
                .timing
                .value()
                .is_some_and(|timing| timing.exposure(target.frame()).0 != "key")
            {
                // Same existing authored-key admission as vector import: never materialize a
                // held/tween cell by dropping other families or inventing JavaScript content.
                return Err(refusal());
            }
        }
        let (canvas_w, canvas_h) = project.project().canvas_dimensions();
        let fit = 1.0_f64
            .min(f64::from(canvas_w) / f64::from(width))
            .min(f64::from(canvas_h) / f64::from(height));
        let display_width = f64::from(width) * fit;
        let display_height = f64::from(height) * fit;
        let item: RasterItem = serde_json::from_value(json!({"assetId":record.id().as_str(),
            "x":f64::from(canvas_w)/2.0,"y":f64::from(canvas_h)/2.0,
            "width":display_width,"height":display_height,"opacity":1}))
        .map_err(|_| refusal())?;
        let (mut raster, text) = lists(&drawing);
        // A candidate that cannot possibly fit the existing 8 MiB drawing envelope is refused
        // before expansion. The full native storage admission below remains authoritative.
        let per_record = serde_json::to_vec(&RasterEntry {
            target: target.clone(),
            item: item.clone(),
        })
        .map_err(|_| refusal())?
        .len();
        if (frames.len() as usize)
            .checked_mul(per_record)
            .is_none_or(|size| size > 8 * 1024 * 1024)
        {
            return Err(refusal());
        }
        raster
            .try_reserve_exact(frames.len())
            .map_err(|_| refusal())?;
        for frame in frames {
            let mut at = target.clone();
            at.frame_scope.frame = frame;
            check_target(&drawing, &at)?;
            order_insert(&mut drawing, &at)?;
            raster.push(RasterEntry {
                target: at,
                item: item.clone(),
            });
        }
        store(&mut drawing, raster, text);
        ObjectSnapshot::validate_drawing(&drawing)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if crate::object_document::item_asset_issue(&drawing, Some(&inventory)).is_some() {
            return Err(refusal());
        }
        let next = project
            .with_asset_inventory(Some(inventory))
            .with_drawing(drawing);
        ObjectSnapshot::check_candidate_saveable(&next)?;
        Ok(PreparedImageImport {
            origin,
            project: next,
            added,
            result: ImageImportResult {
                asset_id: record.id().as_str().to_owned(),
                natural_width: width,
                natural_height: height,
                display_width,
                display_height,
                target,
                new_layer,
            },
        })
    }

    pub(crate) fn commit_image_import(
        &mut self,
        candidate: PreparedImageImport,
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
            .publish_image_import(candidate.project, candidate.added)
            .map_err(|_| refusal())
    }
}
