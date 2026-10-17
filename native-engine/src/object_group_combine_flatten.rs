//! Flatten plan (G19, L02-B): the one explicit, destructive "Aplatir" action
//! of `flattenGroup` (`src/js/group-bridge.js` at fbe4b8a3) as a pure plan.
//!
//! Every leaf under the group (hidden ones included, nested combined children
//! NOT collapsed) is folded in one call with the group's mode (`none` becomes
//! `unite`); the originals plus each operand's linked-fill companion are all
//! removed in the SAME plan so one Undo restores them; the result is inserted
//! at the layer index of the group's FIRST member and styled from its LAST
//! member (companion preferred); descendant group entries go with it. The
//! caller's [`CombineFold::member_shape`] supplies each operand with the
//! CURRENT frame's element Motion baked in.
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    lift, truthy, CombineError, CombineFold, CombineLayer,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::CombineMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperandRef {
    pub(crate) item: usize,
    pub(crate) companion: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlattenError {
    UnknownGroup(String),
    /// Fewer than two members (the count is carried).
    FewerThanTwoMembers(usize),
    /// A legacy entry without `combineMode`: the baseline throws inside the fold.
    MissingCombineMode(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlattenPlan {
    pub(crate) group: String,
    /// `none` already mapped to `unite`.
    pub(crate) mode: CombineMode,
    /// Members in resolve order (depth first through nested groups).
    pub(crate) operands: Vec<OperandRef>,
    /// Last member: stroke width/opacity source.
    pub(crate) style_item: usize,
    /// Its companion when it has one, else the style item: carries the data tags.
    pub(crate) data_source: usize,
    /// `fillSource.fillColor || style.strokeColor`.
    pub(crate) fill: Option<String>,
    pub(crate) opacity: f64,
    /// Layer index of the first member, before any removal.
    pub(crate) insert_at: usize,
    /// Members plus companions, ascending and unique (original indices).
    pub(crate) remove: Vec<usize>,
    /// Index of the first island once `remove` has been applied.
    pub(crate) final_index: usize,
    /// Group entries that leave the table (the group and its descendants).
    pub(crate) removed_groups: Vec<String>,
}

pub(crate) fn plan_flatten(
    layer: &CombineLayer<'_>,
    gid: &str,
) -> Result<FlattenPlan, FlattenError> {
    let groups = layer.groups();
    let facts = layer.facts();
    let Some(group) = groups.entry(gid) else {
        return Err(FlattenError::UnknownGroup(gid.to_string()));
    };
    let members = groups.resolve_group_members(gid, None);
    if members.len() < 2 {
        return Err(FlattenError::FewerThanTwoMembers(members.len()));
    }
    let mode = match group.combine_mode {
        Some(CombineMode::None) => CombineMode::Unite,
        Some(mode) => mode,
        None => return Err(FlattenError::MissingCombineMode(gid.to_string())),
    };
    let operands: Vec<OperandRef> = members
        .iter()
        .map(|&item| OperandRef {
            item,
            companion: layer.companion_of(item),
        })
        .collect();
    let style_item = members[members.len() - 1];
    let data_source = layer.companion_of(style_item).unwrap_or(style_item);
    let fill = if truthy(&facts[data_source].fill_color) {
        facts[data_source].fill_color.clone()
    } else {
        facts[style_item].stroke_color.clone()
    };
    let insert_at = members[0];
    let mut remove: Vec<usize> = operands
        .iter()
        .flat_map(|operand| std::iter::once(operand.item).chain(operand.companion))
        .collect();
    remove.sort_unstable();
    remove.dedup();
    let final_index = insert_at - remove.iter().filter(|&&at| at < insert_at).count();
    let mut after = groups.clone();
    after.remove_subtree(gid);
    let removed_groups = groups
        .entries()
        .iter()
        .filter(|entry| after.entry(&entry.gid).is_none())
        .map(|entry| entry.gid.clone())
        .collect();
    Ok(FlattenPlan {
        group: gid.to_string(),
        mode,
        operands,
        style_item,
        data_source,
        fill,
        opacity: facts[style_item].opacity,
        insert_at,
        remove,
        final_index,
        removed_groups,
    })
}

/// Folds the plan's operands and returns the resulting islands, in order: they
/// become the new selection. Any provider failure refuses the whole flatten
/// (the baseline has no catch here), so nothing is half applied.
pub(crate) fn execute_flatten<F: CombineFold>(
    plan: &FlattenPlan,
    port: &mut F,
) -> Result<Vec<F::Shape>, CombineError> {
    let mut shapes = Vec::with_capacity(plan.operands.len());
    for operand in &plan.operands {
        shapes.push(
            port.member_shape(operand.item, operand.companion)
                .map_err(lift)?,
        );
    }
    let raw = port.fold(plan.mode, &shapes).map_err(lift)?;
    port.islands(&raw).map_err(lift)
}

#[cfg(test)]
#[path = "../tests/object_group_combine_flatten_controls.rs"]
mod controls;
