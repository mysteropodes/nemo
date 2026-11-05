//! #2372 (A-bitmap-brush-edit): the baked companion of a bitmap brush anchor follows an edit of the anchor in the SAME revision.
//! A bitmap brush keeps its look as a baked PNG raster beside its anchor (`bitmapBrushSpec`, brush group, pressure profile on the anchor;
//! the raster carries `isBitmapBrush`, `isBrushTextureCopy` and the group). Baseline `SMBitmapBrush.regenerate` removes the companion and
//! bakes a new one from the anchor's CURRENT geometry and stored spec (same seed, so the texture does not "boil") after a move, a scale, a
//! rotation, a Width edit or a colour edit; and `stripAnyBrushTexture` removes it when a preset replaces the bitmap brush.
//! Here a batch that replaces a bitmap anchor reports a `Rebake` per anchor whose centreline, spec or pressure profile changed (`needed`),
//! or that stopped being a bitmap anchor; once the batch's document is final, `apply` bakes each image with the native stamp baker the
//! tweens use (`brush_trim_bake::consume_full_native`, the one pixel kernel), admits the PNG through the project's asset catalog, points the
//! companion at it with its new rectangle, and drops the companion (and its order element) of an anchor that is no longer a bitmap brush.
//! The result is ONE candidate project, published as one revision and one history entry; a refused bake (an unavailable tip, the image
//! budget) refuses the whole batch with nothing written.
use crate::object_document::{BitmapBrushFields, ObjectRecord, ObjectTarget};

/// What a replaced bitmap anchor asks of its companion image.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Rebake {
    /// Bake the companion again from the anchor stored at this target.
    Bake(ObjectTarget),
    /// The anchor at this target is no longer a bitmap brush: drop the companion of this brush group.
    Drop(ObjectTarget, String),
}

/// The step replacing `old` (the stored record at the same target, when it exists) with `new` asks of the companion, if any.
pub(crate) fn needed(old: Option<&ObjectRecord>, new: &ObjectRecord) -> Option<Rebake> {
    fn fields(r: &ObjectRecord) -> Option<&BitmapBrushFields> {
        r.vector_ext.value().and_then(|e| e.bitmap_brush.value())
    }
    match (old.and_then(|o| fields(o)), fields(new)) {
        (None, None) => None,
        (Some(was), None) => Some(Rebake::Drop(new.target.clone(), was.brush_group_id.clone())),
        (was, Some(now)) => {
            let same = was.is_some_and(|w| {
                w.bitmap_brush_spec == now.bitmap_brush_spec
                    && w.bitmap_pressure_profile == now.bitmap_pressure_profile
            }) && old.is_some_and(|o| o.geometry == new.geometry);
            (!same).then(|| Rebake::Bake(new.target.clone()))
        }
    }
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod bake {
    use super::Rebake;
    use crate::commands::object_fill_owner::vector_edit::order::remove_element;
    use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::{
        BakeRefused, TrimAnchor,
    };
    use crate::object_codec::{
        AssetInventory, AssetRecord, ItemRecords, ObjectDocument, ProjectEnvelope,
    };
    use crate::object_document::object_raster_asset_kernels::object_raster_asset_use::{
        asset_in_use, AssetId,
    };
    use crate::object_document::{ObjectTarget, Presence, RasterEntry};
    use crate::object_snapshot::{AssetEdit, ObjectSnapshot};
    use crate::request_receipts::DispatchErrorCode as Error;
    use std::sync::Arc;

    fn same_cell(a: &ObjectTarget, b: &ObjectTarget) -> bool {
        a.context_id == b.context_id && a.layer_uid == b.layer_uid && a.frame_scope == b.frame_scope
    }
    fn companion(rasters: &[RasterEntry], anchor: &ObjectTarget, group: &str) -> Option<usize> {
        rasters.iter().position(|r| {
            same_cell(&r.target, anchor)
                && r.item.brush_group_id() == Some(group)
                && r.item.is_bitmap_brush() == Some(true)
        })
    }

    /// `drawing` (the batch's final document) with each companion re-baked or dropped, as one candidate project of `origin`.
    pub(crate) fn apply(
        origin: &ObjectSnapshot,
        mut drawing: ObjectDocument,
        steps: &[Rebake],
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
        // The images the edit replaced or dropped: each leaves the catalog unless something else still shows it.
        let mut stale: Vec<AssetId> = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            match step {
                Rebake::Drop(anchor, group) => {
                    while let Some(at) = companion(&rasters, anchor, group) {
                        let gone = rasters.remove(at);
                        stale.push(gone.item.asset_id().clone());
                        if let Presence::Present(plan) = &mut drawing.composition_order {
                            remove_element(plan, &gone.target).map_err(|_| Error::Unavailable)?;
                        }
                    }
                }
                Rebake::Bake(anchor) => {
                    let record = drawing
                        .objects
                        .iter()
                        .find(|r| &r.target == anchor)
                        .ok_or(Error::Internal)?;
                    let fields = record
                        .vector_ext
                        .value()
                        .and_then(|e| e.bitmap_brush.value())
                        .ok_or(Error::Internal)?
                        .clone();
                    let at = companion(&rasters, anchor, &fields.brush_group_id)
                        .ok_or(Error::Unavailable)?;
                    let geometry =
                        TrimAnchor::from_record(record).map_err(|_| Error::Unavailable)?;
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
                        .prepare_asset_attachment(
                            &request_id,
                            "Bitmap brush.png",
                            "image/png",
                            png.as_ref(),
                        )
                        .map_err(|e| e.code())?;
                    let mut records = project
                        .asset_inventory()
                        .map_or(vec![], |i| i.records().to_vec());
                    let record: Arc<AssetRecord> = match admitted.into_edit() {
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
                    let mut value =
                        serde_json::to_value(&rasters[at].item).map_err(|_| Error::Internal)?;
                    value["assetId"] = record.id().as_str().into();
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
                    rasters[at].item =
                        serde_json::from_value(value).map_err(|_| Error::InvalidRequest)?;
                }
            }
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
}
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(crate) use bake::apply;

/// Without the native baker a bitmap edit cannot be completed: a typed refusal, never a stale companion.
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
pub(crate) fn apply(
    _origin: &crate::object_snapshot::ObjectSnapshot,
    _drawing: crate::object_document::ObjectDocument,
    _steps: &[Rebake],
) -> Result<crate::object_document::ProjectEnvelope, crate::request_receipts::DispatchErrorCode> {
    Err(crate::request_receipts::DispatchErrorCode::Unavailable)
}
