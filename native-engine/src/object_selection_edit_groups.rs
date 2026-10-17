//! Group-table helpers of the selection-edit and clipboard plans (G24 join,
//! L02-H): the stored table (L02-G records) to and from the L02-A/L02-C kernel
//! entries, and the one rule that keeps a table admissible after clones or a
//! split took strokes along.
//!
//! The baseline leaves stale references behind (a recreated group whose
//! dropped child is still listed in its parent's `order`; a kept group whose
//! `order` names strokes that stayed on another layer). A stored table must
//! pass admission, so [`settle`] applies the baseline's own meaningless-combine
//! rule (`_remapCombineGroups`: fewer than two operands drops the group and
//! untags its members) until the table is consistent.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupEntry,
};
use crate::object_document::object_group_kernels::object_group_record::{
    GroupMember, GroupRecord, GroupTable,
};
use crate::object_document::Presence;

fn presence<T>(value: Option<T>) -> Presence<T> {
    match value {
        Some(value) => Presence::Present(value),
        None => Presence::Missing,
    }
}

/// Kernel entry to stored record (the inverse of [`entry_of`]).
pub(crate) fn record_of(entry: &GroupEntry) -> GroupRecord {
    GroupRecord {
        gid: entry.gid.clone(),
        name: presence(entry.name.clone()),
        combine_mode: presence(entry.combine_mode.map(|mode| mode.as_str().to_string())),
        parent: presence(entry.parent.clone()),
        order: entry.order.clone(),
    }
}

/// Stored record to kernel entry; `None` for a mode text outside the five the
/// baseline knows (admission refuses those, so `None` means admission was bypassed).
pub(crate) fn entry_of(record: &GroupRecord) -> Option<GroupEntry> {
    let combine_mode = match record.combine_mode.value() {
        Some(text) => Some(CombineMode::parse(text)?),
        None => None,
    };
    Some(GroupEntry {
        gid: record.gid.clone(),
        name: record.name.value().cloned(),
        combine_mode,
        parent: record.parent.value().cloned(),
        order: record.order.clone(),
    })
}

/// All entries of a table in table order, or `None` when one is unreadable.
pub(crate) fn entries_of(table: &GroupTable) -> Option<Vec<GroupEntry>> {
    table.entries.iter().map(entry_of).collect()
}

/// `data.groupId` of one stroke: the innermost group, if tagged.
pub(crate) fn tag_of(table: &GroupTable, stroke_id: &str) -> Option<String> {
    table
        .members
        .iter()
        .find(|member| member.stroke_id == stroke_id)
        .map(|member| member.group_id.clone())
}

/// Makes `entries` / `members` internally consistent against the strokes that
/// exist: order lists keep only live strokes and live groups; a group left with
/// fewer than two operands is dropped (repeatedly) and its members untagged; a
/// parent link survives only when the parent lists the child; a member survives
/// only when its group lists it. A table that was already consistent is returned
/// unchanged.
pub(crate) fn settle(
    entries: &mut Vec<GroupRecord>,
    members: &mut Vec<GroupMember>,
    strokes: &HashSet<String>,
) {
    loop {
        let gids: HashSet<String> = entries.iter().map(|entry| entry.gid.clone()).collect();
        for entry in entries.iter_mut() {
            entry
                .order
                .retain(|id| strokes.contains(id) || gids.contains(id));
        }
        let doomed: HashSet<String> = entries
            .iter()
            .filter(|entry| entry.order.len() < 2)
            .map(|entry| entry.gid.clone())
            .collect();
        if doomed.is_empty() {
            break;
        }
        entries.retain(|entry| !doomed.contains(&entry.gid));
        members.retain(|member| !doomed.contains(&member.group_id));
    }
    let listed: Vec<(String, Vec<String>)> = entries
        .iter()
        .map(|entry| (entry.gid.clone(), entry.order.clone()))
        .collect();
    for entry in entries.iter_mut() {
        let valid = entry.parent.value().is_some_and(|parent| {
            listed
                .iter()
                .any(|(gid, order)| gid == parent && order.contains(&entry.gid))
        });
        if !valid {
            entry.parent = Presence::Missing;
        }
    }
    members.retain(|member| {
        strokes.contains(&member.stroke_id)
            && listed
                .iter()
                .any(|(gid, order)| *gid == member.group_id && order.contains(&member.stroke_id))
    });
}
