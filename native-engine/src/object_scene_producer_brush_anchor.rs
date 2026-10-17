//! Bitmap-brush anchor state and camouflage (L07-J; G17).
//!
//! A bitmap-brush stroke is ONE ordinary vector path (the anchor) plus ONE companion
//! bitmap (the ink). Baseline `applyBitmapBrushTexture` (`src/js/bitmap-brush.js` at
//! fbe4b8a3) hides the anchor's own stroke without losing it:
//! * an anchor WITH a fill keeps painting that fill; only its `strokeColor` goes, and the
//!   colour is remembered in `data.preTextureStroke`;
//! * an anchor WITHOUT a fill becomes invisible (`opacity = 0`) and the previous opacity is
//!   remembered in `data.preTextureOpacity`;
//! * a remembered value is written only by the FIRST application (`=== undefined` test), so
//!   `regenerate` (which re-applies the stored spec) is a fixed point.
//!
//! `AnchorRules::camouflage` is that rule on the persisted fields (colours are the
//! `colorHex8` strings the baseline stores, never interpreted here); it is the EDIT-side
//! kernel. `anchor_draw` is what the pinned scene shows for a STORED anchor, and it reads
//! ONLY the immutable record (its fill, its vector-body stroke and its opacity are exactly
//! what baseline `serP` stored after the camouflage): there is no second copy of the paint.
//! The source keeps just the remembered pre-texture values (`AnchorRestore`), which are never
//! drawn and exist only so a later "clear brush texture" can restore the authored look. They
//! are used to refuse a record that contradicts the camouflage the source remembers.
//!
//! `AnchorRules` carries the baseline switches so the controls can mutate each one and
//! prove a named test notices; `BASELINE` is the only value production callers use.

use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    ScenePaintError, SceneSolid,
};
use crate::object_document::SolidFill;
use serde_json::Number;

/// A remembered pre-camouflage value. `Unset` is JS `undefined`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Remembered<T> {
    Unset,
    Set(T),
}

/// The anchor members `applyBitmapBrushTexture` reads and writes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnchorState {
    /// `basePath.strokeColor` as `colorHex8`; `None` is `null`.
    pub(crate) stroke: Option<String>,
    /// `basePath.opacity`; `None` is JS `undefined`.
    pub(crate) opacity: Option<f64>,
    /// `data.preTextureStroke`: `Set(None)` is a remembered `null`.
    pub(crate) pre_stroke: Remembered<Option<String>>,
    /// `data.preTextureOpacity`.
    pub(crate) pre_opacity: Remembered<f64>,
}

impl AnchorState {
    /// The remembered members, which are all a scene source keeps of the anchor.
    pub(crate) fn restore(&self) -> AnchorRestore {
        AnchorRestore {
            pre_stroke: self.pre_stroke.clone(),
            pre_opacity: self.pre_opacity.clone(),
        }
    }
}

/// `data.preTextureStroke` / `data.preTextureOpacity` of a stored anchor: never drawn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnchorRestore {
    pub(crate) pre_stroke: Remembered<Option<String>>,
    pub(crate) pre_opacity: Remembered<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnchorRules {
    /// `if (data.preTexture* === undefined)`: only the first application remembers.
    pub(crate) remember_first_only: bool,
    /// A filled anchor loses only its stroke, an unfilled one only its opacity.
    pub(crate) fill_keeps_opacity: bool,
}

impl AnchorRules {
    pub(crate) const BASELINE: AnchorRules = AnchorRules {
        remember_first_only: true,
        fill_keeps_opacity: true,
    };

    /// The anchor after `applyBitmapBrushTexture` (or a re-application) for an anchor that
    /// does (`has_fill`) or does not carry a fill.
    pub(crate) fn camouflage(&self, has_fill: bool, state: &AnchorState) -> AnchorState {
        let mut next = state.clone();
        let hide_stroke = if self.fill_keeps_opacity {
            has_fill
        } else {
            !has_fill
        };
        if hide_stroke {
            if !self.remember_first_only || next.pre_stroke == Remembered::Unset {
                next.pre_stroke = Remembered::Set(state.stroke.clone());
            }
            next.stroke = None;
        } else {
            if !self.remember_first_only || next.pre_opacity == Remembered::Unset {
                next.pre_opacity = Remembered::Set(state.opacity.unwrap_or(1.0));
            }
            next.opacity = Some(0.0);
        }
        next
    }
}

/// The baseline camouflage rule.
pub(crate) fn camouflage(has_fill: bool, state: &AnchorState) -> AnchorState {
    AnchorRules::BASELINE.camouflage(has_fill, state)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnchorError {
    /// The record's fill or stroke is not a representable straight RGBA.
    Fill(ScenePaintError),
    /// The record's opacity is NaN or infinite (the document codec admits 0..1 only).
    OpacityNotFinite,
    /// The record contradicts the camouflage the source remembers: a filled anchor keeps
    /// no stroke and remembered one, an unfilled anchor is invisible and remembered its
    /// opacity. The text names the contradiction.
    CamouflageDisagrees(&'static str),
}

/// What the scene shows for the anchor. The remembered pre-texture values are not here.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnchorDraw {
    pub(crate) fill: Option<SceneSolid>,
    /// The record's vector-body stroke: `None` for a camouflaged filled anchor, the kept
    /// authored stroke (drawn at opacity 0) for an unfilled one.
    pub(crate) stroke: Option<SceneSolid>,
    /// The record's vector-body opacity.
    pub(crate) opacity: f64,
}

fn paint(paint: Option<&SolidFill>) -> Result<Option<SceneSolid>, AnchorError> {
    paint
        .map(SceneSolid::from_fill)
        .transpose()
        .map_err(AnchorError::Fill)
}

/// The pinned view of a stored anchor, read from its immutable record (`fill`, and the
/// vector body's `stroke` and `opacity`), checked against what the source remembers.
pub(crate) fn anchor_draw(
    fill: Option<&SolidFill>,
    stroke: Option<&SolidFill>,
    opacity: &Number,
    restore: &AnchorRestore,
) -> Result<AnchorDraw, AnchorError> {
    let (fill_draw, stroke_draw) = (paint(fill)?, paint(stroke)?);
    let opacity = opacity
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or(AnchorError::OpacityNotFinite)?;
    if fill.is_some() {
        if stroke.is_some() {
            return Err(AnchorError::CamouflageDisagrees(
                "filled-anchor-keeps-a-stroke",
            ));
        }
        if restore.pre_stroke == Remembered::Unset {
            return Err(AnchorError::CamouflageDisagrees(
                "filled-anchor-without-remembered-stroke",
            ));
        }
    } else {
        if opacity != 0.0 {
            return Err(AnchorError::CamouflageDisagrees(
                "unfilled-anchor-is-visible",
            ));
        }
        if restore.pre_opacity == Remembered::Unset {
            return Err(AnchorError::CamouflageDisagrees(
                "unfilled-anchor-without-remembered-opacity",
            ));
        }
    }
    Ok(AnchorDraw {
        fill: fill_draw,
        stroke: stroke_draw,
        opacity,
    })
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_anchor.rs"]
mod tests;
