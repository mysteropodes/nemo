//! Combine-group recreation for cloned items (`_remapCombineGroups`, L02-C).
//!
//! Only members that came along survive in a recreated group's `order`;
//! nested groups are recreated recursively and so are the ancestors of every
//! tagged clone; a recreated group left with fewer than two operands is
//! dropped and its clones untagged; a tag whose group is absent from the
//! source table is deleted. Original gids are never reused: a fresh gid
//! that is already taken is refused by the issuer.
use crate::object_document::object_group_kernels::object_clipboard_remap::{
    live, lookup, IdIssuer, IdKind, IdSource, RemapError,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;

/// One recreated group. `entry.parent` is re-pointed at the fresh id of the
/// recreated parent (the baseline copied the SOURCE parent gid verbatim,
/// which leaves a stale link; see HANDOFF). `kept == false` means the group
/// fell below two operands and must not be inserted (its id was still consumed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupRemap {
    pub(crate) source: String,
    pub(crate) fresh: String,
    pub(crate) kept: bool,
    pub(crate) entry: GroupEntry,
}

struct Ctx<'t> {
    table: &'t [GroupEntry],
    sids: &'t [(String, String)],
}

fn find<'t>(table: &'t [GroupEntry], gid: &str) -> Option<&'t GroupEntry> {
    table.iter().find(|entry| entry.gid.as_str() == gid)
}

/// `parentOf`: the first table entry (insertion order) whose `order` lists `gid`.
fn parent_of<'t>(table: &'t [GroupEntry], gid: &str) -> Option<&'t str> {
    table
        .iter()
        .find(|entry| entry.order.iter().any(|member| member.as_str() == gid))
        .map(|entry| entry.gid.as_str())
}

/// Input checks that need no ids: a well-formed table and acyclic ancestor
/// chains for every tag that names a group (the baseline loops forever on a
/// cyclic chain, a self-listing group included).
pub(crate) fn validate(table: &[GroupEntry], tags: &[Option<&str>]) -> Result<(), RemapError> {
    for (index, entry) in table.iter().enumerate() {
        if entry.gid.is_empty() {
            return Err(RemapError::EmptyGroupId);
        }
        if table[..index].iter().any(|prior| prior.gid == entry.gid) {
            return Err(RemapError::DuplicateGroupId(entry.gid.clone()));
        }
    }
    for tag in tags.iter().flatten() {
        if find(table, tag).is_none() {
            continue;
        }
        let mut seen: Vec<&str> = vec![*tag];
        let mut up = parent_of(table, tag);
        while let Some(parent) = up {
            if seen.contains(&parent) {
                return Err(RemapError::GroupCycle(parent.to_string()));
            }
            seen.push(parent);
            up = parent_of(table, parent);
        }
    }
    Ok(())
}

/// `fresh(oldGid)`: the fresh id is recorded BEFORE the members are walked (so a
/// group listing itself terminates), the finished entry is pushed AFTER (so the
/// new table's insertion order puts children before parents, as in the baseline).
fn fresh<S: IdSource + ?Sized>(
    ctx: &Ctx<'_>,
    issuer: &mut IdIssuer<'_, S>,
    created: &mut Vec<(String, String)>,
    done: &mut Vec<GroupRemap>,
    old: &str,
) -> Result<String, RemapError> {
    if let Some(existing) = lookup(created, old) {
        return Ok(existing.to_string());
    }
    let entry = find(ctx.table, old).ok_or_else(|| RemapError::UnknownGroup(old.to_string()))?;
    let fresh_gid = issuer.issue(IdKind::CombineGroup, Some(old))?;
    created.push((old.to_string(), fresh_gid.clone()));
    let mut order: Vec<String> = Vec::new();
    for member in &entry.order {
        if find(ctx.table, member).is_some() {
            order.push(fresh(ctx, issuer, created, done, member)?);
        } else if let Some(new) = lookup(ctx.sids, member) {
            order.push(new.to_string());
        }
    }
    done.push(GroupRemap {
        source: old.to_string(),
        fresh: fresh_gid.clone(),
        kept: true,
        entry: GroupEntry {
            gid: fresh_gid.clone(),
            name: entry.name.clone(),
            combine_mode: entry.combine_mode,
            parent: None,
            order,
        },
    });
    Ok(fresh_gid)
}

/// Rewrites `tags` (one per clone, items then dabs) in place and returns the
/// recreated groups. `sids` maps old stroke id to the clone's fresh id.
pub(crate) fn remap<S: IdSource + ?Sized>(
    table: &[GroupEntry],
    tags: &mut [Option<String>],
    sids: &[(String, String)],
    issuer: &mut IdIssuer<'_, S>,
) -> Result<Vec<GroupRemap>, RemapError> {
    let ctx = Ctx { table, sids };
    let mut created: Vec<(String, String)> = Vec::new();
    let mut done: Vec<GroupRemap> = Vec::new();
    for tag in tags.iter_mut() {
        let old = match tag.as_deref() {
            Some(value) => value.to_string(),
            None => continue,
        };
        if find(table, &old).is_none() {
            *tag = None;
            continue;
        }
        let renamed = fresh(&ctx, issuer, &mut created, &mut done, &old)?;
        *tag = Some(renamed);
        let mut up = parent_of(table, &old).map(|value| value.to_string());
        while let Some(parent) = up {
            fresh(&ctx, issuer, &mut created, &mut done, &parent)?;
            up = parent_of(table, &parent).map(|value| value.to_string());
        }
    }
    for recreated in done.iter_mut() {
        let old_parent = find(table, &recreated.source).and_then(|entry| live(&entry.parent));
        recreated.entry.parent = old_parent
            .and_then(|parent| lookup(&created, parent))
            .map(|value| value.to_string());
    }
    for (_, fresh_gid) in &created {
        let at = match done.iter().position(|item| item.fresh == *fresh_gid) {
            Some(at) => at,
            None => continue,
        };
        if done[at].entry.order.len() < 2 {
            done[at].kept = false;
            for tag in tags.iter_mut() {
                if tag.as_deref() == Some(fresh_gid.as_str()) {
                    *tag = None;
                }
            }
        }
    }
    Ok(done)
}
