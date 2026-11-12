//! #2358 GHOST-FIDELITY: the items of one evaluated ghost frame that belong to the ghosted layer, as underlay pieces.
//! Split from `object_ghost_underlay.rs`, whose private `Piece` list it extends.
use crate::object_frame_packet::ObjectRecord;
use crate::object_render_scene::ghost_underlay::{
    GhostFrameSpec, GhostSource, GhostUnderlay, Piece,
};
use crate::object_render_scene::object_scene_style_emit::PreparedItem;
use crate::object_render_scene::{ordered_entries, ObjectCompositionWitness, PaintEntry};
use vello::kurbo::{Affine, BezPath};
use vello::peniko::Color;

impl GhostUnderlay {
    /// The text glyphs, images, brush dabs and evaluated component / duplicator items of one evaluated ghost frame that
    /// belong to the ghosted layer (a derived layer owns no stored key geometry, so these ARE its ghost).
    pub(super) fn collect_items(
        &mut self,
        source: &dyn GhostSource,
        index: usize,
        at: &GhostFrameSpec,
        witness: &ObjectCompositionWitness,
    ) {
        let layer_uid = source.layer_uid();
        let plan = witness.plan();
        let layers = plan.packet().source_layers();
        let ours = |layer_index: usize| {
            layers
                .get(layer_index)
                .is_some_and(|l| l.layer_uid() == layer_uid)
        };
        let shift = Affine::translate((at.translation[0], at.translation[1]));
        let derived =
            |this: &mut Self, item: &PreparedItem, path: &BezPath, record: &ObjectRecord| {
                let visual = source.record_visual(index, record);
                this.pieces.push(Piece::Prepared {
                    item: item.clone(),
                    path: path.clone(),
                    at: shift,
                    alpha: visual.opacity as f32,
                    tint: visual.stroke.map(|([r, g, b, a], width)| {
                        (Color::new([r as f32, g as f32, b as f32, a as f32]), width)
                    }),
                    clear_fill: visual.clear_fill,
                });
            };
        // Baseline `renderGhostAll` draws a key frame's stored strokes in one purple outline: the produced brush items
        // join them that way, and text, rasters, components and duplicator copies stay out of it.
        let key = at.all_keys;
        let outline = at
            .key_outline
            .map(|[r, g, b, a]| Color::new([r as f32, g as f32, b as f32, a.clamp(0., 1.) as f32]));
        for entry in ordered_entries(plan) {
            match entry {
                PaintEntry::Text(_)
                | PaintEntry::Raster(_)
                | PaintEntry::Component(_)
                | PaintEntry::Duplicator(_)
                    if key => {}
                PaintEntry::Text(source) if ours(source.layer_index) => {
                    for (contour, [r, g, b, a]) in source.item.painted() {
                        let base = Color::from_rgba8(r, g, b, a);
                        let fill = (!at.text_clear_fill)
                            .then(|| base.with_alpha(base.components[3] * at.text_opacity as f32));
                        let stroke = at.text_stroke.map(|([tr, tg, tb, ta], width)| {
                            (
                                Color::new([
                                    tr as f32,
                                    tg as f32,
                                    tb as f32,
                                    (ta * at.text_opacity).clamp(0., 1.) as f32,
                                ]),
                                width,
                            )
                        });
                        self.pieces.push(Piece::Shape {
                            path: contour.contour.path().clone(),
                            fill,
                            stroke,
                            at: shift,
                        });
                    }
                }
                PaintEntry::Raster(i)
                    if plan
                        .raster_sources()
                        .get(i)
                        .is_some_and(|s| ours(s.layer_index)) =>
                {
                    if let Some((image, transform, opacity)) = witness.raster_image(i) {
                        self.pieces.push(Piece::Image {
                            image: image.clone(),
                            transform,
                            alpha: opacity * at.raster_opacity as f32,
                        });
                    }
                }
                // Brush-texture dabs: the baseline draws them in their own look at `op * dab opacity` (the dab's
                // colour alpha already is its opacity), never tinted. The anchor itself is the invisible stored path.
                PaintEntry::Dab(dab) if ours(dab.layer_index) => {
                    self.pieces.push(Piece::Prepared {
                        item: dab.prepared.clone(),
                        path: dab.path.clone(),
                        at: shift,
                        alpha: at.raster_opacity as f32,
                        tint: outline.map(|c| (c, None)),
                        clear_fill: key,
                    })
                }
                PaintEntry::Brush(i)
                    if plan
                        .brush_sources()
                        .and_then(|b| b.paints.get(i))
                        .is_some_and(|b| ours(b.layer_index)) =>
                {
                    if let Ok(Some((image, transform, opacity))) = witness.brush_image(i) {
                        self.pieces.push(Piece::Image {
                            image: image.clone(),
                            transform,
                            alpha: opacity * at.raster_opacity as f32,
                        });
                    }
                }
                PaintEntry::Component(c) if ours(c.host_layer_index) => {
                    derived(self, &c.prepared, &c.path, &c.record)
                }
                PaintEntry::Duplicator(d) if ours(d.layer_index) => {
                    derived(self, &d.prepared, &d.path, &d.record)
                }
                _ => {}
            }
        }
    }
}
