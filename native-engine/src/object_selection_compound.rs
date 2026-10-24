//! Original independent anchors and their actual painted companions; no authority or resources.
use std::collections::HashSet;
use crate::object_document::{FrameScopeKind, ItemRecords, ObjectDocument, ObjectRecord,
    ObjectTarget, Presence, RasterEntry, VECTOR_FAMILY};
use crate::object_document::object_group_kernels::object_selection_edit_plan::SelectionError;
use crate::object_document::object_group_kernels::object_selection_edit_frames::in_cell;

#[derive(Debug, Clone)]
pub(crate) struct CompoundSelection {
    pub(crate) anchors: Vec<ObjectRecord>,
    pub(crate) records: Vec<ObjectRecord>,
    pub(crate) rasters: Vec<RasterEntry>,
}
pub(crate) fn is_companion(record: &ObjectRecord) -> bool {
    record.vector_ext.value().and_then(|x| x.is_linked_fill_companion.value()) == Some(&true)
}
pub(crate) fn same_cell(a: &ObjectTarget, b: &ObjectTarget) -> bool {
    a.context_id == b.context_id && a.layer_uid == b.layer_uid && a.frame_scope == b.frame_scope
}
pub(crate) fn resolve(document: &ObjectDocument, context: &str, layer: &str, frame: u32,
    selected_ids: &[String]) -> Result<CompoundSelection, SelectionError> {
    let mut out = CompoundSelection { anchors: vec![], records: vec![], rasters: vec![] };
    let mut selected = HashSet::new();
    for id in selected_ids {
        if !selected.insert(id) { return Err(SelectionError::ClipMismatch); }
        let anchor = document.objects.iter().find(|r| in_cell(r,context,layer,frame) && r.target.stroke_id == *id)
            .ok_or_else(||SelectionError::UnknownStroke(id.clone()))?;
        if is_companion(anchor) { return Err(SelectionError::ClipMismatch); }
        out.anchors.push(anchor.clone());
        if let Some(ext) = anchor.vector_ext.value() {
            if let Some(link) = ext.linked_fill_id.value() {
                if link.is_empty() || link.len()>128 || link.chars().any(char::is_control)
                    || anchor.family != VECTOR_FAMILY || !ext.bitmap_brush.missing()
                    || ext.pressure.value().and_then(|p|p.center_segments.value()).is_none_or(|p|p.len()<2) {
                    return Err(SelectionError::ClipMismatch);
                }
                let related: Vec<_> = document.objects.iter().filter(|r|same_cell(&r.target,&anchor.target))
                    .filter(|r|r.vector_ext.value().and_then(|x|x.linked_fill_id.value())==Some(link)).collect();
                if related.len()!=2 { return Err(SelectionError::ClipMismatch); }
                let companions: Vec<_> = related.into_iter().filter(|r|is_companion(r)).collect();
                if companions.len()!=1 { return Err(SelectionError::ClipMismatch); }
                let companion=companions[0];
                let x=companion.vector_ext.value().ok_or(SelectionError::ClipMismatch)?;
                if companion.family!=VECTOR_FAMILY || !x.pressure.missing() || !x.bitmap_brush.missing()
                    || companion.solid_fill().is_none() {
                    return Err(SelectionError::ClipMismatch);
                }
                out.records.push(companion.clone());
            }
            if let Some(brush) = ext.bitmap_brush.value() {
                let anchors = document.objects.iter().filter(|r|same_cell(&r.target,&anchor.target))
                    .filter(|r|r.vector_ext.value().and_then(|x|x.bitmap_brush.value())
                        .is_some_and(|b|b.brush_group_id==brush.brush_group_id)).count();
                let rasters: Vec<_> = document.items.value().map_or(&[][..],|i|i.raster_entries()).iter()
                    .filter(|r|same_cell(&r.target,&anchor.target) && r.item.brush_group_id()==Some(brush.brush_group_id.as_str())).collect();
                if anchors!=1 || rasters.len()!=1 || !ext.linked_fill_id.missing() { return Err(SelectionError::ClipMismatch); }
                let raster=rasters[0];
                if raster.item.is_bitmap_brush()!=Some(true) || raster.item.is_brush_texture_copy()!=Some(true)
                    || raster.item.mesh_id().is_some() {
                    return Err(SelectionError::ClipMismatch);
                }
                out.rasters.push(raster.clone());
            }
        }
        out.records.push(anchor.clone());
    }
    Ok(out)
}
/// Validate actual primary relationships without rejecting ordinary imported raster passthrough tags.
pub(crate) fn validate_document(document: &ObjectDocument) -> Result<(),SelectionError> {
    if let Some(set)=document.component_set() {
        for component in &set.components {
            let mut view=document.clone();view.objects=component.objects.clone();view.layers=component.layers.clone();
            view.items=Presence::Missing;view.composition_order=Presence::Missing;view.component_tables=Presence::Missing;
            validate_document(&view)?;
        }
    }
    for record in &document.objects {
        let Some(ext)=record.vector_ext.value() else {continue;};
        if is_companion(record) {
            let link=ext.linked_fill_id.value().ok_or(SelectionError::ClipMismatch)?;
            let primaries: Vec<_> = document.objects.iter().filter(|r|same_cell(&r.target,&record.target) && !is_companion(r))
                .filter(|r|r.vector_ext.value().and_then(|x|x.linked_fill_id.value())==Some(link)).collect();
            if primaries.len()!=1 {return Err(SelectionError::ClipMismatch);}
        } else if !ext.linked_fill_id.missing() || !ext.bitmap_brush.missing() {
            if record.target.frame_scope.kind!=FrameScopeKind::Authored {return Err(SelectionError::ClipMismatch);}
            resolve(document,&record.target.context_id,&record.target.layer_uid,record.target.frame_scope.frame,&[record.target.stroke_id.clone()])?;
        }
    }
    Ok(())
}
pub(crate) fn raster_entries(document:&ObjectDocument)->&[RasterEntry] {
    document.items.value().map_or(&[],|items|items.raster_entries())
}
pub(crate) fn append_rasters(document:&mut ObjectDocument, entries:Vec<RasterEntry>) {
    if entries.is_empty() { return; }
    if document.items.missing() {document.items=Presence::Present(ItemRecords{raster:Presence::Missing,text:Presence::Missing});}
    if let Presence::Present(items)=&mut document.items {
        if items.raster.missing() {items.raster=Presence::Present(vec![]);}
        if let Presence::Present(rasters)=&mut items.raster {rasters.extend(entries);}
    }
}
pub(crate) fn remove_rasters(document:&mut ObjectDocument, targets:&[ObjectTarget]) {
    if let Presence::Present(items)=&mut document.items {
        if let Presence::Present(rasters)=&mut items.raster {
            rasters.retain(|r|!targets.contains(&r.target));
            if rasters.is_empty() {items.raster=Presence::Missing;}
        }
        if items.raster.missing() && items.text.missing() {document.items=Presence::Missing;}
    }
}
