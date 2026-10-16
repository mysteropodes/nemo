//! Selection-driven group transitions (`groupSelection`, `ungroupSelection`,
//! `combineSelection` of group-bridge.js at fbe4b8a3) over the hierarchy table.
//!
//! A selection is a list of item indices into the layer (selection order, which
//! is NOT z-order). Every refusal is decided before the first write.
use super::{CombineMode, GroupEntry, GroupError, GroupLayer, ROOT_DEPTH_CAP};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupUnit {
    Group(String),
    /// Layer item index of a loose leaf.
    Leaf(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupOutcome {
    pub(crate) gid: String,
    pub(crate) units: Vec<GroupUnit>,
    pub(crate) common_owner: Option<String>,
    /// The common owner fell below two entries and was dissolved afterwards.
    pub(crate) owner_dissolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UngroupOutcome {
    pub(crate) dissolved: Vec<String>,
    /// Items whose tag was cleared because no selected tag named a live group.
    pub(crate) untagged: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CombineOutcome {
    Created { gid: String },
    Updated { gid: String, changed: bool },
}

impl GroupLayer {
    fn check_selection(&self, selection: &[usize]) -> Result<(), GroupError> {
        for (n, &item) in selection.iter().enumerate() {
            if item >= self.items.len() {
                return Err(GroupError::UnknownItem(item));
            }
            if selection[..n].contains(&item) {
                return Err(GroupError::DuplicateSelection(item));
            }
        }
        Ok(())
    }

    fn check_new_gid(&self, gid: &str) -> Result<(), GroupError> {
        if gid.is_empty() {
            return Err(GroupError::EmptyGroupId);
        }
        if self.slot(gid).is_some() {
            return Err(GroupError::DuplicateGroupId(gid.to_string()));
        }
        Ok(())
    }

    /// The LARGEST ancestor of the item's group whose every leaf is selected.
    fn largest_fully_selected(&self, item: usize, selected: &[usize]) -> Option<String> {
        let first = self.tag_of(item)?;
        self.entry(first)?;
        let mut best = None;
        let mut cur = first.to_string();
        for _ in 0..ROOT_DEPTH_CAP {
            if cur.is_empty() || self.entry(&cur).is_none() {
                break;
            }
            let leaves = self.resolve_group_members(&cur, None);
            if leaves.is_empty() || !leaves.iter().all(|leaf| selected.contains(leaf)) {
                break;
            }
            best = Some(cur.clone());
            match self.parent_group_of(&cur) {
                Some(up) => cur = up,
                None => break,
            }
        }
        best
    }

    /// The last table entry listing `key` (JS `ownerOf` keeps overwriting).
    fn owner_of(&self, key: &str) -> Option<String> {
        let mut found = None;
        for entry in &self.entries {
            if entry.order.iter().any(|member| member == key) {
                found = Some(entry.gid.clone());
            }
        }
        found
    }

    /// `groupSelection`: units are the largest fully selected ancestor group,
    /// else the loose leaf. The new group is inserted at the smallest order
    /// index inside the units' common owner, else at top level.
    pub(crate) fn group_selection(
        &mut self,
        selection: &[usize],
        new_gid: &str,
        name: &str,
    ) -> Result<GroupOutcome, GroupError> {
        self.check_selection(selection)?;
        if selection.len() < 2 {
            return Err(GroupError::FewerThanTwoUnits(selection.len()));
        }
        let mut ordered = selection.to_vec();
        ordered.sort_unstable();
        let mut units: Vec<GroupUnit> = Vec::new();
        for &item in &ordered {
            match self.largest_fully_selected(item, &ordered) {
                Some(gid) => {
                    if !units.contains(&GroupUnit::Group(gid.clone())) {
                        units.push(GroupUnit::Group(gid));
                    }
                }
                None => units.push(GroupUnit::Leaf(item)),
            }
        }
        if units.len() < 2 {
            return Err(GroupError::FewerThanTwoUnits(units.len()));
        }
        self.check_new_gid(new_gid)?;
        let keys: Vec<String> = units
            .iter()
            .map(|unit| match unit {
                GroupUnit::Group(gid) => gid.clone(),
                GroupUnit::Leaf(item) => self.items[*item].stroke_id.clone(),
            })
            .collect();
        let owners: Vec<Option<String>> = keys.iter().map(|key| self.owner_of(key)).collect();
        let common_owner = if owners.iter().all(|owner| *owner == owners[0]) {
            owners[0].clone()
        } else {
            None
        };
        let insert_at = common_owner.as_deref().and_then(|owner| {
            let order = &self.entry(owner)?.order;
            keys.iter()
                .filter_map(|key| order.iter().position(|m| m == key))
                .min()
        });
        // Detach every unit WITHOUT the fewer-than-two dissolve firing mid-way.
        for key in &keys {
            for entry in self.entries.iter_mut() {
                if let Some(ix) = entry.order.iter().position(|member| member == key) {
                    entry.order.remove(ix);
                }
            }
        }
        for unit in &units {
            match unit {
                GroupUnit::Group(gid) => {
                    if let Some(at) = self.slot(gid) {
                        self.entries[at].parent = Some(new_gid.to_string());
                    }
                }
                GroupUnit::Leaf(item) => self.items[*item].group_id = Some(new_gid.to_string()),
            }
        }
        self.entries.push(GroupEntry {
            gid: new_gid.to_string(),
            name: Some(name.to_string()),
            combine_mode: Some(CombineMode::None),
            parent: common_owner.clone(),
            order: keys,
        });
        let mut owner_dissolved = false;
        if let Some(owner) = common_owner.as_deref() {
            if let Some(at) = self.slot(owner) {
                let list = &mut self.entries[at].order;
                let ix = insert_at.unwrap_or(list.len()).min(list.len());
                list.insert(ix, new_gid.to_string());
                if list.len() < 2 {
                    owner_dissolved = self.dissolve_group(owner).is_ok();
                }
            }
        }
        Ok(GroupOutcome {
            gid: new_gid.to_string(),
            units,
            common_owner,
            owner_dissolved,
        })
    }

    /// `ungroupSelection`: dissolves exactly ONE level, the outermost group of
    /// each selected shape; with no live group the selected tags are cleared.
    pub(crate) fn ungroup_selection(
        &mut self,
        selection: &[usize],
    ) -> Result<UngroupOutcome, GroupError> {
        self.check_selection(selection)?;
        if selection.is_empty() {
            return Err(GroupError::EmptySelection);
        }
        if !selection.iter().any(|&item| self.tag_of(item).is_some()) {
            return Err(GroupError::NoGroupInSelection);
        }
        let mut roots: Vec<String> = Vec::new();
        for &item in selection {
            if let Some(root) = self.root_group_of_item(item) {
                if self.entry(&root).is_some() && !roots.contains(&root) {
                    roots.push(root);
                }
            }
        }
        if roots.is_empty() {
            for &item in selection {
                self.items[item].group_id = None;
            }
            return Ok(UngroupOutcome {
                dissolved: roots,
                untagged: selection.to_vec(),
            });
        }
        for root in &roots {
            // An earlier dissolve may already have removed a later root.
            let _ = self.dissolve_group(root);
        }
        Ok(UngroupOutcome {
            dissolved: roots,
            untagged: Vec::new(),
        })
    }

    /// `combineSelection`: when the selection is exactly one existing group's
    /// membership only that group's mode changes; otherwise a flat group is
    /// created (its members are NOT detached from earlier groups, as in JS).
    pub(crate) fn combine_selection(
        &mut self,
        selection: &[usize],
        mode: CombineMode,
        new_gid: &str,
    ) -> Result<CombineOutcome, GroupError> {
        self.check_selection(selection)?;
        if selection.len() < 2 {
            return Err(GroupError::FewerThanTwoUnits(selection.len()));
        }
        let existing = self
            .tag_of(selection[0])
            .filter(|first| self.entry(first).is_some())
            .map(str::to_string)
            .filter(|first| {
                let members = self.resolve_group_members(first, None);
                members.len() == selection.len() && members.iter().all(|m| selection.contains(m))
            });
        if let Some(gid) = existing {
            let changed = self.set_combine_mode(&gid, mode)?;
            return Ok(CombineOutcome::Updated { gid, changed });
        }
        self.check_new_gid(new_gid)?;
        let mut ordered = selection.to_vec();
        ordered.sort_unstable();
        let mut order = Vec::new();
        for item in ordered {
            self.items[item].group_id = Some(new_gid.to_string());
            order.push(self.items[item].stroke_id.clone());
        }
        self.entries.push(GroupEntry {
            gid: new_gid.to_string(),
            name: None,
            combine_mode: Some(mode),
            parent: None,
            order,
        });
        Ok(CombineOutcome::Created {
            gid: new_gid.to_string(),
        })
    }
}
