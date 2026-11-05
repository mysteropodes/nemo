//! Cmd+G groups of one drawing layer: the baseline keeps `ld.groups[gid] = {name, combineMode, parent?, order}` on the layer
//! and the innermost `groupId` leaf tag on each stroke. The native layer carries one table of entries and leaf tags that the
//! codec checks as a tree; baseline quirks the tree cannot hold (dangling members, a group left with one member, a stale
//! parent) are repaired the way the baseline's own `dissolveGroup` does and reported.
use super::{Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub(super) fn table(
    path: &str,
    context: &str,
    groups: Option<&Value>,
    tags: &[(String, String)],
    strokes: &HashSet<String>,
    ctx: &mut Ctx,
) -> Result<Option<Value>, LegacyImportError> {
    let empty = Map::new();
    let map = match groups {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(map)) => map,
        Some(_) => {
            return Err(LegacyImportError::invalid(
                format!("{path}.groups"),
                "an object is required",
            ))
        }
    };
    if map.is_empty() && tags.is_empty() {
        return Ok(None);
    }
    let mut live: Vec<(String, Map<String, Value>, Vec<String>)> = Vec::new();
    for (gid, entry) in map {
        let members = super::object(entry, &format!("{path}.groups.{gid}"))?;
        let order = members
            .get("order")
            .and_then(Value::as_array)
            .map(|o| {
                o.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        live.push((gid.clone(), members.clone(), order));
    }
    // Dissolve groups until every group lists at least two existing members (leaf strokes or other live groups).
    loop {
        let names: HashSet<String> = live.iter().map(|g| g.0.clone()).collect();
        let mut changed = false;
        for group in &mut live {
            let before = group.2.len();
            group
                .2
                .retain(|m| strokes.contains(m) || (names.contains(m) && *m != group.0));
            if group.2.len() != before {
                ctx.warn(
                    format!("{path}.groups.{}", group.0),
                    "group_dangling_member",
                    "the group listed a shape that does not exist, so it was left out of the group",
                );
                changed = true;
            }
        }
        let before = live.len();
        live.retain(|g| {
            let keep = g.2.len() >= 2;
            if !keep {
                ctx.warn(
                    format!("{path}.groups.{}", g.0),
                    "group_dissolved",
                    "the group had fewer than two members, so it is not carried over",
                );
            }
            keep
        });
        if !changed && live.len() == before {
            break;
        }
    }
    let names: HashSet<String> = live.iter().map(|g| g.0.clone()).collect();
    let mut owner: HashMap<String, String> = HashMap::new();
    for group in &mut live {
        let gid = group.0.clone();
        group.2.retain(|member| {
            if !names.contains(member) {
                return true;
            }
            if owner.contains_key(member) {
                ctx.warn(
                    format!("{path}.groups.{gid}"),
                    "group_listed_twice",
                    "a group was listed by two groups, so only the first listing is kept",
                );
                return false;
            }
            owner.insert(member.clone(), gid.clone());
            true
        });
    }
    let mut entries = Vec::new();
    for (gid, members, order) in &live {
        let mut entry = json!({"gid": gid, "order": order});
        if let Some(name) = members.get("name").and_then(Value::as_str) {
            entry["name"] = json!(name);
        }
        if let Some(mode) = members.get("combineMode").and_then(Value::as_str) {
            entry["combineMode"] = json!(mode);
        }
        if let Some(parent) = owner.get(gid) {
            entry["parent"] = json!(parent);
        }
        entries.push(entry);
    }
    let lists = |gid: &str, stroke: &str| {
        live.iter()
            .any(|g| g.0 == gid && g.2.iter().any(|m| m == stroke))
    };
    let mut seen = HashSet::new();
    let mut members = Vec::new();
    for (stroke, gid) in tags {
        if !seen.insert(stroke.clone()) {
            continue;
        }
        if lists(gid, stroke) {
            members.push(json!({"strokeId": stroke, "groupId": gid}));
        } else {
            ctx.warn(format!("{path}.groups.{gid}"), "group_tag_unlisted", "a shape was tagged with a group that does not list it, so the tag is not carried over");
        }
    }
    if entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({"contextId": context, "entries": entries, "members": members}),
    ))
}
