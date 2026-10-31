//! Tight immutable Bezier path construction for the native parent pivot.
use crate::evaluation::motion_parent_chain::ChainEntry;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_vector_rest, is_vector_record, motion::apply_element_motion, record_layer_has_active_path_fx, EvalSegment,
};
use crate::object_frame_packet::subselect_record::paint_anchors;
use crate::object_frame_packet::ObjectRecord;
use crate::object_render_scene::text_scene::parent_chain::{ChainUnavailable, SceneParentChains};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use vello::kurbo::{BezPath, Rect, Shape};

/// Tight native Bezier bounds; never the control-point hull/envelope centre.
pub(super) fn curve_path(
    segments: &[([f64; 2], [f64; 2], [f64; 2])],
    closed: bool,
) -> Result<BezPath, ChainUnavailable> {
    if segments.is_empty() {
        return Err(ChainUnavailable::VectorGeometry);
    }
    if !segments
        .iter()
        .flat_map(|s| s.0.iter().chain(s.1.iter()).chain(s.2.iter()))
        .all(|v| v.is_finite())
    {
        return Err(ChainUnavailable::VectorGeometry);
    }
    let mut path = BezPath::new();
    path.move_to((segments[0].0[0], segments[0].0[1]));
    let ends = segments.len() + usize::from(closed);
    for i in 1..ends {
        let prev = segments[(i - 1) % segments.len()];
        let next = segments[i % segments.len()];
        path.curve_to(
            (prev.0[0] + prev.2[0], prev.0[1] + prev.2[1]),
            (next.0[0] + next.1[0], next.0[1] + next.1[1]),
            (next.0[0], next.0[1]),
        );
    }
    if closed {
        path.close_path();
    }
    Ok(path)
}

// One finite native geometry union for vector, raster and Text parent pivots.
impl SceneParentChains {
    pub(super) fn add(&mut self, layer: usize, rect: Rect) -> Result<(), ChainUnavailable> {
        if ![rect.x0, rect.y0, rect.x1, rect.y1]
            .iter()
            .all(|v| v.is_finite())
            || rect.x0 > rect.x1
            || rect.y0 > rect.y1
        {
            return Err(ChainUnavailable::MissingGeometry);
        }
        self.bounds
            .entry(layer)
            .and_modify(|old| *old = old.union(rect))
            .or_insert(rect);
        Ok(())
    }
    pub(super) fn add_media(&mut self, layer: usize, rect: Rect) -> Result<(), ChainUnavailable> {
        self.add(layer, rect)?;
        self.media.entry(layer).and_modify(|old| *old = old.union(rect)).or_insert(rect);
        Ok(())
    }
    pub(crate) fn chain(&self, layer: usize) -> Result<Vec<ChainEntry>, ChainUnavailable> {
        self.chain_with(layer, None)
    }
    /// #2372 a Direct Select preview on a layer that also paints Text or images: its own pivot is the layer's vector rest
    /// bounds with `edited` in place of its stored record, joined with the layer's Text/raster/video bounds, which is what the
    /// committed edit's chain reads; the vector-only listing does the same from its own rest. No preview of this layer: unchanged.
    pub(crate) fn chain_with_preview(&self, layer: usize, edited: &ObjectRecord) -> Result<Vec<ChainEntry>, ChainUnavailable> {
        if !self.needs_bounds(layer) {
            return self.chain(layer);
        }
        let uid = self.packet.source_layers()[layer].layer_uid();
        let mut own = self.media.get(&layer).copied();
        for (source, record) in self.packet.snapshot().document().objects().iter().enumerate() {
            if record.target().layer_uid() != uid || !self.matches(layer, record.target()) {
                continue;
            }
            let path = if record.target == edited.target {
                let anchors = paint_anchors(edited).map_err(|_| ChainUnavailable::VectorGeometry)?;
                let segments = anchors.into_iter().map(|a| EvalSegment { point: a.point, handle_in: a.handle_in, handle_out: a.handle_out }).collect();
                let segments = apply_element_motion(self.packet.snapshot(), edited, self.packet.frame(), segments)
                    .map_err(|_| ChainUnavailable::VectorGeometry)?;
                curve_path(&segments.iter().map(|s| (s.point, s.handle_in, s.handle_out)).collect::<Vec<_>>(), edited.geometry.closed)?
            } else {
                self.record_path(source, record)?
            };
            let rect = path.bounding_box();
            own = Some(own.map_or(rect, |o| o.union(rect)));
        }
        self.chain_with(layer, own)
    }
    /// One record's tight rest path for its layer pivot: the real rest evaluator (pathFX/paramShape before layer pose), or the
    /// existing strict contour admission for a legacy closed-cubic record.
    pub(super) fn record_path(&self, source: usize, record: &ObjectRecord) -> Result<BezPath, ChainUnavailable> {
        if is_vector_record(record) {
            let evaluated = evaluate_vector_rest(self.packet.snapshot(), source, self.packet.frame())
                .map_err(|_| ChainUnavailable::VectorGeometry)?;
            return curve_path(&evaluated.segments.iter().map(|s| (s.point, s.handle_in, s.handle_out)).collect::<Vec<_>>(), evaluated.closed);
        }
        if record_layer_has_active_path_fx(&self.packet, record) {
            return Err(ChainUnavailable::VectorGeometry);
        }
        let pair = |p: &crate::object_frame_packet::text::ObjectPoint| -> Result<[f64; 2], ChainUnavailable> {
            Ok([p.x.as_f64().filter(|v| v.is_finite()).ok_or(ChainUnavailable::VectorGeometry)?,
                p.y.as_f64().filter(|v| v.is_finite()).ok_or(ChainUnavailable::VectorGeometry)?])
        };
        let segments = record.geometry.segments.iter()
            .map(|s| Ok(CubicSegment { point: pair(&s.point)?, handle_in: pair(&s.handle_in)?, handle_out: pair(&s.handle_out)? }))
            .collect::<Result<Vec<_>, ChainUnavailable>>()?;
        Ok(ClosedCubicPath::new(segments).map_err(|_| ChainUnavailable::VectorGeometry)?.path().clone())
    }
}
