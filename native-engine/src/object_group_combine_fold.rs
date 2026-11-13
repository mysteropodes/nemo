//! Evidence defect 1 (#2108): the fold of one layer's combining groups, over the shapes the scene DRAWS.
//!
//! Evaluation-layer half of a combined group's paint (the paint-plan half is `object_group_combine_paint.rs`): the same
//! `render_combines` walk over `OverlayFold` that the tween ghost and `flatten` use, fed from the members' drawn outlines
//! (element Motion, layer Motion and parent chain already applied) instead of the stored geometry. It answers with plain
//! numbers only: which scene entries stop painting, and per combined group its member entries, its style source entry and
//! the cubic segments of each island.
//!
//! A member the fold cannot read (unfilled, a degenerate ring, several sub-paths) leaves its group un-combined, which is the
//! baseline's own catch around the fold. A table the document refuses to turn into a kernel layer is a typed refusal.
use crate::object_document::object_group_kernels::object_boolean_ops::{
    dedupe_ring, flatten_polygons, fold_multi,
    stack::{hex8, stack_of, Scope},
    BoolOp, MemberSource, MultiPolygon, OverlayFold,
};
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    render_combines, CombineLayer, FoldError, MemberFacts,
};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord};
use std::collections::HashSet;

/// One member of a combining layer: the scene entry that draws it, its record and its drawn outline.
pub(crate) struct Member<'a> {
    pub(crate) entry: usize,
    pub(crate) record: &'a ObjectRecord,
    /// The drawn outline flattened to a polygon ring; `None` when the path is not one closed polygon.
    pub(crate) ring: Option<Vec<[f64; 2]>>,
}

/// Why a layer cannot be combined at all (never a per-member problem: those leave the group un-combined).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FoldRefused;

/// One combined group: the scene entries of all its members, the entry its style comes from and its islands as
/// `[x, y, inX, inY, outX, outY]` anchors (handles relative to their anchor).
pub(crate) struct GroupFold {
    pub(crate) members: HashSet<usize>,
    pub(crate) style_entry: usize,
    pub(crate) islands: Vec<Vec<[f64; 6]>>,
}

/// The scene entries that stop painting and the combined groups, in emission order.
pub(crate) struct LayerFold {
    pub(crate) suppress: HashSet<usize>,
    pub(crate) groups: Vec<GroupFold>,
}

/// Whether the layer has any group that combines (cheap gate before any shape is read). Group tables belong to authored
/// frames of their own context: a reference or exposure scope never combines.
pub(crate) fn has_combining_group(
    layer: &ObjectLayer,
    context_id: &str,
    scope_kind: &FrameScopeKind,
) -> bool {
    *scope_kind == FrameScopeKind::Authored
        && layer.groups.value().is_some_and(|table| {
            table.context_id == context_id
                && table.entries.iter().any(|entry| {
                    entry
                        .combine_mode
                        .value()
                        .is_some_and(|mode| mode != "none")
                })
        })
}

fn facts_of(record: &ObjectRecord) -> MemberFacts {
    let ext = record.vector_ext.value();
    let body = record.vector();
    MemberFacts {
        visible: true,
        linked_fill_id: ext.and_then(|p| p.linked_fill_id.value()).cloned(),
        brush_group_id: ext
            .and_then(|p| p.brush_texture.value())
            .and_then(|t| t.brush_group_id.value())
            .cloned(),
        is_linked_fill_companion: ext.and_then(|p| p.is_linked_fill_companion.value())
            == Some(&true),
        is_vector_brush: ext.and_then(|p| p.pressure.value()).is_some(),
        fill_color: record.solid_fill().map(hex8),
        stroke_color: body.and_then(|v| v.solid_stroke()).map(hex8),
        stroke_width: body.map_or(1., |v| v.stroke_width.as_f64().unwrap_or(1.)),
        opacity: body.map_or(1., |v| v.opacity.as_f64().unwrap_or(1.)),
    }
}

/// The fill polygon of a drawn ring: consecutive duplicates removed, at least three finite points.
fn polygon_of(ring: &[[f64; 2]]) -> Option<MultiPolygon> {
    let ring = dedupe_ring(ring);
    (ring.len() >= 3 && ring.iter().flatten().all(|v| v.is_finite())).then(|| {
        vec![Polygon {
            exterior: ring,
            holes: Vec::new(),
        }]
    })
}

struct Drawn {
    /// Per layer item (stack position): its polygon, when it is a readable filled shape.
    shapes: Vec<Option<MultiPolygon>>,
}

impl MemberSource for Drawn {
    fn member(&mut self, item: usize, companion: Option<usize>) -> Result<MultiPolygon, FoldError> {
        let read = |at: usize| {
            self.shapes
                .get(at)
                .cloned()
                .flatten()
                .ok_or_else(|| FoldError::Failed("member shape unreadable".into()))
        };
        let shape = read(item)?;
        match companion {
            None => Ok(shape),
            Some(other) => fold_multi(BoolOp::Unite, &[shape, read(other)?])
                .map_err(|error| FoldError::Failed(format!("{error}"))),
        }
    }
}

/// Folds every combining group of `layer` at `frame`. `member` resolves a stroke id to its drawn member (a record the scene
/// does not draw is invisible to the fold). `Ok(None)` when no group ended up combining.
pub(crate) fn fold_layer<'a>(
    document: &ObjectDocument,
    layer: &ObjectLayer,
    context_id: &str,
    frame: u32,
    member: &dyn Fn(&str) -> Option<Member<'a>>,
) -> Result<Option<LayerFold>, FoldRefused> {
    let Some(table) = layer.groups.value() else {
        return Ok(None);
    };
    let scope = Scope {
        context_id: context_id.to_owned(),
        layer_uid: layer.layer_uid().to_owned(),
        frame,
    };
    let ids = stack_of(document, &scope);
    let members: Vec<Option<Member<'a>>> = ids.iter().map(|id| member(id)).collect();
    let facts: Vec<MemberFacts> = members
        .iter()
        .map(|m| match m {
            Some(m) => facts_of(m.record),
            None => MemberFacts {
                visible: false,
                linked_fill_id: None,
                brush_group_id: None,
                is_linked_fill_companion: false,
                is_vector_brush: false,
                fill_color: None,
                stroke_color: None,
                stroke_width: 1.,
                opacity: 1.,
            },
        })
        .collect();
    let groups = table.to_layer(&ids).ok_or(FoldRefused)?;
    let combine = CombineLayer::new(&groups, &facts).map_err(|_| FoldRefused)?;
    // The baseline folds a FILLED shape; a stroke-only member is expanded there, which has no native twin yet.
    let shapes = members
        .iter()
        .map(|m| {
            m.as_ref()
                .filter(|m| m.record.solid_fill().is_some())
                .and_then(|m| m.ring.as_deref())
                .and_then(polygon_of)
        })
        .collect();
    let outcome = render_combines(&combine, &mut OverlayFold::new(Drawn { shapes }))
        .map_err(|_| FoldRefused)?;
    let entry_of = |item: usize| members[item].as_ref().map(|m| m.entry);
    let suppress: HashSet<usize> = outcome
        .suppress
        .iter()
        .filter_map(|&i| entry_of(i))
        .collect();
    let mut out: Vec<(String, GroupFold)> = Vec::new();
    for combined in outcome.paths {
        let leaves = groups.collect_group_stroke_ids(&combined.group);
        let member_entries: HashSet<usize> = ids
            .iter()
            .enumerate()
            .filter(|(_, id)| leaves.contains(id))
            .filter_map(|(item, _)| entry_of(item))
            .collect();
        // The style source: the last visible member, as `renderCombinesFromChildren`.
        let style_entry = groups
            .resolve_group_members(&combined.group, Some(0))
            .into_iter()
            .filter(|&item| facts[item].visible)
            .next_back()
            .and_then(entry_of)
            .ok_or(FoldRefused)?;
        let islands: Vec<Vec<[f64; 6]>> = flatten_polygons(&combined.shape, true)
            .map_err(|_| FoldRefused)?
            .into_iter()
            .map(|island| {
                island
                    .segments
                    .iter()
                    .map(|s| {
                        [
                            s.point[0],
                            s.point[1],
                            s.handle_in[0],
                            s.handle_in[1],
                            s.handle_out[0],
                            s.handle_out[1],
                        ]
                    })
                    .collect()
            })
            .collect();
        match out.iter_mut().find(|(gid, _)| *gid == combined.group) {
            Some((_, existing)) => existing.islands.extend(islands),
            None => out.push((
                combined.group,
                GroupFold {
                    members: member_entries,
                    style_entry,
                    islands,
                },
            )),
        }
    }
    let groups: Vec<GroupFold> = out.into_iter().map(|(_, group)| group).collect();
    Ok((!groups.is_empty() || !suppress.is_empty()).then_some(LayerFold { suppress, groups }))
}
