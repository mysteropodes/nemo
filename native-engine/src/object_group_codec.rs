//! G24 (L02-G): admission of the per-layer group table. Pure checks over the
//! decoded table and the document's records; the single native codec
//! (object_codec.rs) calls `validate_group_table` once per layer that carries
//! a table and stays byte-identical for every layer without one.
//!
//! The rules make the L02-A kernel total: every reference resolves, the
//! parent links and the `order` lists describe one tree (no cycles, at most 17
//! levels, the depth the member walk visits), no group has fewer than two
//! entries, and every leaf tag names a group that lists the stroke. Baseline
//! quirks that the kernel preserves stay admitted: a stroke may be listed by
//! several groups (`combineSelection` does not detach) and so even twice by
//! one group, and a combine group carries no parent. Nothing is repaired or defaulted; each refusal is typed.
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::object_group_kernels::object_group_hierarchy::CombineMode;
use crate::object_document::object_group_kernels::object_group_record::{
    layer_stroke_ids, GroupTable,
};
use crate::object_document::ObjectRecord;
use std::collections::{HashMap, HashSet};

/// Ancestors above an entry; the member walk of the kernel visits depth 0..=16.
const MAX_ANCESTORS: usize = 16;

fn invalid(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
}

/// Admit the table of layer `layer_uid`. `complete_contexts` is the caller's
/// v2 flag: a v1 document only has the `scene-root` context.
pub(crate) fn validate_group_table(
    layer_uid: &str,
    table: &GroupTable,
    objects: &[ObjectRecord],
    complete_contexts: bool,
) -> Result<(), ObjectCodecError> {
    if table.context_id.is_empty() {
        return Err(invalid("group table requires a contextId"));
    }
    if !complete_contexts && table.context_id != "scene-root" {
        return Err(invalid("a v1 group table must name the scene-root context"));
    }
    let stroke_list = layer_stroke_ids(objects, layer_uid, &table.context_id);
    let strokes: HashSet<&str> = stroke_list.iter().map(String::as_str).collect();
    let mut slot: HashMap<&str, usize> = HashMap::new();
    for (at, entry) in table.entries.iter().enumerate() {
        if entry.gid.is_empty() {
            return Err(invalid("empty group id"));
        }
        if slot.insert(entry.gid.as_str(), at).is_some() {
            return Err(ObjectCodecError::new(
                ObjectCodecErrorKind::DuplicateId,
                "duplicate group id",
            ));
        }
        if strokes.contains(entry.gid.as_str()) {
            return Err(invalid("group id collides with a stroke id"));
        }
        if let Some(mode) = entry.combine_mode.value() {
            if CombineMode::parse(mode).is_none() {
                return Err(invalid("unknown group combineMode"));
            }
        }
    }
    // child gid -> the one entry whose order lists it
    let mut listed_by: HashMap<&str, &str> = HashMap::new();
    for entry in &table.entries {
        if entry.order.len() < 2 {
            return Err(invalid("a group lists at least two members"));
        }
        // A repeated STROKE id stays admitted: dissolving a group whose leaf a
        // second group also lists promotes the leaf into an order that has it.
        let mut child_groups: HashSet<&str> = HashSet::new();
        for member in &entry.order {
            if slot.contains_key(member.as_str()) {
                if member == &entry.gid {
                    return Err(invalid("a group lists itself"));
                }
                if !child_groups.insert(member.as_str()) {
                    return Err(invalid("a group lists a child group twice"));
                }
                if listed_by
                    .insert(member.as_str(), entry.gid.as_str())
                    .is_some()
                {
                    return Err(invalid("a group is listed by two groups"));
                }
            } else if !strokes.contains(member.as_str()) {
                return Err(invalid("group member is not a stroke of the layer context"));
            }
        }
    }
    for entry in &table.entries {
        let owner = listed_by.get(entry.gid.as_str()).copied();
        match (entry.parent.value(), owner) {
            (None, None) => {}
            (Some(parent), Some(owner)) if parent.as_str() == owner => {}
            (Some(_), _) => return Err(invalid("group parent is not the group that lists it")),
            (None, Some(_)) => return Err(invalid("a listed group must name its parent")),
        }
    }
    for entry in &table.entries {
        let mut up = entry.parent.value().map(String::as_str);
        let mut levels = 0;
        while let Some(parent) = up {
            levels += 1;
            if levels > MAX_ANCESTORS {
                return Err(invalid("group nesting is deeper than 16 levels or cyclic"));
            }
            up = slot
                .get(parent)
                .and_then(|&at| table.entries[at].parent.value())
                .map(String::as_str);
        }
    }
    let mut tagged: HashSet<&str> = HashSet::new();
    for member in &table.members {
        if !tagged.insert(member.stroke_id.as_str()) {
            return Err(invalid("a stroke carries one group tag"));
        }
        if !strokes.contains(member.stroke_id.as_str()) {
            return Err(invalid(
                "group tag names a stroke outside the layer context",
            ));
        }
        let lists = slot
            .get(member.group_id.as_str())
            .is_some_and(|&at| table.entries[at].order.contains(&member.stroke_id));
        if !lists {
            return Err(invalid(
                "group tag names a group that does not list the stroke",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_group_ops_admission.rs"]
mod object_group_ops_admission;
#[cfg(test)]
#[path = "../tests/object_group_ops_admission_limits.rs"]
mod object_group_ops_admission_limits;
