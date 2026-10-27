//! #2361: one native bitmap-brush Draw stroke, prepared as one candidate project and sealed to the existing owner.
//! bitmap-brush.js applyBitmapBrushTexture @1065e9d8 leaves the anchor (camouflaged, carrying bitmapBrushSpec, its brush
//! group and pressure profile) and ONE companion raster above it (isBrushTextureCopy, isBitmapBrush, the same group)
//! holding the baked stamps. The baked PNG joins the asset catalog in the same revision, and the render join paints the
//! companion from it. A held or unexposed cell first becomes a key (Draw's ensureKeyframe), in the same undo step.
use crate::commands::object_fill_owner::image_import::canonical;
use crate::commands::object_fill_owner::item_commands::{
    allocate_id, check_layer, check_target, lists, order_insert, store,
};
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{
    AssetInventory, AssetRecord, FrameScopeKind, ObjectDocument, ObjectTarget, Presence, ProjectEnvelope, RasterEntry,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::json;
use std::sync::Arc;

#[path = "object_brush_bitmap_stroke_wire.rs"]
pub(crate) mod wire;
use wire::{brush_group, BitmapStroke};

#[derive(Debug)]
pub(crate) struct PreparedBitmapStroke {
    origin: ObjectSnapshot,
    project: ProjectEnvelope,
    added: Option<Arc<AssetRecord>>,
}

fn refusal() -> DispatchErrorCode {
    DispatchErrorCode::Unavailable
}

/// Front: the top of the layer's natural stack. Behind (draw-bridge.js insertChild(0, path), then the companion
/// re-anchored above its anchor @1065e9d8): natural positions 0 and 1, which only an order plan can express.
fn place(
    drawing: &mut ObjectDocument,
    target: &ObjectTarget,
    behind: bool,
    position: usize,
) -> Result<(), DispatchErrorCode> {
    if !behind {
        return order_insert(drawing, target);
    }
    let Presence::Present(plan) = &mut drawing.composition_order else {
        return Err(refusal());
    };
    crate::commands::object_fill_owner::vector_edit::order::insert_element_at(plan, target, position)
        .map_err(crate::commands::object_fill_owner::vector_edit::order_code)
}

impl ObjectFillOwner {
    /// Target admission before host I/O and again before any native decode.
    pub(crate) fn bitmap_stroke_preflight(
        &self,
        args: &BitmapStroke,
    ) -> Result<(), DispatchErrorCode> {
        let origin = self.current_snapshot();
        if origin.project().is_none()
            || args.record.target.frame_scope.kind != FrameScopeKind::Authored
        {
            return Err(refusal());
        }
        let target = ObjectTarget {
            stroke_id: "native-bitmap-preflight".into(),
            ..args.record.target.clone()
        };
        check_target(origin.document(), &target)?;
        check_layer(origin.document(), &target)
    }

    pub(crate) fn prepare_bitmap_stroke(
        &self,
        request_id: &str,
        payload: &BitmapStroke,
        bytes: &[u8],
    ) -> Result<PreparedBitmapStroke, DispatchErrorCode> {
        let origin = self.current_snapshot();
        if payload.document_snapshot_id != origin.snapshot_id() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.bitmap_stroke_preflight(payload)?;
        let project = origin.project().ok_or_else(refusal)?;
        let prepared = origin
            .prepare_asset_attachment(request_id, "Bitmap brush.png", "image/png", bytes)
            .map_err(|error| error.code())?;
        let (asset, added) = canonical(&origin, prepared.into_edit(), bytes)?;
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
        let revision = origin.content_revision();
        let cell = &payload.record.target;
        let mut drawing = origin.document().clone();
        let held = drawing
            .layers
            .iter()
            .find(|layer| layer.layer_uid == cell.layer_uid)
            .and_then(|layer| layer.timing.value())
            .is_some_and(|timing| matches!(timing.exposure(cell.frame()).0, "held" | "unexposed"));
        if held {
            if let DocumentEdit::Changed(next) =
                promote_cells(&drawing, &cell.layer_uid, &[cell.frame()]).map_err(|_| refusal())?
            {
                drawing = next;
            }
        }
        let behind = payload.placement.is_some();
        let mut anchor = payload.record.clone();
        anchor.target.stroke_id = allocate_id(&drawing, revision, "native-stroke")?;
        check_target(&drawing, &anchor.target)?;
        check_layer(&drawing, &anchor.target)?;
        place(&mut drawing, &anchor.target, behind, 0)?;
        drawing.objects.push(anchor.clone());
        let group = brush_group(&anchor).ok_or(DispatchErrorCode::InvalidRequest)?;
        let at = ObjectTarget {
            stroke_id: allocate_id(&drawing, revision, "native-raster")?,
            ..anchor.target.clone()
        };
        let c = &payload.companion;
        let item: RasterItem = serde_json::from_value(json!({"assetId":asset.id().as_str(),"x":c.x,"y":c.y,
            "width":c.width,"height":c.height,"opacity":c.opacity,"isBitmapBrush":true,"isBrushTextureCopy":true,
            "brushGroupId":group}))
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let (mut raster, text) = lists(&drawing);
        place(&mut drawing, &at, behind, 1)?;
        raster.push(RasterEntry {
            target: at.clone(),
            item,
        });
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
        Ok(PreparedBitmapStroke {
            origin,
            project: next,
            added,
        })
    }

    pub(crate) fn commit_bitmap_stroke(
        &mut self,
        candidate: PreparedBitmapStroke,
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
