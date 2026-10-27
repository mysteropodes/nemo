//! G16 option C (#2361): the dabs a seeded brush-texture anchor paints, joined into the paint plan right after the
//! anchor's own (camouflaged) contour, in the baseline order. They are stamped over the stored stroke and posed with
//! the path the compositor draws for the anchor (`ObjectRenderEntry::path`, so motion and poses are the evaluated ones),
//! and carry prepared paint like duplicator copies do. An anchor native cannot stamp yet (a custom preset, a pose no
//! affine carries) makes the plan unavailable rather than silently losing its texture.
use crate::evaluation::brush_dab::paint::{anchor_dab_items, is_seeded_anchor, texture_group};
use std::collections::HashSet;
use crate::object_render_scene::object_scene_style_emit::{prepare_item, PreparedItem};
use crate::object_render_scene::render_vector_path::build_path;
use crate::object_render_scene::{ObjectPaintPlanError, ObjectRenderScene, PaintIndex};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use vello::kurbo::{BezPath, PathEl, Point};

/// One produced dab: its source anchor, drawn path, prepared paint and the exact closed contour and straight RGBA
/// a vector exporter writes.
#[derive(Debug, Clone)]
pub(crate) struct DabPaint {
    pub(crate) layer_index: usize,
    pub(crate) source_index: usize,
    pub(crate) path: BezPath,
    pub(crate) prepared: PreparedItem,
    pub(crate) svg_contour: Option<ClosedCubicPath>,
    pub(crate) svg_rgba: Option<[f64; 4]>,
    /// The dab's own opacity and its layer's opacity factor, as a writer of stored dabs reads them apart.
    pub(crate) opacity: f64,
    pub(crate) layer_opacity: f64,
    pub(crate) closed: bool,
}

/// The segments `build_path` drew (`[x, y, inX, inY, outX, outY]`); a closed path's closing curve ends on its first
/// anchor. An open path's first handle-in and last handle-out never reach the drawn path and read as zero.
fn segments_of(path: &BezPath) -> Option<(Vec<[f64; 6]>, bool)> {
    let (mut out, mut closed): (Vec<[f64; 6]>, bool) = (Vec::new(), false);
    let rel = |c: Point, p: Point| [c.x - p.x, c.y - p.y];
    for element in path.elements() {
        if closed {
            return None;
        }
        match *element {
            PathEl::MoveTo(p) if out.is_empty() => out.push([p.x, p.y, 0.0, 0.0, 0.0, 0.0]),
            PathEl::LineTo(p) if !out.is_empty() => out.push([p.x, p.y, 0.0, 0.0, 0.0, 0.0]),
            PathEl::CurveTo(c1, c2, p) if !out.is_empty() => {
                let last = out.len() - 1;
                let o = rel(c1, Point::new(out[last][0], out[last][1]));
                out[last][4] = o[0];
                out[last][5] = o[1];
                let i = rel(c2, p);
                out.push([p.x, p.y, i[0], i[1], 0.0, 0.0]);
            }
            PathEl::ClosePath if out.len() > 1 => {
                let last = out.len() - 1;
                if out[last][0] == out[0][0] && out[last][1] == out[0][1] {
                    let end = out.pop()?;
                    out[0][2] = end[2];
                    out[0][3] = end[3];
                }
                closed = true;
            }
            _ => return None,
        }
    }
    (out.len() >= 2).then_some((out, closed))
}

/// `mixed` with every seeded anchor followed by its dabs, and the dab paints those indices name.
pub(crate) fn expand(
    scene: &ObjectRenderScene,
    mixed: Vec<PaintIndex>,
) -> Result<(Vec<PaintIndex>, Vec<DabPaint>), ObjectPaintPlanError> {
    let (mut out, mut dabs) = (Vec::with_capacity(mixed.len()), Vec::new());
    // Groups whose dabs are stored records in this frame (imported, or a tween re-stamp): never stamped twice.
    let stored: HashSet<(usize, &str)> = scene
        .entries()
        .filter_map(|entry| {
            let (group, copy) = texture_group(entry.source().record())?;
            copy.then_some((entry.source().source_layer_index(), group))
        })
        .collect();
    for index in mixed {
        out.push(index);
        let PaintIndex::Contour(i) = index else {
            continue;
        };
        let entry = scene.entry(i).ok_or(ObjectPaintPlanError::InvalidOrder)?;
        let source = entry.source();
        if !is_seeded_anchor(source.record())
            || texture_group(source.record()).is_some_and(|(group, _)| stored.contains(&(source.source_layer_index(), group)))
        {
            continue;
        }
        let unavailable = |_| ObjectPaintPlanError::Unavailable;
        let (drawn, closed) = segments_of(entry.path()).ok_or(ObjectPaintPlanError::Unavailable)?;
        let items = anchor_dab_items(source.record(), &drawn, closed, entry.layer_opacity())
            .map_err(unavailable)?;
        for item in items {
            let segments: Vec<CubicSegment> = item
                .path
                .segments
                .iter()
                .map(|s| CubicSegment {
                    point: s.p,
                    handle_in: s.h_in,
                    handle_out: s.h_out,
                })
                .collect();
            let prepared =
                prepare_item(&item.style).map_err(|_| ObjectPaintPlanError::Unavailable)?;
            let svg_rgba = item.style.fill.solid().and_then(|solid| {
                let [r, g, b, a] = solid.channels();
                Some([r.as_f64()?, g.as_f64()?, b.as_f64()?, a.as_f64()?])
            });
            out.push(PaintIndex::Dab(dabs.len(), source.source_layer_index()));
            dabs.push(DabPaint {
                layer_index: source.source_layer_index(),
                source_index: source.source_object_index(),
                path: build_path(&segments, item.path.closed),
                prepared,
                svg_contour: ClosedCubicPath::new(segments).ok(),
                svg_rgba,
                opacity: item.opacity,
                layer_opacity: entry.layer_opacity(),
                closed: item.path.closed,
            });
        }
    }
    Ok((out, dabs))
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_brush_dab_paint.rs"]
mod tests;
