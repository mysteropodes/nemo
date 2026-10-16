//! Per-layer group hierarchy kernel (G24, L02-A): the pure data rules behind
//! group, ungroup, dissolve, detach, remove-member, rename and combine-mode.
//!
//! Mirrors `ld.groups` plus the leaf `data.groupId` tag of `src/js/group-bridge.js`
//! at fbe4b8a3. A leaf tag always names the INNERMOST group; the upward link is
//! `parent` on the child entry. The kernel owns no document, history or id
//! source: the caller supplies ids and names, and every refusal is decided
//! before the first write so a rejected call leaves the layer untouched.
use std::fmt;

#[path = "object_group_hierarchy_selection.rs"]
mod selection;

/// `resolveGroupMembers` / `collectGroupStrokeIds` stop below this depth.
const MEMBER_DEPTH_CAP: usize = 16;
/// `rootGroupOf` / the fully-selected ancestor walk visit at most this many groups.
const ROOT_DEPTH_CAP: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombineMode {
    None,
    Unite,
    Subtract,
    Intersect,
    Exclude,
}

impl CombineMode {
    pub(crate) fn parse(text: &str) -> Option<CombineMode> {
        match text {
            "none" => Some(CombineMode::None),
            "unite" => Some(CombineMode::Unite),
            "subtract" => Some(CombineMode::Subtract),
            "intersect" => Some(CombineMode::Intersect),
            "exclude" => Some(CombineMode::Exclude),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CombineMode::None => "none",
            CombineMode::Unite => "unite",
            CombineMode::Subtract => "subtract",
            CombineMode::Intersect => "intersect",
            CombineMode::Exclude => "exclude",
        }
    }
}

/// One `ld.groups[gid]` entry. `None` fields are ABSENT keys, distinct from
/// an explicit value (a legacy entry has no `combineMode`; reads default to none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupEntry {
    pub(crate) gid: String,
    pub(crate) name: Option<String>,
    pub(crate) combine_mode: Option<CombineMode>,
    pub(crate) parent: Option<String>,
    /// Leaf stroke ids and child gids, bottom to top.
    pub(crate) order: Vec<String>,
}

/// One layer child in z-order (index 0 = bottom). `dup_index` is the mograph
/// duplicator copy (absent and 0 are the same bucket on lookup).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerItem {
    pub(crate) stroke_id: String,
    pub(crate) dup_index: Option<u32>,
    pub(crate) group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupError {
    EmptyGroupId,
    DuplicateGroupId(String),
    EmptyStrokeId(usize),
    UnknownGroup(String),
    UnknownItem(usize),
    DuplicateSelection(usize),
    /// Fewer than two selected shapes, or fewer than two grouping units.
    FewerThanTwoUnits(usize),
    EmptySelection,
    NoGroupInSelection,
    NotInGroup(usize),
}

impl fmt::Display for GroupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenameOutcome {
    pub(crate) created: bool,
    pub(crate) changed: bool,
}

/// The group table (insertion order = JS key order) and the layer's children.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GroupLayer {
    entries: Vec<GroupEntry>,
    items: Vec<LayerItem>,
}

fn live(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|value| !value.is_empty())
}

impl GroupLayer {
    pub(crate) fn new(entries: Vec<GroupEntry>, items: Vec<LayerItem>) -> Result<Self, GroupError> {
        for (n, entry) in entries.iter().enumerate() {
            if entry.gid.is_empty() {
                return Err(GroupError::EmptyGroupId);
            }
            if entries[..n].iter().any(|prior| prior.gid == entry.gid) {
                return Err(GroupError::DuplicateGroupId(entry.gid.clone()));
            }
        }
        if let Some(bad) = items.iter().position(|item| item.stroke_id.is_empty()) {
            return Err(GroupError::EmptyStrokeId(bad));
        }
        Ok(GroupLayer { entries, items })
    }

    pub(crate) fn entries(&self) -> &[GroupEntry] {
        &self.entries
    }

    pub(crate) fn items(&self) -> &[LayerItem] {
        &self.items
    }

    pub(crate) fn entry(&self, gid: &str) -> Option<&GroupEntry> {
        self.entries.iter().find(|entry| entry.gid == gid)
    }

    fn slot(&self, gid: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.gid == gid)
    }

    /// `findByStrokeId`: first item with this stroke id in the duplicator bucket.
    fn find_item(&self, stroke_id: &str, dup: Option<u32>) -> Option<usize> {
        self.items.iter().position(|item| {
            item.stroke_id == stroke_id && item.dup_index.unwrap_or(0) == dup.unwrap_or(0)
        })
    }

    fn tag_of(&self, item: usize) -> Option<&str> {
        self.items.get(item).and_then(|found| live(&found.group_id))
    }

    /// `parentGroupOf`: a parent that names no entry is ignored.
    pub(crate) fn parent_group_of(&self, gid: &str) -> Option<String> {
        let parent = live(&self.entry(gid)?.parent)?;
        self.entry(parent).map(|_| parent.to_string())
    }

    /// `rootGroupOf`: depth cap 32; a cycle stops at the repeated gid.
    pub(crate) fn root_group_of(&self, gid: &str) -> String {
        let mut cur = gid.to_string();
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..ROOT_DEPTH_CAP {
            if cur.is_empty() || seen.contains(&cur) {
                return cur;
            }
            seen.push(cur.clone());
            match self.parent_group_of(&cur) {
                Some(up) => cur = up,
                None => return cur,
            }
        }
        cur
    }

    /// `rootGroupOfItem`: a tag naming no entry is returned as is.
    pub(crate) fn root_group_of_item(&self, item: usize) -> Option<String> {
        let tag = self.tag_of(item)?;
        match self.entry(tag) {
            Some(_) => Some(self.root_group_of(tag)),
            None => Some(tag.to_string()),
        }
    }

    /// `collectGroupStrokeIds`: leaf ids only; depth cap 16; true cycles add nothing.
    pub(crate) fn collect_group_stroke_ids(&self, gid: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.walk(gid, &mut Vec::new(), 0, &mut |leaf| {
            out.push(leaf.to_string())
        });
        out
    }

    /// `resolveGroupMembers`: item indices of one duplicator copy, depth first.
    pub(crate) fn resolve_group_members(&self, gid: &str, dup: Option<u32>) -> Vec<usize> {
        let mut out = Vec::new();
        self.walk(gid, &mut Vec::new(), 0, &mut |leaf| {
            if let Some(found) = self.find_item(leaf, dup) {
                out.push(found);
            }
        });
        out
    }

    // Per-branch ancestor stack: the same sub-group in two branches is legal.
    fn walk(
        &self,
        gid: &str,
        ancestors: &mut Vec<String>,
        depth: usize,
        leaf: &mut dyn FnMut(&str),
    ) {
        if depth > MEMBER_DEPTH_CAP || ancestors.iter().any(|seen| seen == gid) {
            return;
        }
        let Some(group) = self.entry(gid) else {
            return;
        };
        ancestors.push(gid.to_string());
        for member in &group.order {
            if self.entry(member).is_some() {
                self.walk(member, ancestors, depth + 1, leaf);
            } else {
                leaf(member.as_str());
            }
        }
        ancestors.pop();
    }

    /// `membersOf`: the outermost group's leaves, else every item sharing the tag.
    pub(crate) fn members_of(&self, item: usize) -> Vec<usize> {
        let Some(tag) = self.tag_of(item) else {
            return if item < self.items.len() {
                vec![item]
            } else {
                Vec::new()
            };
        };
        if let Some(root) = self.root_group_of_item(item) {
            if self.entry(&root).is_some() {
                let leaves = self.resolve_group_members(&root, None);
                if !leaves.is_empty() {
                    return leaves;
                }
            }
        }
        let wanted = tag.to_string();
        (0..self.items.len())
            .filter(|&at| self.tag_of(at) == Some(wanted.as_str()))
            .collect()
    }

    /// `dissolveGroup`: ONE level. Leaves and child groups are promoted to the
    /// grandparent (or top level) at the dissolved gid's slot in its order.
    pub(crate) fn dissolve_group(&mut self, gid: &str) -> Result<(), GroupError> {
        let Some(at) = self.slot(gid) else {
            return Err(GroupError::UnknownGroup(gid.to_string()));
        };
        let up = self.parent_group_of(gid);
        let order = self.entries[at].order.clone();
        for member in &order {
            if let Some(child) = self.slot(member) {
                self.entries[child].parent = up.clone();
            } else if let Some(leaf) = self.find_item(member, None) {
                self.items[leaf].group_id = up.clone();
            }
        }
        if let Some(parent) = up.as_deref().and_then(|p| self.slot(p)) {
            let list = &mut self.entries[parent].order;
            if let Some(ix) = list.iter().position(|entry| entry == gid) {
                list.remove(ix);
                for (offset, member) in order.iter().enumerate() {
                    list.insert(ix + offset, member.clone());
                }
            }
        }
        self.entries.retain(|entry| entry.gid != gid);
        Ok(())
    }

    /// `detachLeaf`: drop one stroke id from every group listing it, dissolving
    /// any group left with fewer than two entries. The item tag is not touched.
    pub(crate) fn detach_leaf(&mut self, stroke_id: &str) {
        if stroke_id.is_empty() {
            return;
        }
        let gids: Vec<String> = self.entries.iter().map(|entry| entry.gid.clone()).collect();
        for gid in gids {
            let Some(at) = self.slot(&gid) else {
                continue;
            };
            let Some(ix) = self.entries[at]
                .order
                .iter()
                .position(|entry| entry == stroke_id)
            else {
                continue;
            };
            self.entries[at].order.remove(ix);
            if self.entries[at].order.len() < 2 {
                let _ = self.dissolve_group(&gid);
            }
        }
    }

    /// `removeMemberFromGroup`: take one item out of the group its tag names.
    pub(crate) fn remove_member(&mut self, item: usize) -> Result<bool, GroupError> {
        if item >= self.items.len() {
            return Err(GroupError::UnknownItem(item));
        }
        let Some(gid) = self.tag_of(item).map(str::to_string) else {
            return Err(GroupError::NotInGroup(item));
        };
        let Some(at) = self.slot(&gid) else {
            return Err(GroupError::NotInGroup(item));
        };
        let stroke_id = self.items[item].stroke_id.clone();
        self.entries[at].order.retain(|entry| *entry != stroke_id);
        self.items[item].group_id = None;
        let dissolve = self.entries[at].order.len() < 2;
        if dissolve {
            let _ = self.dissolve_group(&gid);
        }
        Ok(dissolve)
    }

    /// Flatten's subtree removal: every descendant entry named by an order (depth 16).
    pub(crate) fn remove_subtree(&mut self, gid: &str) {
        self.remove_below(gid, 0);
    }

    fn remove_below(&mut self, gid: &str, depth: usize) {
        if depth > MEMBER_DEPTH_CAP {
            return;
        }
        let Some(at) = self.slot(gid) else {
            return;
        };
        for member in self.entries[at].order.clone() {
            if self.slot(&member).is_some() {
                self.remove_below(&member, depth + 1);
            }
        }
        self.entries.retain(|entry| entry.gid != gid);
    }

    /// `setGroupCombineMode`; the bool says whether the stored value changed.
    pub(crate) fn set_combine_mode(
        &mut self,
        gid: &str,
        mode: CombineMode,
    ) -> Result<bool, GroupError> {
        let Some(at) = self.slot(gid) else {
            return Err(GroupError::UnknownGroup(gid.to_string()));
        };
        let changed = self.entries[at].combine_mode != Some(mode);
        self.entries[at].combine_mode = Some(mode);
        Ok(changed)
    }

    /// `renameGroup`: creates a plain (`none`) entry on first rename.
    pub(crate) fn rename_group(
        &mut self,
        gid: &str,
        name: &str,
        member_ids: &[String],
    ) -> Result<RenameOutcome, GroupError> {
        if gid.is_empty() {
            return Err(GroupError::EmptyGroupId);
        }
        let created = self.slot(gid).is_none();
        if created {
            self.entries.push(GroupEntry {
                gid: gid.to_string(),
                name: None,
                combine_mode: Some(CombineMode::None),
                parent: None,
                order: member_ids.to_vec(),
            });
        }
        let Some(at) = self.slot(gid) else {
            return Err(GroupError::UnknownGroup(gid.to_string()));
        };
        let changed = created || self.entries[at].name.as_deref() != Some(name);
        self.entries[at].name = Some(name.to_string());
        Ok(RenameOutcome { created, changed })
    }
}

#[cfg(test)]
#[path = "../tests/object_group_hierarchy_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_group_hierarchy_mutations.rs"]
mod mutations;
#[cfg(test)]
#[path = "../tests/object_group_hierarchy.rs"]
mod tests;
