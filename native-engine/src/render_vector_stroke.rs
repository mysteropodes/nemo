//! Stroke-aware scene metadata of the editable vector family (G14, #2107, leaf L01-I), pure and I/O free.
//!
//! The typed per-item style (fill, stroke, paint order, opacity) is L07-I's `style_of_record`; the drawing is L07-I's
//! `PreparedItem::draw`. What the vector scene join needs on top of them, and nothing else, is decided here:
//! * `primary_solid`: the one `SceneSolid` every scene entry retains for `ObjectPaintCommand::solid()/paint()`. A filled item
//!   retains its fill (exactly what the legacy family retained), a fill-less item with an authored stroke retains the stroke
//!   paint, and an item with neither retains a transparent solid: an invisible vector is a valid item that draws nothing, not
//!   an error and not an opaque default. It is metadata only; the compositor draws through the typed style.
//! * `stroke_reach`: how far an authored stroke can paint beyond the control bounds of its path, used for the conservative
//!   clipping envelope. Half the width times the widest of: 1 (round and butt caps, bevel and round joins), sqrt(2) (a square
//!   cap reaches the corner of its square), and the miter limit (a miter tip is at most `limit * width / 2` from its vertex),
//!   plus one pixel of anti-aliasing margin. Dashes never extend a stroke. No stroke is `None`, which is not zero reach.
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    ScenePaintError, SceneSolid,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    SceneCap, SceneJoin,
};
use crate::object_document::SolidFill;
use serde_json::Number;

/// Anti-aliasing margin, in pixels, added to every envelope (with or without a stroke).
pub(crate) const ENVELOPE_MARGIN: f64 = 1.0;

/// The solid retained with a scene entry: the fill, else the stroke paint, else transparent.
pub(crate) fn primary_solid(style: &SceneItemStyle) -> Result<SceneSolid, ScenePaintError> {
    if let Some(solid) = style.fill.solid() {
        return Ok(solid.clone());
    }
    if let Some(item) = &style.stroke {
        return Ok(item.paint.clone());
    }
    SceneSolid::from_fill(&SolidFill {
        kind: "solid".to_owned(),
        r: Number::from(0u8),
        g: Number::from(0u8),
        b: Number::from(0u8),
        a: Number::from(0u8),
    })
}

/// Distance an authored stroke can reach beyond the control bounds of its path; `None` when the item has no stroke.
pub(crate) fn stroke_reach(style: &SceneItemStyle) -> Option<f64> {
    let stroke = &style.stroke.as_ref()?.stroke;
    let mut factor: f64 = 1.0;
    if stroke.cap == SceneCap::Square {
        factor = factor.max(std::f64::consts::SQRT_2);
    }
    if stroke.join == SceneJoin::Miter {
        factor = factor.max(stroke.effective_miter_limit());
    }
    Some(stroke.width / 2.0 * factor + ENVELOPE_MARGIN)
}

#[cfg(test)]
#[path = "../tests/render_vector_stroke.rs"]
mod tests;
