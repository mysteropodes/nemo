//! Typed scene item style (L07-I / F16, G03a), pure and I/O free.
//!
//! The one place that turns a stored object record into the fill / stroke / paint-order triple the scene
//! consumer draws (`object_scene_style_emit.rs`). It joins the L07-A paint kernel and the L07-B stroke kernel
//! to the typed record bodies of the document model:
//! * `closed-solid-fill-cubic-path`: one solid fill, no stroke, fill first (exactly what the compositor drew
//!   before this leaf);
//! * `editable-vector-path` (L01-D `VectorBody`): nullable solid fill, nullable solid stroke, width, cap,
//!   join, miter limit, dash array and offset, paint order and item opacity.
//!
//! Baseline rules reproduced (buildSceneJson, src/js/engine-bridge.js at fbe4b8a3):
//! * the stroke exists only when an authored stroke paint exists (`hasRealStroke`), and the stroke fields
//!   are read only then; width is `(strokeWidth || 1) * strokeScale` with `strokeScale == 1` in the native
//!   world-space scene, so an authored width of 0 draws at width 1 (`StrokeRules::scene_width`);
//! * the bridge folds the item opacity into every paint alpha (`cssColorToRgba(color, opacity)`); native
//!   paints stay unquantized straight float RGBA, so the folded alpha is the product `a * opacity` and the
//!   channels are never rewritten;
//! * `paintOrder` is stroke first only for the exact string `strokeFirst` (absent means fill first);
//! * cap, join, miter limit, dash and dash offset go through the L07-B engine stroke (`engine_stroke`),
//!   including the baked dash offset.
//!
//! Absent, `null` and explicit values stay distinct: an absent fill or stroke slot is refused, an explicit
//! `null` is "no paint", an explicit zero miter limit is a limit of zero. Nothing is defaulted or clamped
//! here; a record that cannot be drawn is a typed `StyleUnavailable`, never a flat or missing shape.
//! The document model has no gradient field, so this module never produces a gradient fill; the emit module
//! draws one when a future producer supplies it through `FillResolution::Gradient`.

use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, PaintOrder, PaintRules, ScenePaintError, SceneSolid,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    EngineStrokeItem, SceneStroke, StrokeError, BASELINE as STROKE_RULES,
};
use crate::object_document::{ObjectRecord, SolidFill, VectorBody, VECTOR_FAMILY};
use serde_json::Number;

/// The legacy record family (one closed cubic contour with a required solid fill).
pub(crate) const LEGACY_FAMILY: &str = "closed-solid-fill-cubic-path";

/// An authored stroke: its straight RGBA paint (opacity already folded) and the resolved stroke style.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ItemStroke {
    pub(crate) paint: SceneSolid,
    pub(crate) stroke: SceneStroke,
}

/// Everything the scene consumer needs to draw one item besides its path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SceneItemStyle {
    pub(crate) fill: FillResolution,
    pub(crate) stroke: Option<ItemStroke>,
    pub(crate) order: PaintOrder,
}

impl SceneItemStyle {
    /// The legacy family: one solid fill, nothing else, the baseline default order.
    pub(crate) fn solid_fill(solid: SceneSolid) -> Self {
        Self {
            fill: FillResolution::Solid(solid),
            stroke: None,
            order: PaintOrder::FillFirst,
        }
    }
}

/// Why a record has no drawable style. Nothing is published for the record when one is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleUnavailable {
    UnknownFamily,
    MissingVectorBody,
    MissingFill,
    MissingStroke,
    NonSolidPaint,
    InconsistentStroke,
    InvalidOpacity,
    Paint(ScenePaintError),
    Stroke(StrokeError),
}

impl StyleUnavailable {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::UnknownFamily => "record family has no admitted scene style",
            Self::MissingVectorBody => "vector record carries no vector body",
            Self::MissingFill => "record carries no fill slot",
            Self::MissingStroke => "vector body carries no stroke slot",
            Self::NonSolidPaint => "only solid paint has a scene representation",
            Self::InconsistentStroke => "hasRealStroke disagrees with the stroke slot",
            Self::InvalidOpacity => "item opacity must be finite and within 0..1",
            Self::Paint(error) => error.message(),
            Self::Stroke(error) => error.message(),
        }
    }
}

/// Resolve the style of one stored record, or say exactly why it has none.
pub(crate) fn style_of_record(record: &ObjectRecord) -> Result<SceneItemStyle, StyleUnavailable> {
    match record.family.as_str() {
        LEGACY_FAMILY => {
            let fill = record.solid_fill().ok_or(StyleUnavailable::MissingFill)?;
            Ok(SceneItemStyle::solid_fill(solid_of(fill, 1.0)?))
        }
        VECTOR_FAMILY => vector_style(record),
        _ => Err(StyleUnavailable::UnknownFamily),
    }
}

fn vector_style(record: &ObjectRecord) -> Result<SceneItemStyle, StyleUnavailable> {
    let body = record.vector().ok_or(StyleUnavailable::MissingVectorBody)?;
    let opacity = body
        .opacity
        .as_f64()
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or(StyleUnavailable::InvalidOpacity)?;
    let flat = match record.fill.value() {
        None => return Err(StyleUnavailable::MissingFill),
        Some(None) => None,
        Some(Some(paint)) => Some(solid_of(paint, opacity)?),
    };
    let stroke_paint = match body.stroke.value() {
        None => return Err(StyleUnavailable::MissingStroke),
        Some(paint) => paint.as_ref(),
    };
    if stroke_paint.is_some() != body.has_real_stroke() {
        return Err(StyleUnavailable::InconsistentStroke);
    }
    let stroke = match stroke_paint {
        None => None,
        Some(paint) => Some(ItemStroke {
            paint: solid_of(paint, opacity)?,
            stroke: stroke_of(body)?,
        }),
    };
    let order = PaintRules::BASELINE.paint_order(body.paint_order.value().map(String::as_str));
    let fill = PaintRules::BASELINE
        .resolve_fill(None, flat, None)
        .map_err(StyleUnavailable::Paint)?;
    Ok(SceneItemStyle {
        fill,
        stroke,
        order,
    })
}

/// A solid paint with the item opacity folded into its alpha. Opacity 1 keeps the source numbers untouched.
fn solid_of(fill: &SolidFill, opacity: f64) -> Result<SceneSolid, StyleUnavailable> {
    if fill.kind != "solid" {
        return Err(StyleUnavailable::NonSolidPaint);
    }
    let solid = SceneSolid::from_fill(fill).map_err(StyleUnavailable::Paint)?;
    if opacity == 1.0 {
        return Ok(solid);
    }
    let unrepresentable = StyleUnavailable::Paint(ScenePaintError::UnrepresentableStraightRgba);
    let alpha = fill.a.as_f64().ok_or(unrepresentable)?;
    let folded = Number::from_f64(alpha * opacity).ok_or(unrepresentable)?;
    SceneSolid::from_fill(&SolidFill {
        kind: fill.kind.clone(),
        r: fill.r.clone(),
        g: fill.g.clone(),
        b: fill.b.clone(),
        a: folded,
    })
    .map_err(StyleUnavailable::Paint)
}

/// The L07-B composition of `scene_stroke`, minus the css color the native paint replaces.
fn stroke_of(body: &VectorBody) -> Result<SceneStroke, StyleUnavailable> {
    let width = body
        .stroke_width
        .as_f64()
        .ok_or(StyleUnavailable::Stroke(StrokeError::NonFiniteWidth))?;
    let mut dash = None;
    if let Some(entries) = body.dash_array.value() {
        let mut pattern = Vec::with_capacity(entries.len());
        for entry in entries {
            pattern.push(
                entry
                    .as_f64()
                    .ok_or(StyleUnavailable::Stroke(StrokeError::NonFiniteDashEntry))?,
            );
        }
        // The bridge sends `dashPattern`/`dashOffset` only for a non-empty `dashArray`.
        if !pattern.is_empty() {
            dash = Some(pattern);
        }
    }
    let item = EngineStrokeItem {
        width: Some(STROKE_RULES.scene_width(Some(width), false, 1.0)),
        cap: Some(body.stroke_cap.clone()),
        join: Some(body.stroke_join.clone()),
        miter_limit: body.miter_limit.value().and_then(Number::as_f64),
        dash_offset: if dash.is_some() {
            body.dash_offset.value().and_then(Number::as_f64)
        } else {
            None
        },
        dash_pattern: dash,
    };
    STROKE_RULES
        .engine_stroke(&item)
        .map_err(StyleUnavailable::Stroke)
}

#[cfg(test)]
#[path = "../tests/object_scene_style_item_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_style_item.rs"]
mod tests;
