//! #2372 (A-bitmap-brush-edit): the baked companion of a bitmap brush anchor follows an edit of the anchor in the SAME revision.
//! A bitmap brush keeps its look as a baked PNG raster beside its anchor (`bitmapBrushSpec`, brush group, pressure profile on the anchor;
//! the raster carries `isBitmapBrush`, `isBrushTextureCopy` and the group). Baseline `SMBitmapBrush.regenerate` removes the companion and
//! bakes a new one from the anchor's CURRENT geometry and stored spec (same seed, so the texture does not "boil") after a move, a scale, a
//! rotation, a Width edit or a colour edit; and `stripAnyBrushTexture` removes it when a preset replaces the bitmap brush.
//! A batch that replaces a bitmap anchor reports a `Rebake` per anchor whose centreline, spec or pressure profile changed (`needed`), or that
//! stopped being a bitmap anchor. Once the batch's document is final, `apply` drops the companion (and its order element) of an anchor that is
//! no longer a bitmap brush and has the bake kernel (`object_snapshot::vector_rebake`) bake the others from the edited anchors. The result is
//! ONE candidate project, published as one revision and one history entry.
use crate::object_document::object_raster_asset_kernels::object_raster_asset_use::AssetId;
use crate::object_document::{BitmapBrushFields, ObjectRecord, ObjectTarget, Presence};

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

/// `drawing` (the batch's final document) with each companion re-baked or dropped, as one candidate project of `origin`.
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(crate) fn apply(
    origin: &crate::object_snapshot::ObjectSnapshot,
    mut drawing: crate::object_document::ObjectDocument,
    steps: &[Rebake],
) -> Result<crate::object_document::ProjectEnvelope, crate::request_receipts::DispatchErrorCode> {
    use crate::commands::remove_order_element;
    use crate::object_document::ItemRecords;
    use crate::request_receipts::DispatchErrorCode as Error;
    let mut rasters = drawing
        .items
        .value()
        .map_or(vec![], |i| i.raster_entries().to_vec());
    let texts = drawing
        .items
        .value()
        .map_or(vec![], |i| i.text_entries().to_vec());
    let (mut anchors, mut stale): (Vec<ObjectTarget>, Vec<AssetId>) = (Vec::new(), Vec::new());
    for step in steps {
        match step {
            Rebake::Bake(anchor) => anchors.push(anchor.clone()),
            Rebake::Drop(anchor, group) => {
                let companion = |r: &crate::object_document::RasterEntry| {
                    r.target.context_id == anchor.context_id
                        && r.target.layer_uid == anchor.layer_uid
                        && r.target.frame_scope == anchor.frame_scope
                        && r.item.brush_group_id() == Some(group.as_str())
                        && r.item.is_bitmap_brush() == Some(true)
                };
                while let Some(at) = rasters.iter().position(companion) {
                    let gone = rasters.remove(at);
                    stale.push(gone.item.asset_id().clone());
                    if let Presence::Present(plan) = &mut drawing.composition_order {
                        remove_order_element(plan, &gone.target).map_err(|_| Error::Unavailable)?;
                    }
                }
            }
        }
    }
    drawing.items = ItemRecords::normalized(rasters, texts);
    crate::object_snapshot::vector_rebake::rebake(origin, drawing, &anchors, stale)
}

/// Without the native baker a bitmap edit cannot be completed: a typed refusal, never a stale companion.
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
pub(crate) fn apply(
    _origin: &crate::object_snapshot::ObjectSnapshot,
    _drawing: crate::object_document::ObjectDocument,
    _steps: &[Rebake],
) -> Result<crate::object_document::ProjectEnvelope, crate::request_receipts::DispatchErrorCode> {
    Err(crate::request_receipts::DispatchErrorCode::Unavailable)
}
