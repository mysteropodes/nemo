//! Evidence defect 1 (#2108): a group with a combine mode paints its boolean RESULT, never its members (baseline
//! `renderCombinesFromChildren` + the `buildSceneJson` post-process, `src/js/engine-bridge.js` at 1065e9d8).
//!
//! The plan keeps the document untouched. After the contour sequence is joined, every combining group of a layer is folded
//! (`evaluation::object_group_combine_fold`, the same fold the tween ghost and `flatten` use), from the members' DRAWN
//! paths (element Motion, layer Motion and parent chain already applied, so an animated member combines where it is
//! seen). The members (and their linked-fill / brush-texture blocks) leave the plan; each island of the result is
//! added right after the group's front-most member, styled from the group's last visible member as the baseline does.
//! Islands are carried as [`DabPaint`]s (a path with prepared paint), so every consumer that already walks the plan's
//! entries (viewport, PNG/video export, layered content, masks) paints them without a second entry kind.
//!
//! A member the fold cannot read (an unfilled, degenerate or non-polygon shape) leaves its group un-combined, which is the
//! baseline's own catch around the fold. A layer that hosts a duplicator paints its copies and is not combined here.
use crate::evaluation::object_group_combine_fold::{
    fold_layer, has_combining_group, FoldRefused, Member,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record_posed, SceneItemStyle,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::FillResolution;
use crate::object_render_scene::object_scene_style_emit::prepare_item;
use crate::object_render_scene::paint::dab_paint::DabPaint;
use crate::object_render_scene::render_vector_path::build_path;
use crate::object_render_scene::{ObjectPaintPlanError, ObjectRenderScene, PaintIndex};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::collections::{HashMap, HashSet};
use vello::kurbo::{flatten, BezPath, PathEl};

/// Flattening tolerance of a curved member, in document units.
const TOLERANCE: f64 = 0.05;

/// The drawn outline as one polygon ring; `None` when the path is not a single closed outline.
fn ring_of(path: &BezPath) -> Option<Vec<[f64; 2]>> {
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
    (subpaths == 1).then_some(ring)
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
    let document = packet.snapshot().document();
    let mut suppressed: HashSet<usize> = HashSet::new();
    let mut groups_out: Vec<Combined> = Vec::new();
    for (layer_index, layer) in packet.source_layers().iter().enumerate() {
        if !has_combining_group(layer, packet.context_id(), packet.scope_kind())
            || scene.duplicators.is_host(layer_index)
        {
            continue;
        }
        let Some(frame) = packet.source_frame_for_layer(layer_index) else {
            continue;
        };
        let by_id: HashMap<&str, usize> = scene
            .entries()
            .enumerate()
            .filter(|(_, entry)| entry.source().source_layer_index() == layer_index)
            .map(|(index, entry)| (entry.source().record().target().stroke_id(), index))
            .collect();
        let member = |id: &str| {
            let entry = *by_id.get(id)?;
            let drawn = scene.entry(entry)?;
            Some(Member {
                entry,
                record: drawn.source().record(),
                ring: ring_of(drawn.path()),
            })
        };
        let folded = fold_layer(document, layer, packet.context_id(), frame, &member)
            .map_err(|FoldRefused| ObjectPaintPlanError::Unavailable)?;
        let Some(folded) = folded else { continue };
        suppressed.extend(folded.suppress);
        for group in folded.groups {
            let islands = islands_of(scene, layer_index, group.style_entry, &group.islands)?;
            groups_out.push(Combined {
                members: group.members,
                islands,
            });
        }
    }
    if groups_out.is_empty() && suppressed.is_empty() {
        return Ok(None);
    }
    let object_of = |entry: &usize| {
        scene
            .entry(*entry)
            .map(|e| e.source().source_object_index())
    };
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
                    out.push(PaintIndex::Dab(
                        dabs.len() + extra.len(),
                        island.layer_index,
                    ));
                    extra.push(island.clone());
                }
            }
        }
    }
    Ok(Some((out, extra)))
}

/// One paint per island of a folded group, styled from the style source entry.
fn islands_of(
    scene: &ObjectRenderScene,
    layer_index: usize,
    source: usize,
    islands: &[Vec<[f64; 6]>],
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
    for island in islands {
        let segments: Vec<CubicSegment> = island
            .iter()
            .map(|s| CubicSegment {
                point: [s[0], s[1]],
                handle_in: [s[2], s[3]],
                handle_out: [s[4], s[5]],
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
