//! Evidence defect 1 (#2108): a group with a combine mode paints its boolean RESULT, never its members (baseline
//! `renderCombinesFromChildren` + the `buildSceneJson` post-process, `src/js/engine-bridge.js` at 1065e9d8).
//!
//! The plan keeps the document untouched. After the contour sequence is joined, every combining group of a layer is folded
//! with the same fold the tween ghost and `flatten` use (`render_combines` over `OverlayFold`), from the members' DRAWN
//! paths (element Motion, layer Motion and parent chain already applied, so an animated member combines where it is
//! seen). The members (and their linked-fill / brush-texture blocks) leave the plan; each island of the result is
//! added right after the group's front-most member, styled from the group's last visible member as the baseline does.
//! Islands are carried as [`DabPaint`]s (a path with prepared paint), so every consumer that already walks the plan's
//! entries (viewport, PNG/video export, layered content, masks) paints them without a second entry kind.
//!
//! A member the fold cannot read (an unfilled, degenerate or non-polygon shape) leaves its group un-combined, which is the
//! baseline's own catch around the fold. A layer that hosts a duplicator paints its copies and is not combined here.
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record_posed, SceneItemStyle,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::FillResolution;
use crate::object_document::object_group_kernels::object_boolean_ops::{
    dedupe_ring, flatten_polygons, fold_multi,
    stack::{hex8, stack_of, Scope},
    BoolOp, MemberSource, MultiPolygon, OverlayFold,
};
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    render_combines, CombineLayer, FoldError, MemberFacts,
};
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
use crate::object_document::{FrameScopeKind, ObjectRecord};
use crate::object_render_scene::paint::dab_paint::DabPaint;
use crate::object_render_scene::object_scene_style_emit::prepare_item;
use crate::object_render_scene::render_vector_path::build_path;
use crate::object_render_scene::{ObjectPaintPlanError, ObjectRenderScene, PaintIndex};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::collections::{HashMap, HashSet};
use vello::kurbo::{flatten, BezPath, PathEl};

/// Flattening tolerance of a curved member, in document units.
const TOLERANCE: f64 = 0.05;

/// `object_boolean_ops_stack::hex8` of the record's solid fill / stroke, the facts of one member.
fn facts_of(record: &ObjectRecord) -> MemberFacts {
    let pressure = record.vector_ext.value().and_then(|e| e.pressure.value());
    let texture = record
        .vector_ext
        .value()
        .and_then(|e| e.brush_texture.value());
    let body = record.vector();
    MemberFacts {
        visible: true,
        linked_fill_id: record
            .vector_ext
            .value()
            .and_then(|p| p.linked_fill_id.value())
            .cloned(),
        brush_group_id: texture.and_then(|t| t.brush_group_id.value()).cloned(),
        is_linked_fill_companion: record
            .vector_ext
            .value()
            .and_then(|p| p.is_linked_fill_companion.value())
            == Some(&true),
        is_vector_brush: pressure.is_some(),
        fill_color: record.solid_fill().map(hex8),
        stroke_color: body.and_then(|v| v.solid_stroke()).map(hex8),
        stroke_width: body.map_or(1., |v| v.stroke_width.as_f64().unwrap_or(1.)),
        opacity: body.map_or(1., |v| v.opacity.as_f64().unwrap_or(1.)),
    }
}

/// The fill polygon of one drawn path: the flattened outline, consecutive duplicates removed.
fn polygon_of(path: &BezPath) -> Option<MultiPolygon> {
    let mut ring: Vec<[f64; 2]> = Vec::new();
    let mut subpaths = 0;
    flatten(path.elements().iter().copied(), TOLERANCE, |el| match el {
        PathEl::MoveTo(p) => {
            subpaths += 1;
            ring.push([p.x, p.y]);
        }
        PathEl::LineTo(p) => ring.push([p.x, p.y]),
        _ => {}
    });
    if subpaths != 1 {
        return None;
    }
    let ring = dedupe_ring(&ring);
    (ring.len() >= 3 && ring.iter().flatten().all(|v| v.is_finite())).then(|| {
        vec![Polygon {
            exterior: ring,
            holes: Vec::new(),
        }]
    })
}

struct DrawnMembers<'a> {
    /// Per layer item (the stack position): the scene entry that draws it.
    entries: &'a [Option<usize>],
    scene: &'a ObjectRenderScene,
}

impl DrawnMembers<'_> {
    fn shape(&self, item: usize) -> Result<MultiPolygon, FoldError> {
        let entry = self
            .entries
            .get(item)
            .copied()
            .flatten()
            .and_then(|index| self.scene.entry(index))
            .ok_or_else(|| FoldError::Failed("member has no drawn entry".into()))?;
        // The baseline folds a FILLED shape; a stroke-only member is expanded there, which has no native twin yet.
        if entry.source().record().solid_fill().is_none() {
            return Err(FoldError::Failed("unfilled member".into()));
        }
        polygon_of(entry.path()).ok_or_else(|| FoldError::Failed("member is not a polygon".into()))
    }
}

impl MemberSource for DrawnMembers<'_> {
    fn member(&mut self, item: usize, companion: Option<usize>) -> Result<MultiPolygon, FoldError> {
        let mut shape = self.shape(item)?;
        if let Some(other) = companion {
            let fill = self.shape(other)?;
            shape = fold_multi(BoolOp::Unite, &[shape, fill])
            .map_err(|error| FoldError::Failed(format!("{error}")))?;
        }
        Ok(shape)
    }
}

/// The islands of one combined group, ready to paint, and where they go.
struct Combined {
    /// Scene entries of every member of the group (and its nested groups).
    members: HashSet<usize>,
    islands: Vec<DabPaint>,
}

/// The paint sequence with every combined group replaced by its islands, and the island paints the new
/// `PaintIndex::Dab` entries name (indexed after `dabs`, which they extend). `None` when no group combines.
pub(crate) fn expand(
    scene: &ObjectRenderScene,
    mixed: &[PaintIndex],
    dabs: &[DabPaint],
) -> Result<Option<(Vec<PaintIndex>, Vec<DabPaint>)>, ObjectPaintPlanError> {
    let packet = scene.packet();
    if packet.scope_kind() != &FrameScopeKind::Authored {
        return Ok(None);
    }
    let document = packet.snapshot().document();
    let mut suppressed: HashSet<usize> = HashSet::new();
    let mut groups_out: Vec<Combined> = Vec::new();
    for (layer_index, layer) in packet.source_layers().iter().enumerate() {
        let Some(table) = layer.groups.value() else {
            continue;
        };
        if table.context_id != packet.context_id()
            || scene.duplicators.is_host(layer_index)
            || !table
                .entries
                .iter()
                .any(|entry| entry.combine_mode.value().is_some_and(|mode| mode != "none"))
        {
            continue;
        }
        let Some(frame) = packet.source_frame_for_layer(layer_index) else {
            continue;
        };
        let scope = Scope {
            context_id: packet.context_id().to_owned(),
            layer_uid: layer.layer_uid().to_owned(),
            frame,
        };
        let ids = stack_of(document, &scope);
        let by_id: HashMap<&str, usize> = scene
            .entries()
            .enumerate()
            .filter(|(_, entry)| entry.source().source_layer_index() == layer_index)
            .map(|(index, entry)| (entry.source().record().target().stroke_id(), index))
            .collect();
        let entries: Vec<Option<usize>> = ids
            .iter()
            .map(|id| by_id.get(id.as_str()).copied())
            .collect();
        let facts: Vec<MemberFacts> = entries
            .iter()
            .map(|entry| match entry.and_then(|index| scene.entry(index)) {
                Some(entry) => facts_of(entry.source().record()),
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
        let groups = table
            .to_layer(&ids)
            .ok_or(ObjectPaintPlanError::Unavailable)?;
        let combine =
            CombineLayer::new(&groups, &facts).map_err(|_| ObjectPaintPlanError::Unavailable)?;
        let mut provider = OverlayFold::new(DrawnMembers {
            entries: &entries,
            scene,
        });
        let outcome = render_combines(&combine, &mut provider)
            .map_err(|_| ObjectPaintPlanError::Unavailable)?;
        for &item in &outcome.suppress {
            suppressed.extend(entries[item]);
        }
        let mut by_group: Vec<(String, Combined)> = Vec::new();
        for combined in outcome.paths {
            let leaves = groups.collect_group_stroke_ids(&combined.group);
            let members: HashSet<usize> = ids
                .iter()
                .enumerate()
                .filter(|(_, id)| leaves.contains(id))
                .filter_map(|(item, _)| entries[item])
                .collect();
            // The style source: the last visible member, as `renderCombinesFromChildren`.
            let source = groups
                .resolve_group_members(&combined.group, Some(0))
                .into_iter()
                .filter(|&item| facts[item].visible)
                .next_back()
                .and_then(|item| entries[item])
                .ok_or(ObjectPaintPlanError::Unavailable)?;
            let islands = islands_of(scene, layer_index, source, &combined.shape)?;
            match by_group.iter_mut().find(|(gid, _)| *gid == combined.group) {
                Some((_, existing)) => existing.islands.extend(islands),
                None => by_group.push((combined.group, Combined { members, islands })),
            }
        }
        groups_out.extend(by_group.into_iter().map(|(_, combined)| combined));
    }
    if groups_out.is_empty() && suppressed.is_empty() {
        return Ok(None);
    }
    let object_of = |entry: &usize| scene.entry(*entry).map(|e| e.source().source_object_index());
    let hidden_objects: HashSet<usize> = suppressed.iter().filter_map(object_of).collect();
    let contour = |index: &PaintIndex| match index {
        PaintIndex::Contour(entry) => Some(*entry),
        _ => None,
    };
    // Where each group's islands go: right after its front-most member in paint order.
    let anchors: Vec<Option<usize>> = groups_out
        .iter()
        .map(|combined| {
            mixed
                .iter()
                .rposition(|index| contour(index).is_some_and(|e| combined.members.contains(&e)))
        })
        .collect();
    let mut extra: Vec<DabPaint> = Vec::new();
    let mut out = Vec::with_capacity(mixed.len());
    for (at, index) in mixed.iter().enumerate() {
        let hidden = match index {
            PaintIndex::Contour(entry) => suppressed.contains(entry),
            // The dabs of a suppressed anchor go with it.
            PaintIndex::Dab(dab, _) => hidden_objects.contains(&dabs[*dab].source_index),
            _ => false,
        };
        if !hidden {
            out.push(*index);
        }
        for (combined, anchor) in groups_out.iter().zip(&anchors) {
            if *anchor == Some(at) {
                for island in &combined.islands {
                    out.push(PaintIndex::Dab(dabs.len() + extra.len(), island.layer_index));
                    extra.push(island.clone());
                }
            }
        }
    }
    Ok(Some((out, extra)))
}

/// One paint per island of a folded shape, styled from the style source entry.
fn islands_of(
    scene: &ObjectRenderScene,
    layer_index: usize,
    source: usize,
    shape: &MultiPolygon,
) -> Result<Vec<DabPaint>, ObjectPaintPlanError> {
    let entry = scene
        .entry(source)
        .ok_or(ObjectPaintPlanError::InvalidOrder)?;
    let unavailable = |_| ObjectPaintPlanError::Unavailable;
    let mut style = style_of_record_posed(
        entry.source().record(),
        entry.layer_opacity(),
        scene.packet(),
        entry.paint_pose(),
    )
    .map_err(unavailable)?;
    // A stroke-only source: the folded geometry is the member's expanded ribbon, so the result is FILLED with the stroke
    // colour and has no stroke of its own (`isl_fill = fillColor || strokeColor`).
    if matches!(style.fill, FillResolution::Empty) {
        if let Some(stroke) = style.stroke.take() {
            style = SceneItemStyle {
                fill: FillResolution::Solid(stroke.paint),
                stroke: None,
                order: style.order,
            };
        }
    }
    let opacity = entry
        .source()
        .record()
        .vector()
        .map_or(1., |body| body.opacity.as_f64().unwrap_or(1.));
    let svg_rgba = match (&style.fill, &style.stroke) {
        (FillResolution::Solid(solid), None) => {
            let [r, g, b, a] = solid.channels();
            Some([r.as_f64(), g.as_f64(), b.as_f64(), a.as_f64()])
                .and_then(|c| Some([c[0]?, c[1]?, c[2]?, c[3]?]))
        }
        // A stroke or a gradient has no fill-only vector representation: the vector exporters refuse, never drop it.
        _ => None,
    };
    let prepared = prepare_item(&style).map_err(|_| ObjectPaintPlanError::Unavailable)?;
    let mut out = Vec::new();
    for island in flatten_polygons(shape, true).map_err(|_| ObjectPaintPlanError::Unavailable)? {
        let segments: Vec<CubicSegment> = island
            .segments
            .iter()
            .map(|s| CubicSegment {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
            })
            .collect();
        out.push(DabPaint {
            layer_index,
            source_index: entry.source().source_object_index(),
            path: build_path(&segments, true),
            prepared: prepared.clone(),
            svg_contour: svg_rgba.and_then(|_| ClosedCubicPath::new(segments).ok()),
            svg_rgba,
            opacity,
            layer_opacity: entry.layer_opacity(),
            closed: true,
        });
    }
    Ok(out)
}
