//! #2372 (A-bitmap-brush-edit): the bake half of re-baking a bitmap brush's companion image when its anchor is edited
//! (`object_vector_batch_rebake.rs` decides which anchors, drops the companions of anchors that stopped being bitmap brushes, and
//! owns the revision). Each anchor's image is baked with the native stamp baker the tweens use (`brush_trim_bake::consume_full_native`,
//! the one pixel kernel) from the anchor and spec the edited document holds, admitted through the project's asset catalog, and the
//! companion raster (same cell, same brush group, `isBitmapBrush`) is pointed at it with the new world rectangle; its other members
//! (opacity) stay. An image no raster shows any more leaves the catalog, so nudging a stroke does not pile up PNGs. The result is ONE
//! candidate project of the origin; a refused bake (an unavailable tip, the image budget) refuses the whole edit with nothing written.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::{
    BakeRefused, TrimAnchor,
};
use crate::object_codec::raster_asset_use::{asset_in_use, AssetId};
use crate::object_codec::{
    AssetInventory, AssetRecord, ItemRecords, ObjectDocument, ObjectTarget, ProjectEnvelope,
};
use crate::object_snapshot::{AssetEdit, ObjectSnapshot};
use crate::request_receipts::DispatchErrorCode as Error;
use std::sync::Arc;

fn same_cell(a: &ObjectTarget, b: &ObjectTarget) -> bool {
    a.context_id == b.context_id && a.layer_uid == b.layer_uid && a.frame_scope == b.frame_scope
}

/// `drawing` (the edit's final document) with the companion of each anchor in `anchors` baked again, as one candidate project of
/// `origin`. `stale` are the images of companions the edit already dropped: they leave the catalog too when nothing shows them.
pub(crate) fn rebake(
    origin: &ObjectSnapshot,
    mut drawing: ObjectDocument,
    anchors: &[ObjectTarget],
    mut stale: Vec<AssetId>,
) -> Result<ProjectEnvelope, Error> {
    let mut project = origin.project().ok_or(Error::Unavailable)?.clone();
    let mut rasters = drawing
        .items
        .value()
        .map_or(vec![], |i| i.raster_entries().to_vec());
    let texts = drawing
        .items
        .value()
        .map_or(vec![], |i| i.text_entries().to_vec());
    for (index, anchor) in anchors.iter().enumerate() {
        // The anchor is addressed by its full target: a scene record, or a record of the component the target names.
        let record = drawing
            .objects
            .iter()
            .chain(
                drawing
                    .component_set()
                    .and_then(|set| set.component(&anchor.context_id))
                    .map_or(&[][..], |component| component.objects.as_slice()),
            )
            .find(|r| &r.target == anchor)
            .ok_or(Error::Internal)?;
        let fields = record
            .vector_ext
            .value()
            .and_then(|e| e.bitmap_brush.value())
            .ok_or(Error::Internal)?
            .clone();
        let at = rasters
            .iter()
            .position(|r| {
                same_cell(&r.target, anchor)
                    && r.item.brush_group_id() == Some(fields.brush_group_id.as_str())
                    && r.item.is_bitmap_brush() == Some(true)
            })
            .ok_or(Error::Unavailable)?;
        let geometry = TrimAnchor::from_record(record).map_err(|_| Error::Unavailable)?;
        let (png, rect) = crate::evaluation::brush_trim_bake::consume_full_native(
            &fields,
            &geometry,
            crate::revision::shared_image_resource_budget(),
            |ink| {
                let ink = ink.ok_or(BakeRefused("empty bitmap brush ink"))?;
                let png = crate::object_snapshot::tween_read::encode_brush_png(
                    ink.width, ink.height, &ink.rgba,
                )
                .map_err(|_| BakeRefused("bitmap brush PNG encoding unavailable"))?;
                Ok((png, ink.rect))
            },
        )
        .map_err(|_| Error::Unavailable)?;
        // A detached immutable project view admits the PNG through the existing digest / quota port; it never becomes a live owner.
        let staged = ObjectSnapshot::from_admitted_project(
            "native-bitmap-rebake-stage".into(),
            project.clone(),
        )
        .map_err(|_| Error::Unavailable)?;
        let request_id = format!(
            "rebake-{}-{}-{index}",
            origin.content_revision(),
            origin.document_id()
        );
        let admitted = staged
            .prepare_asset_attachment(&request_id, "Bitmap brush.png", "image/png", png.as_ref())
            .map_err(|e| e.code())?;
        let mut records = project
            .asset_inventory()
            .map_or(vec![], |i| i.records().to_vec());
        let added: Arc<AssetRecord> = match admitted.into_edit() {
            AssetEdit::Add(record) => {
                records.push(Arc::clone(&record));
                record
            }
            AssetEdit::Unchanged => records
                .iter()
                .find(|r| {
                    r.source().media_type() == Some("image/png")
                        && r.source()
                            .encoded_allocation()
                            .is_some_and(|a| a.as_slice() == png.as_ref())
                })
                .cloned()
                .ok_or(Error::Unavailable)?,
            AssetEdit::Remove(_) => return Err(Error::Internal),
        };
        let placements = project
            .asset_inventory()
            .map_or(vec![], |i| i.placements().to_vec());
        let limit = placements.len();
        let inventory = AssetInventory::new(records, placements, 1024, limit)
            .map_err(|_| Error::Unavailable)?;
        project = project.with_asset_inventory(Some(inventory));
        stale.push(rasters[at].item.asset_id().clone());
        // The companion keeps its flags and opacity; the image and its rectangle are the new bake's.
        let mut value = serde_json::to_value(&rasters[at].item).map_err(|_| Error::Internal)?;
        value["assetId"] = added.id().as_str().into();
        for (key, number) in [
            ("x", rect.min_x + rect.w / 2.),
            ("y", rect.min_y + rect.h / 2.),
            ("width", rect.w),
            ("height", rect.h),
        ] {
            value[key] = serde_json::Number::from_f64(number)
                .ok_or(Error::Unavailable)?
                .into();
        }
        rasters[at].item = serde_json::from_value(value).map_err(|_| Error::InvalidRequest)?;
    }
    drawing.items = ItemRecords::normalized(rasters, texts);
    ObjectSnapshot::validate_drawing(&drawing).map_err(|_| Error::InvalidRequest)?;
    if crate::object_codec::item_asset_issue(&drawing, project.asset_inventory()).is_some() {
        return Err(Error::Unavailable);
    }
    let mut project = project.with_drawing(drawing);
    let unused: Vec<AssetId> = stale
        .iter()
        .filter(|id| !asset_in_use(&project, id))
        .cloned()
        .collect();
    if !unused.is_empty() {
        let inventory = project.asset_inventory().ok_or(Error::Internal)?;
        let records: Vec<Arc<AssetRecord>> = inventory
            .records()
            .iter()
            .filter(|r| !unused.contains(r.id()))
            .cloned()
            .collect();
        let placements = inventory.placements().to_vec();
        let limit = placements.len();
        let pruned = AssetInventory::new(records, placements, 1024, limit)
            .map_err(|_| Error::Unavailable)?;
        project = project.with_asset_inventory(Some(pruned));
    }
    ObjectSnapshot::check_candidate_saveable(&project)?;
    Ok(project)
}
