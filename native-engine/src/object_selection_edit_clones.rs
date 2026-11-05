//! Clone application shared by duplicate-selection and paste (F15, L02-H): the
//! fresh-identity source named after the publishing revision, the coordinate
//! shift of a copy, and the append of the remapped copies, their order elements
//! and their recreated combine groups to one destination cell.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_clipboard_remap::{
    plan_clone, CloneRequest, IdKind, IdSource, ItemSource,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;
use crate::object_document::object_group_kernels::object_group_record::{
    layer_stroke_ids, GroupMember, GroupTable,
};
use crate::object_document::object_group_kernels::object_selection_edit_frames::append_elements;
use crate::object_document::object_group_kernels::object_selection_edit_groups::{
    record_of, settle,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::SelectionError;
use crate::object_document::{
    FrameScope, FrameScopeKind, ObjectDocument, ObjectPoint, ObjectRecord, Presence,
};
use serde_json::Number;

/// Fresh identities named after the revision the edit will publish, like the
/// layer-create / duplicate identities: `native-stroke:{revision}:{n}` and
/// `native-group:{revision}:{n}`, skipping every id already in the document.
#[derive(Debug, Clone)]
pub(crate) struct RevisionIds {
    revision: u64,
    taken: HashSet<String>,
    strokes: u64,
    groups: u64,
    links: u64,
    brushes: u64,
}

impl RevisionIds {
    pub(crate) fn new(revision: u64, document: &ObjectDocument) -> Self {
        let mut taken: HashSet<String> = HashSet::new();
        for record in &document.objects {
            taken.insert(record.target.stroke_id.clone());
            if let Some(ext) = record.vector_ext.value() {
                if let Some(id) = ext.linked_fill_id.value() {
                    taken.insert(id.clone());
                }
                if let Some(brush) = ext.bitmap_brush.value() {
                    taken.insert(brush.brush_group_id.clone());
                }
            }
        }
        if let Some(items) = document.items.value() {
            taken.extend(items.targets().map(|t| t.stroke_id.clone()));
            for raster in items.raster_entries() {
                if let Some(id) = raster.item.brush_group_id() {
                    taken.insert(id.to_string());
                }
            }
        }
        for layer in &document.layers {
            if let Some(table) = layer.groups.value() {
                for entry in &table.entries {
                    taken.insert(entry.gid.clone());
                }
            }
        }
        Self {
            revision,
            taken,
            strokes: 0,
            groups: 0,
            links: 0,
            brushes: 0,
        }
    }
}

impl IdSource for RevisionIds {
    fn fresh(&mut self, kind: IdKind, _replaces: Option<&str>) -> Option<String> {
        let (prefix, counter) = match kind {
            IdKind::Stroke => ("native-stroke", &mut self.strokes),
            IdKind::CombineGroup => ("native-group", &mut self.groups),
            IdKind::LinkedFill => ("native-linked-fill", &mut self.links),
            IdKind::BrushGroup => ("native-brush-group", &mut self.brushes),
            IdKind::Mesh => return None,
        };
        loop {
            let candidate = format!("{prefix}:{}:{}", self.revision, *counter);
            *counter += 1;
            if self.taken.insert(candidate.clone()) {
                return Some(candidate);
            }
        }
    }
}

fn shift_point(point: &mut ObjectPoint, offset: [f64; 2]) -> Result<(), SelectionError> {
    for (axis, delta) in [(&mut point.x, offset[0]), (&mut point.y, offset[1])] {
        let moved = axis
            .as_f64()
            .map(|value| value + delta)
            .and_then(Number::from_f64)
            .ok_or(SelectionError::NonFiniteCoordinate)?;
        *axis = moved;
    }
    Ok(())
}

/// `clone.translate(Point(dx, dy))` (the baseline nudge is `[12, 12]`; an Alt-drag duplicate passes the drag) plus the explicit centerline shift
/// `_materializeClones` gives a vector brush. Nothing else moves, as in the
/// baseline (a `paramShape` box and `boxAngle` stay as authored). An offset of
/// zero leaves every number untouched.
pub(crate) fn shift_record(
    record: &mut ObjectRecord,
    offset: [f64; 2],
) -> Result<(), SelectionError> {
    if offset == [0.0, 0.0] {
        return Ok(());
    }
    for segment in &mut record.geometry.segments {
        shift_point(&mut segment.point, offset)?;
    }
    if let Presence::Present(ext) = &mut record.vector_ext {
        if let Presence::Present(trace) = &mut ext.fill_trace {
            for seed in trace
                .seed
                .value()
                .into_iter()
                .chain(trace.seeds.value().into_iter().flatten())
            {
                if seed.iter().enumerate().any(|(at, n)| {
                    n.as_f64()
                        .and_then(|v| Number::from_f64(v + offset[at % 2]))
                        .is_none()
                }) {
                    return Err(SelectionError::NonFiniteCoordinate);
                }
            }
            if let Presence::Present(seed) = &mut trace.seed {
                for (at, n) in seed.iter_mut().enumerate() {
                    *n = Number::from_f64(n.as_f64().unwrap() + offset[at % 2]).unwrap();
                }
            }
            if let Presence::Present(seeds) = &mut trace.seeds {
                for seed in seeds {
                    for (at, n) in seed.iter_mut().enumerate() {
                        *n = Number::from_f64(n.as_f64().unwrap() + offset[at % 2]).unwrap();
                    }
                }
            }
        }
        if let Presence::Present(pressure) = &mut ext.pressure {
            if let Presence::Present(center) = &mut pressure.center_segments {
                for segment in center.iter_mut() {
                    shift_point(&mut segment.point, offset)?;
                }
            }
        }
    }
    Ok(())
}

/// What the ordinary Select move does to the rest of a dynamic or gradient-filled copy (baseline select-bridge onDown ~1186-1207
/// clones in place and then runs the ordinary move, which carries the `paramShape.box` frame and the gradient end points with the
/// segments): the same translation applied to those, after [`shift_record`] moved the geometry. Only an Alt-drag duplicate asks
/// for it; the Cmd+D nudge stays the baseline `clone.translate`.
pub(crate) fn shift_frame_and_gradient(
    record: &mut ObjectRecord,
    offset: [f64; 2],
) -> Result<(), SelectionError> {
    let Presence::Present(ext) = &mut record.vector_ext else {
        return Ok(());
    };
    let moved = |n: &Number, delta: f64| -> Result<Number, SelectionError> {
        n.as_f64()
            .and_then(|v| Number::from_f64(v + delta))
            .ok_or(SelectionError::NonFiniteCoordinate)
    };
    if let Presence::Present(shape) = &mut ext.param_shape {
        if let Presence::Present(frame) = &mut shape.frame {
            (frame.x1, frame.y1) = (moved(&frame.x1, offset[0])?, moved(&frame.y1, offset[1])?);
            (frame.x2, frame.y2) = (moved(&frame.x2, offset[0])?, moved(&frame.y2, offset[1])?);
        }
    }
    if let Presence::Present(gradient) = &mut ext.fill_gradient {
        for end in [&mut gradient.from, &mut gradient.to] {
            *end = [moved(&end[0], offset[0])?, moved(&end[1], offset[1])?];
        }
    }
    Ok(())
}

/// Clones of the selected records join the top of the destination cell.
pub(crate) struct Clones {
    pub(crate) created: Vec<String>,
    pub(crate) created_groups: Vec<String>,
}

/// The shared tail of duplicate and paste: remap through L02-C, append the
/// moved copies and their order elements, recreate the combine groups.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_clones(
    work: &mut ObjectDocument,
    dest_index: usize,
    context_id: &str,
    frame: u32,
    records: &[ObjectRecord],
    tags: &[Option<String>],
    source_groups: Option<&[GroupEntry]>,
    offset: [f64; 2],
    ids: &mut dyn IdSource,
) -> Result<Clones, SelectionError> {
    if !offset.iter().all(|v| v.is_finite()) {
        return Err(SelectionError::NonFiniteCoordinate);
    }
    let sources: Vec<ItemSource> = records
        .iter()
        .zip(tags)
        .map(|(record, tag)| ItemSource {
            stroke_id: Some(record.target.stroke_id.clone()),
            group_id: tag.clone(),
            ..ItemSource::default()
        })
        .collect();
    let destination_gids: Vec<String> = match work.layers[dest_index].groups.value() {
        Some(table) => table
            .entries
            .iter()
            .map(|entry| entry.gid.clone())
            .collect(),
        None => Vec::new(),
    };
    let request = CloneRequest {
        items: &sources,
        source_groups,
        destination_group_ids: &destination_gids,
        offset: offset[0],
    };
    let plan = plan_clone(&request, ids).map_err(SelectionError::Remap)?;
    let dest_uid = work.layers[dest_index].layer_uid.clone();
    let mut copies: Vec<ObjectRecord> = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let mut record = records[item.source].clone();
        record.target.context_id = context_id.to_string();
        record.target.layer_uid = dest_uid.clone();
        record.target.frame_scope = FrameScope {
            kind: FrameScopeKind::Authored,
            frame,
        };
        record.target.stroke_id = item.stroke_id.clone();
        if let Presence::Present(ext) = &mut record.vector_ext {
            if let Presence::Present(trace) = &mut ext.fill_trace {
                if let Presence::Present(walls) = &mut trace.walls {
                    for wall in walls {
                        if let Some(copy) = plan
                            .items
                            .iter()
                            .find(|candidate| records[candidate.source].target.stroke_id == *wall)
                        {
                            *wall = copy.stroke_id.clone();
                        }
                    }
                }
            }
        }
        shift_record(&mut record, offset)?;
        copies.push(record);
    }
    let created: Vec<String> = plan
        .items
        .iter()
        .map(|item| item.stroke_id.clone())
        .collect();
    let entries: Vec<_> = plan
        .groups
        .iter()
        .filter(|remap| remap.kept)
        .map(|remap| record_of(&remap.entry))
        .collect();
    let members: Vec<GroupMember> = plan
        .items
        .iter()
        .filter_map(|item| {
            item.group_id.as_ref().map(|gid| GroupMember {
                stroke_id: item.stroke_id.clone(),
                group_id: gid.clone(),
            })
        })
        .collect();
    let added: Vec<String> = entries.iter().map(|entry| entry.gid.clone()).collect();
    let wants_table = !entries.is_empty() || !members.is_empty();
    let mut table = match work.layers[dest_index].groups.value() {
        Some(table) if table.context_id != context_id && wants_table => {
            return Err(SelectionError::TableContext)
        }
        Some(table) => table.clone(),
        None => GroupTable::empty(context_id),
    };
    work.objects.extend(copies);
    append_elements(work, context_id, &dest_uid, frame, &created);
    let mut surviving: Vec<String> = Vec::new();
    if wants_table {
        table.entries.extend(entries);
        table.members.extend(members);
        let alive: HashSet<String> = layer_stroke_ids(&work.objects, &dest_uid, context_id)
            .into_iter()
            .collect();
        settle(&mut table.entries, &mut table.members, &alive);
        surviving = added
            .into_iter()
            .filter(|gid| table.entries.iter().any(|entry| entry.gid == *gid))
            .collect();
        work.layers[dest_index].groups = Presence::Present(table);
    }
    Ok(Clones {
        created,
        created_groups: surviving,
    })
}
