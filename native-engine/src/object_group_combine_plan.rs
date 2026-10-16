//! Combine operand-plan kernel (G24/G19, L02-B): which shapes a combining
//! group folds, what stops painting, and how the result is styled.
//!
//! Mirrors `renderCombinesFromChildren` / `emit` of `src/js/group-bridge.js`
//! at fbe4b8a3. The walk is top-down from the root groups (one pass per
//! duplicator bucket): a combining child contributes ONE operand (its own
//! folded result, holes included), a non-combining child contributes its
//! leaves as if it were absent, and a result is either handed up to a
//! combining parent or emitted as islands, never both. The boolean math is
//! NOT here: it sits behind [`CombineFold`], whose real provider is outside
//! this crate today, so [`UnavailableFold`] reports typed unavailability.
//! The kernel owns no document, history, id source or geometry; it reads the
//! L02-A group table plus per-item facts the caller extracted.
use std::fmt;

use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupLayer,
};

#[path = "object_group_combine_flatten.rs"]
mod flatten;
#[allow(unused_imports)]
pub(crate) use flatten::{execute_flatten, plan_flatten, FlattenError, FlattenPlan, OperandRef};

/// `emit` stops below this depth (same cap as `resolveGroupMembers`).
const DEPTH_CAP: usize = 16;

/// JS truthiness of a string field: absent and `""` are both falsy.
pub(crate) fn truthy(text: &Option<String>) -> bool {
    text.as_deref().is_some_and(|value| !value.is_empty())
}

fn nonempty(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|value| !value.is_empty())
}

/// What the baseline reads off one layer child (`data.*` tags and paint).
/// Parallel to `GroupLayer::items()`; paint stays opaque text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MemberFacts {
    /// `item.visible !== false`.
    pub(crate) visible: bool,
    pub(crate) linked_fill_id: Option<String>,
    pub(crate) brush_group_id: Option<String>,
    pub(crate) is_linked_fill_companion: bool,
    pub(crate) is_vector_brush: bool,
    pub(crate) fill_color: Option<String>,
    pub(crate) stroke_color: Option<String>,
    pub(crate) stroke_width: f64,
    pub(crate) opacity: f64,
}

/// Paint of every island a group emits (taken from its style source).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResultStyle {
    pub(crate) fill: Option<String>,
    pub(crate) stroke: Option<String>,
    pub(crate) stroke_width: f64,
    pub(crate) opacity: f64,
}

/// The provider could not run (`Unavailable`: abort, typed) or this one call
/// failed (`Failed`: the baseline's catch, the group stays un-combined).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FoldError {
    Unavailable,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CombineError {
    FactsMismatch {
        items: usize,
        facts: usize,
    },
    FoldUnavailable,
    FoldFailed(String),
    /// A combining group with no visible leaf (cannot happen for a consistent table).
    NoStyleSource(String),
}

impl fmt::Display for CombineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub(crate) fn lift(error: FoldError) -> CombineError {
    match error {
        FoldError::Unavailable => CombineError::FoldUnavailable,
        FoldError::Failed(text) => CombineError::FoldFailed(text),
    }
}

/// Boolean provider contract (`foldBooleanOp` + `flattenBooleanResult`).
/// `member_shape` is the operand for the CURRENT frame: element Motion baked,
/// linked-fill companion unioned in. `fold` receives every operand in z-order
/// in ONE call and must retain all disjoint pieces and holes (boolean_op_multi
/// semantics); shapes it returned earlier come back as operands and are fill
/// geometry (never stroke-expanded). `islands` splits a fold result.
pub(crate) trait CombineFold {
    type Shape: Clone;
    fn member_shape(
        &mut self,
        item: usize,
        companion: Option<usize>,
    ) -> Result<Self::Shape, FoldError>;
    fn fold(
        &mut self,
        mode: CombineMode,
        operands: &[Self::Shape],
    ) -> Result<Self::Shape, FoldError>;
    fn islands(&mut self, shape: &Self::Shape) -> Result<Vec<Self::Shape>, FoldError>;
}

/// The honest port while no geometry provider exists (lead ruling R3).
pub(crate) struct UnavailableFold;

impl CombineFold for UnavailableFold {
    type Shape = ();
    fn member_shape(&mut self, _item: usize, _companion: Option<usize>) -> Result<(), FoldError> {
        Err(FoldError::Unavailable)
    }
    fn fold(&mut self, _mode: CombineMode, _operands: &[()]) -> Result<(), FoldError> {
        Err(FoldError::Unavailable)
    }
    fn islands(&mut self, _shape: &()) -> Result<Vec<()>, FoldError> {
        Err(FoldError::Unavailable)
    }
}

/// One layer: the L02-A group table plus one [`MemberFacts`] per item.
#[derive(Clone, Copy)]
pub(crate) struct CombineLayer<'a> {
    groups: &'a GroupLayer,
    facts: &'a [MemberFacts],
}

impl<'a> CombineLayer<'a> {
    pub(crate) fn new(
        groups: &'a GroupLayer,
        facts: &'a [MemberFacts],
    ) -> Result<Self, CombineError> {
        let items = groups.items().len();
        if items != facts.len() {
            return Err(CombineError::FactsMismatch {
                items,
                facts: facts.len(),
            });
        }
        Ok(CombineLayer { groups, facts })
    }

    pub(crate) fn groups(&self) -> &'a GroupLayer {
        self.groups
    }

    pub(crate) fn facts(&self) -> &'a [MemberFacts] {
        self.facts
    }

    /// `findByStrokeId`: first child with this id in the duplicator bucket.
    fn find_member(&self, stroke_id: &str, dup: u32) -> Option<usize> {
        self.groups
            .items()
            .iter()
            .position(|item| item.stroke_id == stroke_id && item.dup_index.unwrap_or(0) == dup)
    }

    /// `findLinkedFillCompanion`: first OTHER child tagged as the companion of
    /// this one's `linkedFillId` (the live back-reference has no native twin).
    pub(crate) fn companion_of(&self, item: usize) -> Option<usize> {
        let wanted = nonempty(&self.facts.get(item)?.linked_fill_id)?;
        (0..self.facts.len()).find(|&at| {
            at != item
                && self.facts[at].is_linked_fill_companion
                && self.facts[at].linked_fill_id.as_deref() == Some(wanted)
        })
    }

    /// `suppressBlock`: the member, then every sibling sharing its
    /// `linkedFillId` or `brushGroupId`, in layer order (all duplicator buckets).
    fn block_of(&self, item: usize) -> Vec<usize> {
        let mut block = vec![item];
        let linked = nonempty(&self.facts[item].linked_fill_id);
        let brush = nonempty(&self.facts[item].brush_group_id);
        if linked.is_none() && brush.is_none() {
            return block;
        }
        for (at, other) in self.facts.iter().enumerate() {
            if at == item {
                continue;
            }
            let same_fill = linked.is_some() && nonempty(&other.linked_fill_id) == linked;
            let same_brush = brush.is_some() && nonempty(&other.brush_group_id) == brush;
            if same_fill || same_brush {
                block.push(at);
            }
        }
        block
    }

    /// Island paint for a style source: a vector-brush ribbon's own fill is its
    /// ink (the stroke), the visible fill is its companion's; otherwise fill
    /// falls back to the stroke colour for a stroke-only member.
    pub(crate) fn result_style(&self, source: usize) -> ResultStyle {
        let member = &self.facts[source];
        let (fill, stroke) = if member.is_vector_brush {
            let backdrop = self
                .companion_of(source)
                .and_then(|at| self.facts[at].fill_color.clone());
            (backdrop, member.fill_color.clone())
        } else if truthy(&member.fill_color) {
            (member.fill_color.clone(), member.stroke_color.clone())
        } else {
            (member.stroke_color.clone(), None)
        };
        ResultStyle {
            fill,
            stroke,
            stroke_width: member.stroke_width,
            opacity: member.opacity,
        }
    }
}

/// One emitted island (`extra` entry: `{path, groupCombineOf}` plus paint).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CombinedPath<S> {
    pub(crate) group: String,
    pub(crate) style: ResultStyle,
    pub(crate) shape: S,
}

/// `{suppress, extra}`: item indices that stop painting, in first-seen order,
/// and the combined islands in emission order (inner groups first).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CombineOutcome<S> {
    pub(crate) suppress: Vec<usize>,
    pub(crate) paths: Vec<CombinedPath<S>>,
}

enum Entry<S> {
    Member {
        item: usize,
        companion: Option<usize>,
    },
    Shape(S),
}

enum Folded<S> {
    Raw(S),
    Islands(Vec<S>),
}

struct Walk<'a, F: CombineFold> {
    layer: CombineLayer<'a>,
    port: &'a mut F,
    dup: u32,
    ancestors: Vec<String>,
    suppress: Vec<usize>,
    paths: Vec<CombinedPath<F::Shape>>,
}

impl<'a, F: CombineFold> Walk<'a, F> {
    fn suppress_block(&mut self, item: usize) {
        for at in self.layer.block_of(item) {
            if !self.suppress.contains(&at) {
                self.suppress.push(at);
            }
        }
    }

    fn fold_entries(
        &mut self,
        mode: CombineMode,
        operands: &[Entry<F::Shape>],
        flatten: bool,
    ) -> Result<Folded<F::Shape>, FoldError> {
        let mut shapes = Vec::with_capacity(operands.len());
        for operand in operands {
            shapes.push(match operand {
                Entry::Member { item, companion } => self.port.member_shape(*item, *companion)?,
                Entry::Shape(shape) => shape.clone(),
            });
        }
        let raw = self.port.fold(mode, &shapes)?;
        if flatten {
            Ok(Folded::Islands(self.port.islands(&raw)?))
        } else {
            Ok(Folded::Raw(raw))
        }
    }

    fn emit(
        &mut self,
        gid: &str,
        parent_combines: bool,
        depth: usize,
    ) -> Result<Vec<Entry<F::Shape>>, CombineError> {
        let layer = self.layer;
        if depth > DEPTH_CAP || self.ancestors.iter().any(|seen| seen == gid) {
            return Ok(Vec::new());
        }
        let Some(group) = layer.groups.entry(gid) else {
            return Ok(Vec::new());
        };
        let mode = group.combine_mode.unwrap_or(CombineMode::None);
        self.ancestors.push(gid.to_string());
        let mut operands: Vec<Entry<F::Shape>> = Vec::new();
        for member in &group.order {
            if layer.groups.entry(member).is_some() {
                let up = self.emit(member, mode != CombineMode::None, depth + 1)?;
                operands.extend(up);
            } else if let Some(item) = layer.find_member(member, self.dup) {
                if layer.facts[item].visible {
                    operands.push(Entry::Member {
                        item,
                        companion: layer.companion_of(item),
                    });
                }
            }
        }
        self.ancestors.pop();
        if mode == CombineMode::None || operands.len() < 2 {
            return Ok(operands);
        }
        let leaves: Vec<usize> = layer
            .groups
            .resolve_group_members(gid, Some(self.dup))
            .into_iter()
            .filter(|&at| layer.facts[at].visible)
            .collect();
        let folded = match self.fold_entries(mode, &operands, !parent_combines) {
            Ok(folded) => folded,
            Err(FoldError::Unavailable) => return Err(CombineError::FoldUnavailable),
            // The baseline's catch: the group stays un-combined, nothing is suppressed.
            Err(FoldError::Failed(_)) => return Ok(operands),
        };
        for &leaf in &leaves {
            self.suppress_block(leaf);
        }
        let islands = match folded {
            Folded::Raw(shape) => return Ok(vec![Entry::Shape(shape)]),
            Folded::Islands(islands) => islands,
        };
        let Some(&source) = leaves.last() else {
            return Err(CombineError::NoStyleSource(gid.to_string()));
        };
        let style = layer.result_style(source);
        for shape in &islands {
            self.paths.push(CombinedPath {
                group: gid.to_string(),
                style: style.clone(),
                shape: shape.clone(),
            });
        }
        Ok(islands.into_iter().map(Entry::Shape).collect())
    }
}

/// `renderCombinesFromChildren`: roots only, one pass per duplicator bucket.
/// `Unavailable` from the provider aborts with no outcome at all.
pub(crate) fn render_combines<F: CombineFold>(
    layer: &CombineLayer<'_>,
    port: &mut F,
) -> Result<CombineOutcome<F::Shape>, CombineError> {
    let mut walk = Walk {
        layer: *layer,
        port,
        dup: 0,
        ancestors: Vec::new(),
        suppress: Vec::new(),
        paths: Vec::new(),
    };
    for entry in layer.groups.entries() {
        if layer.groups.parent_group_of(&entry.gid).is_some() {
            continue;
        }
        let strokes = layer.groups.collect_group_stroke_ids(&entry.gid);
        if strokes.is_empty() {
            continue;
        }
        let mut buckets: Vec<u32> = layer
            .groups
            .items()
            .iter()
            .filter(|item| strokes.contains(&item.stroke_id))
            .map(|item| item.dup_index.unwrap_or(0))
            .collect();
        buckets.sort_unstable();
        buckets.dedup();
        for dup in buckets {
            walk.dup = dup;
            walk.ancestors.clear();
            walk.emit(&entry.gid, false, 0)?;
        }
    }
    Ok(CombineOutcome {
        suppress: walk.suppress,
        paths: walk.paths,
    })
}

#[cfg(test)]
#[path = "../tests/object_group_combine_plan_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_group_combine_plan_ports.rs"]
mod ports;
#[cfg(test)]
#[path = "../tests/object_group_combine_plan.rs"]
mod tests;
