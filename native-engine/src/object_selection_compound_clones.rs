//! L02-C identity materialization for actual linked paths and existing raster companions.
use crate::object_document::object_group_kernels::{
    object_clipboard_remap::{
        plan_clone, remap_clone_groups, validate_clone_groups, CloneRequest, DabSource, IdIssuer,
        IdKind, IdSource, ItemSource, RemapError,
    },
    object_group_hierarchy::GroupEntry,
    object_group_record::{layer_stroke_ids, GroupMember, GroupTable},
    object_selection_compound as compound,
    object_selection_edit_clones::{apply_clones, shift_record, Clones},
    object_selection_edit_frames::append_elements,
    object_selection_edit_groups::{record_of, settle},
    object_selection_edit_plan::SelectionError,
};
use crate::object_document::{
    FrameScope, FrameScopeKind, ItemRecords, ObjectDocument, ObjectRecord, Presence, RasterEntry,
};
use std::collections::HashSet;

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_compound_clones(
    work: &mut ObjectDocument,
    dest_index: usize,
    context_id: &str,
    frame: u32,
    records: &[ObjectRecord],
    rasters: &[RasterEntry],
    tags: &[Option<String>],
    source_groups: Option<&[GroupEntry]>,
    offset: f64,
    ids: &mut dyn IdSource,
) -> Result<Clones, SelectionError> {
    if records.len() != tags.len() {
        return Err(SelectionError::ClipMismatch);
    }
    if rasters.is_empty()
        && !records.iter().any(|r| {
            r.vector_ext.value().is_some_and(|x| {
                !x.linked_fill_id.missing()
                    || !x.bitmap_brush.missing()
                    || compound::is_companion(r)
            })
        })
    {
        return apply_clones(
            work,
            dest_index,
            context_id,
            frame,
            records,
            tags,
            source_groups,
            offset,
            ids,
        );
    }
    let first = records.first().ok_or(SelectionError::ClipMismatch)?;
    // A detached packet projection is only a plan input. Owner/snapshot/resource provenance is checked
    // by the sealed command owner before this pure kernel; it grants no publication or asset authority.
    let mut source = work.clone();
    source.objects = records.to_vec();
    source.composition_order = Presence::Missing;
    source.items = if rasters.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(ItemRecords {
            raster: Presence::Present(rasters.to_vec()),
            text: Presence::Missing,
        })
    };
    let anchor_ids: Vec<String> = records
        .iter()
        .filter(|r| !compound::is_companion(r))
        .map(|r| r.target.stroke_id.clone())
        .collect();
    let selected = compound::resolve(
        &source,
        &first.target.context_id,
        &first.target.layer_uid,
        first.target.frame_scope.frame,
        &anchor_ids,
    )?;
    if selected.records != records || selected.rasters != rasters {
        return Err(SelectionError::ClipMismatch);
    }
    if let Some(groups) = source_groups {
        let borrowed: Vec<_> = tags.iter().map(|tag| tag.as_deref()).collect();
        validate_clone_groups(groups, &borrowed).map_err(SelectionError::Remap)?;
    }
    let mut sources = Vec::new();
    for anchor in &selected.anchors {
        let at = records
            .iter()
            .position(|r| r.target == anchor.target)
            .ok_or(SelectionError::ClipMismatch)?;
        let ext = anchor.vector_ext.value();
        let brush = ext.and_then(|x| x.bitmap_brush.value());
        let companion = brush.and_then(|b| {
            rasters
                .iter()
                .find(|r| r.item.brush_group_id() == Some(b.brush_group_id.as_str()))
        });
        sources.push(ItemSource {
            stroke_id: Some(anchor.target.stroke_id.clone()),
            group_id: tags[at].clone(),
            brush_group_id: brush.map(|b| b.brush_group_id.clone()),
            linked_fill_id: ext.and_then(|x| x.linked_fill_id.value()).cloned(),
            vector_brush_with_centerline: ext
                .and_then(|x| x.pressure.value())
                .and_then(|p| p.center_segments.value())
                .is_some(),
            linked_fill_snapshot: ext.is_some_and(|x| !x.linked_fill_id.missing()),
            dabs: companion
                .map(|r| {
                    vec![DabSource {
                        stroke_id: Some(r.target.stroke_id.clone()),
                        group_id: None,
                    }]
                })
                .unwrap_or_default(),
            ..ItemSource::default()
        });
    }
    let destination_gids: Vec<String> =
        work.layers[dest_index].groups.value().map_or(vec![], |t| {
            t.entries.iter().map(|e| e.gid.clone()).collect()
        });
    let plan = plan_clone(
        &CloneRequest {
            items: &sources,
            source_groups: None,
            destination_group_ids: &destination_gids,
            offset,
        },
        ids,
    )
    .map_err(SelectionError::Remap)?;
    let mut taken: HashSet<String> = work
        .objects
        .iter()
        .map(|r| r.target.stroke_id.clone())
        .chain(
            work.items
                .value()
                .into_iter()
                .flat_map(|i| i.targets().map(|t| t.stroke_id.clone())),
        )
        .chain(records.iter().map(|r| r.target.stroke_id.clone()))
        .chain(rasters.iter().map(|r| r.target.stroke_id.clone()))
        .collect();
    taken.extend(
        records
            .iter()
            .filter_map(|record| record.vector_ext.value())
            .filter_map(|ext| ext.linked_fill_id.value())
            .cloned(),
    );
    taken.extend(
        sources
            .iter()
            .filter_map(|source| source.brush_group_id.clone()),
    );
    taken.extend(destination_gids.iter().cloned());
    if let Some(groups) = source_groups {
        taken.extend(groups.iter().map(|group| group.gid.clone()));
    }
    for fresh in plan
        .items
        .iter()
        .map(|i| i.stroke_id.as_str())
        .chain(plan.dabs.iter().filter_map(|d| d.stroke_id.as_deref()))
    {
        if !taken.insert(fresh.to_string()) {
            return Err(SelectionError::Remap(RemapError::IdCollision(fresh.into())));
        }
    }
    for fresh in plan
        .items
        .iter()
        .filter_map(|item| item.linked_fill_id.as_ref())
        .chain(
            plan.items
                .iter()
                .filter_map(|item| item.brush_group_id.as_ref()),
        )
    {
        if !taken.insert(fresh.clone()) {
            return Err(SelectionError::Remap(RemapError::IdCollision(
                fresh.clone(),
            )));
        }
    }
    let dest_uid = work.layers[dest_index].layer_uid.clone();
    let retarget = |record: &mut ObjectRecord, id: String| {
        record.target.context_id = context_id.into();
        record.target.layer_uid = dest_uid.clone();
        record.target.frame_scope = FrameScope {
            kind: FrameScopeKind::Authored,
            frame,
        };
        record.target.stroke_id = id;
    };
    let mut copies = Vec::new();
    let mut raster_copies = Vec::new();
    let mut paint_ids = Vec::new();
    let mut stroke_map = plan.stroke_map.clone();
    for item in &plan.items {
        let anchor = &selected.anchors[item.source];
        let mut copied = anchor.clone();
        if item.companion {
            let old_link = anchor
                .vector_ext
                .value()
                .and_then(|x| x.linked_fill_id.value())
                .ok_or(SelectionError::ClipMismatch)?;
            let original = records
                .iter()
                .find(|r| {
                    compound::is_companion(r)
                        && r.vector_ext.value().and_then(|x| x.linked_fill_id.value())
                            == Some(old_link)
                })
                .ok_or(SelectionError::ClipMismatch)?;
            let id = ids
                .fresh(IdKind::Stroke, Some(&original.target.stroke_id))
                .ok_or(SelectionError::IdUnavailable)?;
            if id.is_empty() || !taken.insert(id.clone()) {
                return Err(SelectionError::Remap(RemapError::IdCollision(id)));
            }
            stroke_map.push((original.target.stroke_id.clone(), id.clone()));
            let mut backdrop = original.clone();
            retarget(&mut backdrop, id.clone());
            if let Presence::Present(ext) = &mut backdrop.vector_ext {
                ext.linked_fill_id = Presence::Present(
                    item.linked_fill_id
                        .clone()
                        .ok_or(SelectionError::ClipMismatch)?,
                );
            }
            shift_record(&mut backdrop, offset)?;
            copies.push(backdrop);
            paint_ids.push(id);
        }
        retarget(&mut copied, item.stroke_id.clone());
        if let Presence::Present(ext) = &mut copied.vector_ext {
            if let Some(link) = &item.linked_fill_id {
                ext.linked_fill_id = Presence::Present(link.clone());
            }
            if let Presence::Present(brush) = &mut ext.bitmap_brush {
                brush.brush_group_id = item
                    .brush_group_id
                    .clone()
                    .ok_or(SelectionError::ClipMismatch)?;
            }
        }
        shift_record(&mut copied, offset)?;
        copies.push(copied);
        paint_ids.push(item.stroke_id.clone());
        for dab in plan.dabs.iter().filter(|d| d.source == item.source) {
            let old_group = sources[item.source]
                .brush_group_id
                .as_deref()
                .ok_or(SelectionError::ClipMismatch)?;
            let original = rasters
                .iter()
                .find(|r| r.item.brush_group_id() == Some(old_group))
                .ok_or(SelectionError::ClipMismatch)?;
            let mut raster = original.clone();
            raster.target.context_id = context_id.into();
            raster.target.layer_uid = dest_uid.clone();
            raster.target.frame_scope = FrameScope {
                kind: FrameScopeKind::Authored,
                frame,
            };
            raster.target.stroke_id = dab.stroke_id.clone().ok_or(SelectionError::ClipMismatch)?;
            if !stroke_map
                .iter()
                .any(|(old, _)| old == &original.target.stroke_id)
            {
                stroke_map.push((
                    original.target.stroke_id.clone(),
                    raster.target.stroke_id.clone(),
                ));
            }
            raster.item = original
                .item
                .selection_companion_clone(&dab.brush_group_id, offset)
                .map_err(|_| SelectionError::ClipMismatch)?;
            paint_ids.push(raster.target.stroke_id.clone());
            raster_copies.push(raster);
        }
    }
    // All companion identities must exist before any stored wall reference is rewritten.
    // Both the filled backdrop and its primary may refer to any copied path or raster.
    for copied in &mut copies {
        if let Presence::Present(ext) = &mut copied.vector_ext {
            if let Presence::Present(trace) = &mut ext.fill_trace {
                if let Presence::Present(walls) = &mut trace.walls {
                    for wall in walls {
                        if let Some((_, new)) = stroke_map.iter().find(|(old, _)| old == wall) {
                            *wall = new.clone();
                        }
                    }
                }
            }
        }
    }
    let created = plan.items.iter().map(|i| i.stroke_id.clone()).collect();
    let mut copied_tags = tags.to_vec();
    let remapped = if let Some(groups) = source_groups {
        let mut group_taken = taken;
        group_taken.extend(destination_gids.iter().cloned());
        group_taken.extend(groups.iter().map(|group| group.gid.clone()));
        group_taken.extend(
            plan.items
                .iter()
                .filter_map(|item| item.linked_fill_id.clone()),
        );
        group_taken.extend(
            plan.items
                .iter()
                .filter_map(|item| item.brush_group_id.clone()),
        );
        let mut issuer = IdIssuer::new(ids, group_taken);
        remap_clone_groups(groups, &mut copied_tags, &stroke_map, &mut issuer)
            .map_err(SelectionError::Remap)?
    } else {
        Vec::new()
    };
    let entries: Vec<_> = remapped
        .iter()
        .filter(|group| group.kept)
        .map(|group| record_of(&group.entry))
        .collect();
    let members: Vec<_> = records
        .iter()
        .zip(copied_tags)
        .filter_map(|(record, tag)| {
            let gid = tag?;
            let (_, fresh) = stroke_map
                .iter()
                .find(|(old, _)| old == &record.target.stroke_id)?;
            Some(GroupMember {
                stroke_id: fresh.clone(),
                group_id: gid,
            })
        })
        .collect();
    let added: Vec<String> = entries.iter().map(|e| e.gid.clone()).collect();
    let mut next = work.clone();
    next.objects.extend(copies);
    compound::append_rasters(&mut next, raster_copies);
    append_elements(&mut next, context_id, &dest_uid, frame, &paint_ids);
    let mut created_groups = Vec::new();
    if !entries.is_empty() || !members.is_empty() {
        let mut table = match next.layers[dest_index].groups.value() {
            Some(t) if t.context_id != context_id => return Err(SelectionError::TableContext),
            Some(t) => t.clone(),
            None => GroupTable::empty(context_id),
        };
        table.entries.extend(entries);
        table.members.extend(members);
        let alive = layer_stroke_ids(&next.objects, &dest_uid, context_id)
            .into_iter()
            .collect();
        settle(&mut table.entries, &mut table.members, &alive);
        created_groups = added
            .into_iter()
            .filter(|id| table.entries.iter().any(|e| e.gid == *id))
            .collect();
        next.layers[dest_index].groups = Presence::Present(table);
    }
    compound::validate_document(&next)?;
    *work = next;
    Ok(Clones {
        created,
        created_groups,
    })
}
