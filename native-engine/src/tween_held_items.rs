//! Held raster placements share the existing asset inventory; tweening never copies image bytes.
use crate::object_document::{
    FrameScopeKind, ItemRecords, ObjectDocument, ObjectRecord, ObjectTarget, Presence, RasterEntry,
};
use crate::request_receipts::DispatchErrorCode;
use crate::tween_kernels::*;
pub(super) fn target_in_cell(t: &ObjectTarget, layer: &str, frame: u32) -> bool {
    t.context_id == "scene-root"
        && t.layer_uid == layer
        && t.frame_scope.kind == FrameScopeKind::Authored
        && t.frame_scope.frame == frame
}
#[derive(Clone)]
pub(super) enum Held {
    Object(ObjectRecord),
    Raster(RasterEntry),
}
impl Held {
    pub(super) fn target(&self) -> &ObjectTarget {
        match self {
            Self::Object(r) => &r.target,
            Self::Raster(r) => &r.target,
        }
    }
    pub(super) fn at(&self, frame: u32) -> Self {
        let mut result = self.clone();
        match &mut result {
            Self::Object(r) => r.target.frame_scope.frame = frame,
            Self::Raster(r) => r.target.frame_scope.frame = frame,
        }
        result
    }
}
pub(super) fn held_entries(
    doc: &ObjectDocument,
    layer: &str,
    frame: u32,
    manual: Option<&Vec<String>>,
) -> Vec<Held> {
    let mut held: Vec<_> = frame_records(doc, layer, frame)
        .into_iter()
        .filter(|r| {
            !is_dab(r)
                && !is_backdrop(r)
                && manual.is_some_and(|ids| !ids.contains(&r.target.stroke_id))
        })
        .map(Held::Object)
        .collect();
    held.extend(
        doc.items
            .value()
            .into_iter()
            .flat_map(|items| items.raster_entries())
            .filter(|r| {
                target_in_cell(&r.target, layer, frame)
                    && r.item.is_brush_texture_copy() != Some(true)
            })
            .cloned()
            .map(Held::Raster),
    );
    if let Some(scope) = doc
        .composition_order
        .value()
        .into_iter()
        .flat_map(|p| &p.contexts)
        .find(|c| c.context_id == "scene-root")
        .and_then(|c| {
            c.scopes.iter().find(|s| {
                s.frame_scope.kind == FrameScopeKind::Authored && s.frame_scope.frame == frame
            })
        })
    {
        held.sort_by_key(|r| {
            scope
                .elements
                .iter()
                .find(|e| e.layer_uid == layer && e.stroke_id == r.target().stroke_id)
                .map_or(u32::MAX, |e| e.natural_position)
        });
    }
    held
}
pub(super) fn replace_mixed(
    doc: &mut ObjectDocument,
    layer: &str,
    frame: u32,
    records: Vec<Held>,
) -> R<()> {
    let old: Vec<_> = doc
        .objects
        .iter()
        .map(|r| &r.target)
        .chain(
            doc.items
                .value()
                .into_iter()
                .flat_map(|i| i.raster_entries())
                .map(|r| &r.target),
        )
        .filter(|t| target_in_cell(t, layer, frame))
        .cloned()
        .collect();
    if let Presence::Present(plan) = &mut doc.composition_order {
        for t in &old {
            crate::commands::remove_order_element(plan, t)
                .map_err(|_| DispatchErrorCode::Unavailable)?;
        }
        for r in &records {
            crate::commands::insert_order_element(plan, r.target())
                .map_err(|_| DispatchErrorCode::Unavailable)?;
        }
    }
    doc.objects
        .retain(|r| !target_in_cell(&r.target, layer, frame));
    let mut rasters = doc
        .items
        .value()
        .map_or(vec![], |i| i.raster_entries().to_vec());
    let texts = doc
        .items
        .value()
        .map_or(vec![], |i| i.text_entries().to_vec());
    rasters.retain(|r| !target_in_cell(&r.target, layer, frame));
    for r in records {
        match r {
            Held::Object(r) => doc.objects.push(r),
            Held::Raster(r) => rasters.push(r),
        }
    }
    doc.items = ItemRecords::normalized(rasters, texts);
    Ok(())
}
