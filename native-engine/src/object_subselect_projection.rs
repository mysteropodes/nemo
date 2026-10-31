//! Selection identities and affine maps borrowed from the actual retained native paint plan.
use crate::object_frame_packet::subselect_record as record;
use crate::object_render_scene::{
    object_scene_producer_contour::batch::subselect, ordered_entries, ObjectCompositionWitness,
    PaintEntry,
};
use crate::object_snapshot::{SubselectReadItem, SubselectReadProjection};
impl ObjectCompositionWitness {
    pub fn subselect_read_projection(
        &self,
        layer_uid: &str,
        preview: Option<&serde_json::Value>,
        hit_test: bool,
    ) -> Result<SubselectReadProjection, &'static str> {
        if !self.is_viewport() {
            return Err("Subselect requires a native viewport presentation");
        }
        let packet = self.plan().packet();
        let index = packet
            .source_layers()
            .iter()
            .position(|l| l.layer_uid() == layer_uid)
            .ok_or("Subselect layer is absent")?;
        let preview: Option<record::Preview> = preview
            .map(|v| serde_json::from_value(v.clone()).map_err(|_| "Invalid subselect preview"))
            .transpose()?;
        let edited = preview
            .as_ref()
            .map(|p| {
                let r = packet
                    .records()
                    .find(|s| {
                        s.record().target == p.stable_target && s.source_layer_index() == index
                    })
                    .ok_or("Subselect preview target is absent")?;
                record::plan(r.record(), &p.intent)
                    .map_err(|_| "Subselect preview intent is unavailable")
            })
            .transpose()?;
        // A hit-test listing of a layer that also paints images or text poses its vectors with the union chain they share.
        let shared = hit_test
            && (packet.raster_records().any(|r| r.layer_index() == index)
                || packet.text_records().any(|r| r.layer_index() == index));
        let matrices = if shared {
            subselect::hit_matrices(packet, index, &self.plan().native_parent_chains())?
        } else {
            subselect::matrices_with_preview(packet, index, edited.as_ref())?
        };
        let mut items = Vec::new();
        // Preserve the native paint order so hit testing can choose the last hit, just like the visible stack.
        // The editable listing is contour-only; the hit-test listing is the whole mixed order (#2372), so a raster image
        // and a text block take their place in the stack.
        let plan = self.plan();
        let ordered: Vec<PaintEntry<'_>> = if hit_test {
            ordered_entries(plan).collect()
        } else {
            plan.commands().map(PaintEntry::Contour).collect()
        };
        // #2372: a texture brush is listed as its anchor alone, the bounds covering every stamped copy (a copy resolves to its
        // anchor anyway), so a layer of many textured strokes stays inside the listing bound. A copy whose anchor is absent
        // from the layer is listed as itself.
        let dabs = if hit_test { dab_bounds(self, index, &matrices) } else { DabBounds::default() };
        for entry in ordered {
            let command = match entry {
                PaintEntry::Contour(command) if command.source().source_layer_index() == index => command,
                PaintEntry::Raster(at) => {
                    items.extend(self.raster_hit_item(index, at));
                    continue;
                }
                PaintEntry::Text(source) => {
                    let style = self.plan().packet().text_records().find(|r| r.source_index() == source.source_index).and_then(|r| record::style::text(r.entry().block.color()));
                    items.extend(self.text_hit_item(index, source).map(|item| SubselectReadItem { style, ..item }));
                    continue;
                }
                _ => continue,
            };
            let source = command.source();
            let r = edited
                .as_ref()
                .filter(|r| r.target == source.record().target)
                .unwrap_or_else(|| source.record());
            // #2362 hit-test listing: every painted record, counted on its painted geometry, never refused for
            // being outside the editable scope (Select and the Labs knife only read it).
            let count = if hit_test {
                r.geometry.segments.len()
            } else {
                record::anchors(r).map_err(|_| "Subselect target geometry is unavailable")?.len()
            };
            let matrix = *matrices
                .get(&source.source_object_index())
                .ok_or("Subselect transform is absent")?;
            let brush = if hit_test { record::brush_of(r) } else { (None, None) };
            if brush.0 == Some("copy") && brush.1.as_ref().is_some_and(|g| dabs.anchored.contains(g)) {
                continue;
            }
            items.push(SubselectReadItem {
                stable_target: r.target.clone(),
                closed: if hit_test { r.geometry.closed } else { record::closed(r) },
                count,
                paint_count: (!hit_test && record::has_pressure(r)).then_some(r.geometry.segments.len()),
                matrix,
                bounds: hit_test.then(|| {
                    let own = record::painted_bounds(r);
                    match (brush.0, brush.1.as_ref().and_then(|g| dabs.union.get(g)), matrices.get(&source.source_object_index())) {
                        (Some("anchor"), Some(world), Some(m)) => widen(own, *world, m),
                        _ => own,
                    }
                }),
                role: hit_test.then(|| record::hit_role(r)),
                filled: hit_test.then(|| r.solid_fill().is_some() || r.fill_gradient().is_some()),
                stroke_width: hit_test.then(|| record::painted_stroke_width(r)),
                box_angle: if hit_test { record::stored_box_angle(r) } else { None },
                owner: (hit_test && record::hit_role(r) == "fill")
                    .then(|| record::linked_fill(r))
                    .flatten()
                    .and_then(|link| {
                        packet
                            .records()
                            .filter(|s| s.source_layer_index() == index)
                            .map(|s| s.record())
                            .find(|o| {
                                record::has_pressure(o)
                                    && record::linked_fill(o) == Some(link)
                                    && o.target.frame_scope == r.target.frame_scope
                            })
                            .map(|o| o.target.stroke_id.clone())
                    }),
                brush: brush.0,
                group: brush.1,
                style: if hit_test { record::style::listed(r) } else { None },
            });
        }
        if items.len() > 1024 {
            return Err("Subselect layer exceeds the selection bound");
        }
        Ok(SubselectReadProjection::new(
            packet.snapshot(),
            packet.frame(),
            layer_uid,
            items,
            preview,
        )
        .with_hit_test(hit_test))
    }
}

/// #2372 hit-test listing of the non-stroke scene items: a raster image and a text block, each with its painted world
/// bounds and its own stable target, in the position the plan paints it. They carry no editable geometry (count 0).
impl ObjectCompositionWitness {
    fn raster_hit_item(&self, layer: usize, at: usize) -> Option<SubselectReadItem> {
        let plan = self.plan();
        let source = plan.raster_sources().get(at).filter(|s| !s.placed && s.layer_index == layer)?;
        let target = plan
            .packet()
            .raster_records()
            .find(|r| r.source_index() == source.source_index)?
            .entry()
            .target
            .clone();
        let rect = self.raster_final_rect(at)?;
        // The listing's record space is the stored item's own frame (its centre and size, unrotated): bounds are the stored rect and
        // the matrix carries it to the painted rect (rotation about the final centre, the layer chain included), so a record-space
        // matrix a client conjugates with it is exactly what the item's batch transform applies to the stored centre and size.
        let record = plan.packet().raster_records().find(|r| r.source_index() == source.source_index)?;
        let ((cx, cy), (w, h)) = (record.entry().item.center(), record.entry().item.size());
        let (fx, fy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let (sin, cos) = rect.rotation.to_radians().sin_cos();
        let (kx, ky) = (rect.width / w, rect.height / h);
        let (a, b, c, d) = (cos * kx, sin * kx, -sin * ky, cos * ky);
        let matrix = [a, b, c, d, fx - (a * cx + c * cy), fy - (b * cx + d * cy)];
        let bounds = [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0];
        (bounds.iter().chain(&matrix).all(|v| v.is_finite()) && w > 0.0 && h > 0.0 && rect.width > 0.0 && rect.height > 0.0)
            .then(|| hit_item(target, bounds, matrix, "raster"))
            .map(|item| SubselectReadItem { style: record::style::raster(record.entry().item.opacity()), ..item })
    }
    fn text_hit_item(
        &self,
        layer: usize,
        source: &crate::object_render_scene::text_scene::TextSource,
    ) -> Option<SubselectReadItem> {
        if source.placed || source.layer_index != layer {
            return None;
        }
        let record = self
            .plan()
            .packet()
            .text_records()
            .find(|r| r.source_index() == source.source_index)?;
        let target = record.entry().target.clone();
        let turn = record.entry().block.rotation();
        // The block's painted glyph and decoration contours are already in world space; its record space is the stored anchor's
        // own frame, which the layer's parent chain carries to the world. An unposed layer keeps the identity (and the world box).
        let b = source.item.scene().contours().iter().fold(
            [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
            |b, c| [b[0].min(c.envelope[0]), b[1].min(c.envelope[1]), b[2].max(c.envelope[2]), b[3].max(c.envelope[3])],
        );
        let chain = subselect::layer_chain(&self.plan().native_parent_chains(), layer).ok();
        if turn != 0.0 {
            // #2267: a turned block's record space is its stored (unturned) frame about the anchor; the matrix is the turn then the layer's
            // chain, and the bounds are the contours' extent in that frame, so a client's conjugated gesture is what the batch applies.
            return turned_text_item(target, source.item.scene().contours(), record.entry().block.anchor(), turn, chain.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
        }
        let (bounds, matrix) = match chain {
            Some(m) if m != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] => {
                let det = m[0] * m[3] - m[1] * m[2];
                let back = |x: f64, y: f64| {
                    let (x, y) = (x - m[4], y - m[5]);
                    [(m[3] * x - m[2] * y) / det, (-m[1] * x + m[0] * y) / det]
                };
                let corners = [back(b[0], b[1]), back(b[2], b[1]), back(b[0], b[3]), back(b[2], b[3])];
                let fold = |at: usize, lo: bool| corners.iter().map(|p| p[at]).fold(if lo { f64::INFINITY } else { f64::NEG_INFINITY }, |a, v| if lo { a.min(v) } else { a.max(v) });
                ([fold(0, true), fold(1, true), fold(0, false), fold(1, false)], m)
            }
            _ => (b, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        };
        (bounds.iter().chain(&matrix).all(|v| v.is_finite())).then(|| hit_item(target, bounds, matrix, "text"))
    }
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    fn raster_final_rect(&self, at: usize) -> Option<crate::evaluation::motion_parent_chain::ImageRect> {
        self.rasters.as_ref()?.final_rect(at)
    }
    #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
    fn raster_final_rect(&self, _at: usize) -> Option<crate::evaluation::motion_parent_chain::ImageRect> {
        None
    }
}
/// `a` after `b` (`b` applied first) as `[a b c d e f]` affines.
fn after(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}
fn turned_text_item(
    target: crate::object_frame_packet::text::ObjectTarget,
    contours: &[crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::SceneContour],
    anchor: [f64; 2],
    degrees: f64,
    layer: [f64; 6],
) -> Option<SubselectReadItem> {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let turn = [cos, sin, -sin, cos, anchor[0] - cos * anchor[0] + sin * anchor[1], anchor[1] - sin * anchor[0] - cos * anchor[1]];
    let m = after(layer, turn);
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inverse = [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det, (m[2] * m[5] - m[3] * m[4]) / det, (m[1] * m[4] - m[0] * m[5]) / det];
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for c in contours {
        let e = c.contour.transformed_envelope(inverse).ok()?;
        b = [b[0].min(e[0]), b[1].min(e[1]), b[2].max(e[2]), b[3].max(e[3])];
    }
    (b.iter().chain(&m).all(|v| v.is_finite())).then(|| hit_item(target, b, m, "text"))
}
fn hit_item(
    stable_target: crate::object_frame_packet::text::ObjectTarget,
    bounds: [f64; 4],
    matrix: [f64; 6],
    role: &'static str,
) -> SubselectReadItem {
    SubselectReadItem {
        stable_target,
        closed: true,
        count: 0,
        paint_count: None,
        matrix,
        bounds: Some(bounds),
        role: Some(role),
        filled: Some(true),
        stroke_width: None,
        box_angle: None,
        owner: None,
        brush: None,
        group: None,
        style: None,
    }
}

/// #2372: the stamped copies of each texture brush of a layer, as one world box per brush group, and the groups whose anchor
/// is listed (only those copies drop out of the listing).
#[derive(Default)]
struct DabBounds {
    union: std::collections::HashMap<String, [f64; 4]>,
    anchored: std::collections::HashSet<String>,
}
fn dab_bounds(witness: &ObjectCompositionWitness, layer: usize, matrices: &std::collections::BTreeMap<usize, [f64; 6]>) -> DabBounds {
    let mut out = DabBounds::default();
    for command in witness.plan().commands().filter(|c| c.source().source_layer_index() == layer) {
        let source = command.source();
        let r = source.record();
        let (kind, Some(group)) = record::brush_of(r) else { continue };
        match kind {
            Some("anchor") => {
                out.anchored.insert(group);
            }
            Some("copy") => {
                let world = matrices.get(&source.source_object_index()).and_then(|m| through(m, record::painted_bounds(r)));
                if let Some(w) = world {
                    let b = out.union.entry(group).or_insert(w);
                    *b = [b[0].min(w[0]), b[1].min(w[1]), b[2].max(w[2]), b[3].max(w[3])];
                }
            }
            _ => {}
        }
    }
    out
}
/// The box of `b` (x0, y0, x1, y1) carried through the affine `m` (a, b, c, d, e, f), when every corner stays finite.
fn through(m: &[f64; 6], b: [f64; 4]) -> Option<[f64; 4]> {
    let mut out = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for (x, y) in [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])] {
        let (px, py) = (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        out = [out[0].min(px), out[1].min(py), out[2].max(px), out[3].max(py)];
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}
/// `own` (an anchor's record-space bounds) grown to also cover the world box `world`, mapped back through the inverse of `m`.
/// A singular or non-finite map leaves `own` as it was.
fn widen(own: [f64; 4], world: [f64; 4], m: &[f64; 6]) -> [f64; 4] {
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-12 {
        return own;
    }
    let inv = [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det, 0.0, 0.0];
    let inv = [inv[0], inv[1], inv[2], inv[3], -(inv[0] * m[4] + inv[2] * m[5]), -(inv[1] * m[4] + inv[3] * m[5])];
    match through(&inv, world) {
        Some(b) => [own[0].min(b[0]), own[1].min(b[1]), own[2].max(b[2]), own[3].max(b[3])],
        None => own,
    }
}
