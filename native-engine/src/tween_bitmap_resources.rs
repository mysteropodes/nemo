//! Complete charged native brush resources and placements before the sealed owner may publish.
use super::BitmapTweenBake;
use crate::object_document::{ObjectDocument, ProjectEnvelope};
use crate::request_receipts::DispatchErrorCode as Error;
use crate::revision::ObjectSnapshot;

#[derive(Debug)]
pub(crate) struct PreparedTweenResources {
    project: ProjectEnvelope,
}
impl PreparedTweenResources {
    pub(crate) fn into_project(self) -> ProjectEnvelope {
        self.project
    }
}
pub(crate) fn prepare_tween_resources(
    origin: &ObjectSnapshot,
    drawing: ObjectDocument,
    bakes: Vec<BitmapTweenBake>,
) -> Result<PreparedTweenResources, Error> {
    #[cfg(not(feature = "project_media_codec"))]
    {
        let _ = (origin, drawing, bakes);
        Err(Error::Unavailable)
    }
    #[cfg(feature = "project_media_codec")]
    prepare(origin, drawing, bakes)
}
#[cfg(feature = "project_media_codec")]
fn prepare(
    origin: &ObjectSnapshot,
    mut drawing: ObjectDocument,
    bakes: Vec<BitmapTweenBake>,
) -> Result<PreparedTweenResources, Error> {
    use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::{
        BakeRefused, TrimAnchor,
    };
    use crate::object_document::{AssetInventory, AssetRecord, ItemRecords};
    use crate::object_snapshot::AssetEdit;
    use std::sync::Arc;
    let mut project = origin.project().ok_or(Error::Unavailable)?.clone();
    let mut rasters = drawing
        .items
        .value()
        .map_or(vec![], |i| i.raster_entries().to_vec());
    let texts = drawing
        .items
        .value()
        .map_or(vec![], |i| i.text_entries().to_vec());
    for (index, bake) in bakes.into_iter().enumerate() {
        let anchor = TrimAnchor::from_record(&bake.anchor).map_err(|_| Error::Unavailable)?;
        let (png, rect) = crate::evaluation::brush_trim_bake::consume_full_native(
            &bake.fields,
            &anchor,
            crate::revision::shared_image_resource_budget(),
            |ink| {
                let ink = ink.ok_or(BakeRefused("empty tween brush ink"))?;
                let png = crate::png_output::encode_rgba8(ink.width, ink.height, &ink.rgba)
                    .map_err(|_| BakeRefused("tween PNG encoding unavailable"))?;
                Ok((png, ink.rect))
            },
        )
        .map_err(|_| Error::Unavailable)?;
        // A detached immutable project view admits through the existing PNG/digest/quota port.
        // It never becomes a live owner; the candidate remains sealed to `origin` below.
        let staged = ObjectSnapshot::from_admitted_project(
            "native-tween-resource-stage".into(),
            project.clone(),
        )
        .map_err(|_| Error::Unavailable)?;
        let request_id = format!(
            "tween-{}-{}-{index}",
            origin.content_revision(),
            origin.document_id()
        );
        let admitted = staged
            .prepare_asset_attachment(&request_id, "Tween brush.png", "image/png", &png)
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
                            .is_some_and(|a| a.as_slice() == &*png)
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
        let target = rasters
            .iter_mut()
            .find(|r| r.target == bake.target)
            .ok_or(Error::Internal)?;
        let value = serde_json::json!({"assetId":record.id().as_str(),"x":rect.min_x+rect.w/2.,"y":rect.min_y+rect.h/2.,"width":rect.w,"height":rect.h,"opacity":bake.opacity,"isBrushTextureCopy":true,"isBitmapBrush":true,"brushGroupId":bake.fields.brush_group_id});
        target.item = serde_json::from_value(value).map_err(|_| Error::InvalidRequest)?;
    }
    drawing.items = ItemRecords::normalized(rasters, texts);
    ObjectSnapshot::validate_drawing(&drawing).map_err(|_| Error::InvalidRequest)?;
    if crate::object_document::item_asset_issue(&drawing, project.asset_inventory()).is_some() {
        return Err(Error::Unavailable);
    }
    let project = project.with_drawing(drawing);
    ObjectSnapshot::check_candidate_saveable(&project)?;
    Ok(PreparedTweenResources { project })
}
