//! G24 (L02-G): storage DTO of the per-layer group table (`ld.groups` plus the
//! innermost `data.groupId` leaf tags of `src/js/group-bridge.js` at fbe4b8a3).
//!
//! Pure data and strict wire shape. Admission lives in `object_group_codec.rs`,
//! the edit rules in the L02-A kernel and the planning in `object_group_plan.rs`;
//! this table is one more member of `ObjectLayer`, not a second document.
//! Absent, `null` and explicit values stay distinct through `Presence`: a layer
//! with no `groups` member never had a table, a table with empty `entries` is an
//! authored empty one, and `null` is refused. Leaf tags are stored as an explicit
//! `members` list because the baseline keeps them on the stroke, not in the table
//! (a record-level tag would touch every `ObjectRecord` consumer).
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupEntry, GroupLayer, LayerItem,
};
use crate::object_document::{object_deserialize, FrameScopeKind, ObjectRecord, Presence};
use serde::{Deserialize, Serialize};

/// One layer's group table, scoped to ONE native context.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GroupTable {
    /// Every stroke id below is a stroke of this context on this layer.
    pub(crate) context_id: String,
    /// Table insertion order (the JS key order of `ld.groups`).
    pub(crate) entries: Vec<GroupRecord>,
    /// Leaf tags in layer order; a stroke without an entry here is loose.
    pub(crate) members: Vec<GroupMember>,
}

/// One `ld.groups[gid]` entry. Absent keys stay absent: a legacy entry has no
/// `combineMode` and reads as `none`, which is not the same as an explicit `none`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GroupRecord {
    pub(crate) gid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) combine_mode: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) parent: Presence<String>,
    /// Leaf stroke ids and child gids, bottom to top.
    pub(crate) order: Vec<String>,
}

/// `data.groupId` of one stroke: the INNERMOST group, never an ancestor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GroupMember {
    pub(crate) stroke_id: String,
    pub(crate) group_id: String,
}

object_deserialize!(GroupTable {
    context_id: String,
    entries: Vec<GroupRecord>,
    members: Vec<GroupMember>
});
object_deserialize!(GroupRecord {
    gid: String,
    #[serde(default)] name: Presence<String>,
    #[serde(default)] combine_mode: Presence<String>,
    #[serde(default)] parent: Presence<String>,
    order: Vec<String>
});
object_deserialize!(GroupMember {
    stroke_id: String,
    group_id: String
});

fn presence<T>(value: Option<T>) -> Presence<T> {
    match value {
        Some(value) => Presence::Present(value),
        None => Presence::Missing,
    }
}

/// The distinct stroke ids of one layer in one context in document order
/// (first occurrence wins), authored scopes only: the layer's natural z-order.
pub(crate) fn layer_stroke_ids(
    objects: &[ObjectRecord],
    layer_uid: &str,
    context_id: &str,
) -> Vec<String> {
    let mut strokes: Vec<String> = Vec::new();
    for record in objects {
        let target = &record.target;
        if target.context_id == context_id
            && target.layer_uid == layer_uid
            && target.frame_scope.kind == FrameScopeKind::Authored
            && !strokes.contains(&target.stroke_id)
        {
            strokes.push(target.stroke_id.clone());
        }
    }
    strokes
}

impl GroupTable {
    /// An authored table with no group; the kernel view of a layer that has none.
    pub(crate) fn empty(context_id: &str) -> Self {
        Self {
            context_id: context_id.to_string(),
            entries: Vec::new(),
            members: Vec::new(),
        }
    }

    /// The L02-A kernel view over this table and the layer's strokes (z-order).
    /// `None` when the stored table cannot form a kernel layer (an unknown mode
    /// text, a duplicate or empty id): admission refuses all of those, so a
    /// `None` here means admission was bypassed.
    pub(crate) fn to_layer(&self, strokes: &[String]) -> Option<GroupLayer> {
        let mut entries = Vec::with_capacity(self.entries.len());
        for record in &self.entries {
            let combine_mode = match record.combine_mode.value() {
                Some(text) => Some(CombineMode::parse(text)?),
                None => None,
            };
            entries.push(GroupEntry {
                gid: record.gid.clone(),
                name: record.name.value().cloned(),
                combine_mode,
                parent: record.parent.value().cloned(),
                order: record.order.clone(),
            });
        }
        let items: Vec<LayerItem> = strokes
            .iter()
            .map(|stroke_id| LayerItem {
                stroke_id: stroke_id.clone(),
                dup_index: None,
                group_id: self
                    .members
                    .iter()
                    .find(|member| member.stroke_id == *stroke_id)
                    .map(|member| member.group_id.clone()),
            })
            .collect();
        GroupLayer::new(entries, items).ok()
    }

    /// The canonical table of an edited kernel layer: entries in table order,
    /// members in layer order, empty tags dropped.
    pub(crate) fn from_layer(context_id: &str, layer: &GroupLayer) -> Self {
        let entries = layer
            .entries()
            .iter()
            .map(|entry| GroupRecord {
                gid: entry.gid.clone(),
                name: presence(entry.name.clone()),
                combine_mode: presence(entry.combine_mode.map(|mode| mode.as_str().to_string())),
                parent: presence(entry.parent.clone()),
                order: entry.order.clone(),
            })
            .collect();
        let members = layer
            .items()
            .iter()
            .filter_map(|item| {
                item.group_id
                    .as_ref()
                    .filter(|gid| !gid.is_empty())
                    .map(|gid| GroupMember {
                        stroke_id: item.stroke_id.clone(),
                        group_id: gid.clone(),
                    })
            })
            .collect();
        Self {
            context_id: context_id.to_string(),
            entries,
            members,
        }
    }
}
