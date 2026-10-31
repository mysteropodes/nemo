//! Typed scene style to Vello drawing (L07-I / F16, G03a): the compositor's real consumer of L07-A and L07-B.
//!
//! `prepare_item` turns one `SceneItemStyle` into Vello values (straight-float `Color`s, a kurbo `Stroke`, a
//! peniko `Gradient`) and refuses everything it cannot draw; `PreparedItem::draw` then paints one path in the
//! baseline order. Preparation is separate from drawing so the compositor can validate every item before it
//! replaces the retained scene or allocates a target, exactly as it already did for solid fills.
//!
//! Baseline semantics (geometry-wasm/src/engine.rs at fbe4b8a3, `paint_layer_items`):
//! * fill and stroke both use the item path, NonZero fill, the item transform, no brush transform;
//! * `paint_order == "strokeFirst"` paints the stroke and then the fill, anything else fill then stroke
//!   (`PaintOrder::steps`, L07-A);
//! * the stroke is `Stroke::new(width)` with the cap on both ends, the join, the miter limit when present and
//!   the already baked dash pattern at offset 0 when present (`SceneStroke`, L07-B);
//! * a gradient is `Gradient::new_linear(from, to)` or `new_radial(center, radius)` with the stops
//!   `(offset, rgba8)`; the extend mode, interpolation space and alpha space stay peniko's defaults.
//!
//! Interpretation (lane acceptance "unavailable geometry never silently disappears"): the baseline keeps the
//! flat fill, or nothing, when a gradient cannot be built (`gradient_brush` returns `None`). A scene that
//! asked for a gradient must not degrade to a flat or missing fill, so `FillResolution::Fallback` is the typed
//! `EmitError::GradientRejected` instead. Stored two-stop vector gradients now use this same emitter.
//! For the same reason a stroke that Vello would skip silently (width
//! zero: `Scene::stroke` returns early) or never finish (a dash pattern without a positive finite length) is
//! a typed error; the style kernel's `width || 1` rule already turns an authored zero width into width 1.

use crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, GradientGeometry, GradientRejection, PaintStep, SceneGradient, ScenePaintError,
    SceneSolid,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    SceneCap, SceneJoin, SceneStroke,
};
use vello::kurbo::{Affine, BezPath, Cap, Join, Stroke};
use vello::peniko::{Color, Fill, Gradient};
use vello::Scene;

/// Why a style has no Vello representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmitError {
    Paint(ScenePaintError),
    GradientRejected(GradientRejection),
    /// Vello drops a stroke of width zero without a trace; a negative or non-finite width has no meaning.
    StrokeWidth,
    /// A dash pattern without a positive finite length would never advance along the path.
    DashLength,
}

impl EmitError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Paint(error) => error.message(),
            Self::GradientRejected(GradientRejection::MissingGeometry) => {
                "gradient geometry is missing"
            }
            Self::GradientRejected(GradientRejection::TooFewStops) => {
                "gradient needs at least two stops"
            }
            Self::StrokeWidth => "stroke width must be positive and finite",
            Self::DashLength => "dash pattern needs a positive finite length",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PreparedFill {
    Solid(Color),
    Gradient(Gradient),
}

/// Vello-ready paint for one item; holds no reference to the document or the scene.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedItem {
    fill: Option<PreparedFill>,
    stroke: Option<(Stroke, Color)>,
    order: [PaintStep; 2],
}

/// Validate and convert one style. Nothing is drawn and nothing is retained.
pub(crate) fn prepare_item(style: &SceneItemStyle) -> Result<PreparedItem, EmitError> {
    let fill = match &style.fill {
        FillResolution::Empty => None,
        FillResolution::Solid(solid) => Some(PreparedFill::Solid(color_of(solid)?)),
        FillResolution::Gradient(gradient) => Some(PreparedFill::Gradient(gradient_of(gradient))),
        FillResolution::Fallback { rejection, .. } => {
            return Err(EmitError::GradientRejected(*rejection))
        }
    };
    let stroke = match &style.stroke {
        None => None,
        Some(item) => Some((stroke_of(&item.stroke)?, color_of(&item.paint)?)),
    };
    Ok(PreparedItem {
        fill,
        stroke,
        order: style.order.steps(),
    })
}

impl PreparedItem {
    /// Paint the item in its recorded order. Fill and stroke share the path and transform.
    pub(crate) fn draw(&self, scene: &mut Scene, transform: Affine, path: &BezPath) {
        for step in self.order {
            match step {
                PaintStep::Fill => match &self.fill {
                    None => {}
                    Some(PreparedFill::Solid(color)) => {
                        scene.fill(Fill::NonZero, transform, *color, None, path)
                    }
                    Some(PreparedFill::Gradient(gradient)) => {
                        scene.fill(Fill::NonZero, transform, gradient, None, path)
                    }
                },
                PaintStep::Stroke => {
                    if let Some((stroke, color)) = &self.stroke {
                        scene.stroke(stroke, transform, *color, None, path);
                    }
                }
            }
        }
    }
}

impl PreparedItem {
    /// #2358 GHOST-FIDELITY: the item as an onion / Ghost All ghost. Fill and stroke fade to `alpha`; `tint` (already
    /// carrying its own alpha) replaces the stroke paint, keeping the item's stroke style and, when it has none or
    /// `width` is given, the width; `clear_fill` drops the fill (Outline mode). Same order, path and transform as `draw`.
    pub(crate) fn draw_ghost(
        &self,
        scene: &mut Scene,
        transform: Affine,
        path: &BezPath,
        alpha: f32,
        tint: Option<(Color, Option<f64>)>,
        clear_fill: bool,
    ) {
        for step in self.order {
            match step {
                PaintStep::Fill if clear_fill => {}
                PaintStep::Fill => match &self.fill {
                    None => {}
                    Some(PreparedFill::Solid(color)) => scene.fill(
                        Fill::NonZero,
                        transform,
                        color.multiply_alpha(alpha),
                        None,
                        path,
                    ),
                    Some(PreparedFill::Gradient(gradient)) => scene.fill(
                        Fill::NonZero,
                        transform,
                        &gradient.clone().multiply_alpha(alpha),
                        None,
                        path,
                    ),
                },
                PaintStep::Stroke => match (&tint, &self.stroke) {
                    (Some((paint, width)), own) => {
                        let mut stroke = own
                            .as_ref()
                            .map_or_else(|| Stroke::new(1.), |(stroke, _)| stroke.clone());
                        if let Some(width) = width {
                            stroke.width = *width;
                        }
                        scene.stroke(&stroke, transform, *paint, None, path);
                    }
                    (None, Some((stroke, color))) => {
                        scene.stroke(stroke, transform, color.multiply_alpha(alpha), None, path)
                    }
                    (None, None) => {}
                },
            }
        }
    }
}

fn color_of(solid: &SceneSolid) -> Result<Color, EmitError> {
    // AlphaColor<Srgb> holds straight float channels; do not quantize to RGB8.
    // The channel rule lives once, in the L07-A SceneSolid kernel.
    let rgba = solid.rgba_f32().map_err(EmitError::Paint)?;
    Ok(Color::new(rgba))
}

fn stroke_of(resolved: &SceneStroke) -> Result<Stroke, EmitError> {
    if !(resolved.width.is_finite() && resolved.width > 0.0) {
        return Err(EmitError::StrokeWidth);
    }
    if let Some(pattern) = &resolved.dash {
        let length: f64 = pattern.iter().sum();
        if !(length.is_finite() && length > 0.0) {
            return Err(EmitError::DashLength);
        }
    }
    let cap = match resolved.cap {
        SceneCap::Butt => Cap::Butt,
        SceneCap::Round => Cap::Round,
        SceneCap::Square => Cap::Square,
    };
    let join = match resolved.join {
        SceneJoin::Bevel => Join::Bevel,
        SceneJoin::Miter => Join::Miter,
        SceneJoin::Round => Join::Round,
    };
    let mut stroke = Stroke::new(resolved.width).with_caps(cap).with_join(join);
    if let Some(limit) = resolved.miter_limit {
        stroke = stroke.with_miter_limit(limit);
    }
    if let Some(pattern) = &resolved.dash {
        stroke = stroke.with_dashes(resolved.dash_offset(), pattern.iter());
    }
    Ok(stroke)
}

fn gradient_of(scene: &SceneGradient) -> Gradient {
    let gradient = match scene.geometry {
        GradientGeometry::Linear { from, to } => {
            Gradient::new_linear((from[0], from[1]), (to[0], to[1]))
        }
        GradientGeometry::Radial { center, radius } => {
            Gradient::new_radial((center[0], center[1]), radius)
        }
    };
    let stops: Vec<(f32, Color)> = scene
        .stops
        .iter()
        .map(|stop| {
            let [r, g, b, a] = stop.color;
            (stop.offset, Color::from_rgba8(r, g, b, a))
        })
        .collect();
    gradient.with_stops(stops.as_slice())
}

#[cfg(test)]
#[path = "../tests/object_scene_style_emit_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_style_emit_rows.rs"]
pub(crate) mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_style_emit.rs"]
mod tests;
