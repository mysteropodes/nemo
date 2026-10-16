//! Typed scene paint kernel (L07-A / F16), pure and I/O free.
//!
//! Joins the baseline paint rules to the immutable native `SolidFill`:
//! - `SceneSolid` keeps the four original `serde_json::Number` values of the
//!   source fill (no quantization, no premultiplication) and refuses a channel
//!   outside `[0, 1]` with the same predicate the compositor's `color()`
//!   applies, so a record the compositor would reject never becomes a scene paint;
//! - `css_color_to_rgba` ports `cssColorToRgba` of `src/js/engine-bridge.js` at
//!   fbe4b8a3 including its UTF-16 length rule, first-`#` removal, `parseInt`
//!   radix-16 leniency (whitespace, sign, `0x`), the `hexA * opacity` product and
//!   the 0..255 clamp. A value the Rust `[u8; 4]` channels reject in the baseline
//!   (negative channel, NaN alpha: JSON `null`) is `CssColor::Unrepresentable`;
//! - `GradientInput` mirrors `gradGeomOk` (gradient-bridge.js) and engine.rs
//!   `gradient_brush`: missing geometry or an empty stop list is
//!   `MissingGeometry`, fewer than two stops is `TooFewStops`; exactly
//!   `"radial"` is radial (`radius = hypot(to - from) as f32`, floored at 0.01),
//!   every other name is linear; stops carry `(offset as f32, rgba8)` and a stop
//!   whose css color is unparsable is transparent black (`|| [0,0,0,0]`);
//! - `resolve_fill` keeps `Empty`, `Solid`, `Gradient` and a gradient `Fallback`
//!   (to the flat fill, or to nothing) as distinct typed outcomes;
//! - `PaintOrder`: only the exact string `strokeFirst` is stroke-first.
//!
//! Non-finite gradient coordinates or offsets are a typed error: the baseline
//! serializes them as JSON `null`, which fails Rust deserialization and takes the
//! whole render down. `PaintRules` carries the five baseline constants so the
//! controls can mutate each one and prove a named test notices.

use crate::object_document::SolidFill;
use serde_json::Number;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScenePaintError {
    UnrepresentableStraightRgba,
    NonFiniteGradientNumber,
    UnrepresentableCssColor,
}

impl ScenePaintError {
    /// `UnrepresentableStraightRgba` carries the compositor's exact message.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::UnrepresentableStraightRgba => "unrepresentable object straight RGBA",
            Self::NonFiniteGradientNumber => "gradient coordinates and offsets must be finite",
            Self::UnrepresentableCssColor => "css color is not representable as u8 channels",
        }
    }
}

/// Original straight RGBA of a `SolidFill`, validated but never rewritten.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SceneSolid {
    r: Number,
    g: Number,
    b: Number,
    a: Number,
}

impl SceneSolid {
    pub(crate) fn from_fill(fill: &SolidFill) -> Result<Self, ScenePaintError> {
        let solid = Self {
            r: fill.r.clone(),
            g: fill.g.clone(),
            b: fill.b.clone(),
            a: fill.a.clone(),
        };
        solid.rgba_f32()?;
        Ok(solid)
    }

    pub(crate) fn channels(&self) -> [&Number; 4] {
        [&self.r, &self.g, &self.b, &self.a]
    }

    /// Same conversion as the compositor's `color()` before `Color::new`.
    pub(crate) fn rgba_f32(&self) -> Result<[f32; 4], ScenePaintError> {
        let mut rgba = [0.0f32; 4];
        for (target, source) in rgba.iter_mut().zip(self.channels()) {
            let value = source
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                .ok_or(ScenePaintError::UnrepresentableStraightRgba)?;
            *target = value as f32;
        }
        Ok(rgba)
    }
}

/// Result of `cssColorToRgba` when it does not return `null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CssColor {
    Rgba([u8; 4]),
    /// A negative channel (`parseInt("-f", 16)`) or a NaN alpha: the baseline
    /// array exists but its JSON cannot become `[u8; 4]`.
    Unrepresentable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GradientKind {
    Linear,
    Radial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GradientGeometry {
    Linear { from: [f64; 2], to: [f64; 2] },
    Radial { center: [f64; 2], radius: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SceneStop {
    pub(crate) offset: f32,
    pub(crate) color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SceneGradient {
    pub(crate) geometry: GradientGeometry,
    pub(crate) stops: Vec<SceneStop>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StopInput<'a> {
    pub(crate) offset: f64,
    /// `None` is a stop whose css color is absent/unparsable (transparent black).
    pub(crate) color: Option<&'a str>,
}

/// Loosely typed gradient as the bridge sees it: `from`/`to`/`stops` may be absent.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GradientInput<'a> {
    pub(crate) kind: &'a str,
    pub(crate) from: Option<&'a [f64]>,
    pub(crate) to: Option<&'a [f64]>,
    pub(crate) stops: Option<&'a [StopInput<'a>]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GradientRejection {
    /// `gradGeomOk` false: from/to shorter than 2 or absent, stops absent/empty.
    MissingGeometry,
    /// engine.rs `gradient_brush`: fewer than two stops has no ramp.
    TooFewStops,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GradientOutcome {
    Ready(SceneGradient),
    Rejected(GradientRejection),
}

/// Distinct typed fill outcomes; a rejected gradient names why it fell back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FillResolution {
    Empty,
    Solid(SceneSolid),
    Gradient(SceneGradient),
    Fallback {
        rejection: GradientRejection,
        flat: Option<SceneSolid>,
    },
}

impl FillResolution {
    pub(crate) fn gradient(&self) -> Option<&SceneGradient> {
        match self {
            Self::Gradient(gradient) => Some(gradient),
            _ => None,
        }
    }

    /// The flat paint that is actually drawn (directly or after a fallback).
    pub(crate) fn solid(&self) -> Option<&SceneSolid> {
        match self {
            Self::Solid(solid) => Some(solid),
            Self::Fallback { flat, .. } => flat.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn fallback(&self) -> Option<GradientRejection> {
        match self {
            Self::Fallback { rejection, .. } => Some(*rejection),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintOrder {
    FillFirst,
    StrokeFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintStep {
    Fill,
    Stroke,
}

impl PaintOrder {
    pub(crate) fn steps(self) -> [PaintStep; 2] {
        match self {
            Self::FillFirst => [PaintStep::Fill, PaintStep::Stroke],
            Self::StrokeFirst => [PaintStep::Stroke, PaintStep::Fill],
        }
    }
}

/// The baseline constants, parameterized only so controls can mutate them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PaintRules {
    pub(crate) min_stops: usize,
    pub(crate) radial_name: &'static str,
    pub(crate) radius_floor: f32,
    pub(crate) alpha_ceiling: f64,
    pub(crate) stroke_first_name: &'static str,
}

/// `gradGeomOk`: from/to with at least two numbers and a non-empty stop list.
pub(crate) fn gradient_geometry_ok(input: &GradientInput<'_>) -> bool {
    matches!(
        (input.from, input.to, input.stops),
        (Some(f), Some(t), Some(s)) if f.len() >= 2 && t.len() >= 2 && !s.is_empty()
    )
}

impl PaintRules {
    pub(crate) const BASELINE: PaintRules = PaintRules {
        min_stops: 2,
        radial_name: "radial",
        radius_floor: 0.01,
        alpha_ceiling: 255.0,
        stroke_first_name: "strokeFirst",
    };

    pub(crate) fn gradient_kind(&self, name: &str) -> GradientKind {
        if name == self.radial_name {
            GradientKind::Radial
        } else {
            GradientKind::Linear
        }
    }

    pub(crate) fn paint_order(&self, name: Option<&str>) -> PaintOrder {
        if name == Some(self.stroke_first_name) {
            PaintOrder::StrokeFirst
        } else {
            PaintOrder::FillFirst
        }
    }

    pub(crate) fn radial_radius(&self, from: [f64; 2], to: [f64; 2]) -> f32 {
        let radius = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt() as f32;
        radius.max(self.radius_floor)
    }

    /// `cssColorToRgba(css, opacity)`; `None` is the baseline's `null`.
    pub(crate) fn css_color_to_rgba(
        &self,
        css: Option<&str>,
        opacity: Option<f64>,
    ) -> Option<CssColor> {
        let css = css.filter(|s| !s.is_empty())?;
        let mut units: Vec<u16> = css.encode_utf16().collect();
        if let Some(at) = units.iter().position(|&u| u == 0x23) {
            units.remove(at);
        }
        if units.len() == 3 {
            units = units.iter().flat_map(|&u| [u, u]).collect();
        }
        if units.len() != 6 && units.len() != 8 {
            return None;
        }
        let (Some(r), Some(g), Some(b)) = (
            parse_int16(&units[0..2]),
            parse_int16(&units[2..4]),
            parse_int16(&units[4..6]),
        ) else {
            return None;
        };
        let hex_a = if units.len() == 8 {
            parse_int16(&units[6..8]).map_or(f64::NAN, |v| f64::from(v) / 255.0)
        } else {
            1.0
        };
        let scaled = (255.0 * opacity.unwrap_or(1.0) * hex_a).round();
        if r < 0 || g < 0 || b < 0 || scaled.is_nan() {
            return Some(CssColor::Unrepresentable);
        }
        let alpha = scaled.min(self.alpha_ceiling).max(0.0) as u8;
        Some(CssColor::Rgba([r as u8, g as u8, b as u8, alpha]))
    }

    /// Validate and convert one gradient; rejections are typed, not errors.
    pub(crate) fn prepare_gradient(
        &self,
        input: &GradientInput<'_>,
        opacity: Option<f64>,
    ) -> Result<GradientOutcome, ScenePaintError> {
        if !gradient_geometry_ok(input) {
            return Ok(GradientOutcome::Rejected(
                GradientRejection::MissingGeometry,
            ));
        }
        let (Some(from), Some(to), Some(stops)) = (input.from, input.to, input.stops) else {
            return Ok(GradientOutcome::Rejected(
                GradientRejection::MissingGeometry,
            ));
        };
        if stops.len() < self.min_stops {
            return Ok(GradientOutcome::Rejected(GradientRejection::TooFewStops));
        }
        let from = [from[0], from[1]];
        let to = [to[0], to[1]];
        if !from.iter().chain(to.iter()).all(|v| v.is_finite()) {
            return Err(ScenePaintError::NonFiniteGradientNumber);
        }
        let mut scene_stops = Vec::with_capacity(stops.len());
        for stop in stops {
            if !stop.offset.is_finite() {
                return Err(ScenePaintError::NonFiniteGradientNumber);
            }
            let color = match self.css_color_to_rgba(stop.color, opacity) {
                None => [0, 0, 0, 0],
                Some(CssColor::Rgba(rgba)) => rgba,
                Some(CssColor::Unrepresentable) => {
                    return Err(ScenePaintError::UnrepresentableCssColor)
                }
            };
            scene_stops.push(SceneStop {
                offset: stop.offset as f32,
                color,
            });
        }
        let geometry = match self.gradient_kind(input.kind) {
            GradientKind::Radial => GradientGeometry::Radial {
                center: from,
                radius: self.radial_radius(from, to),
            },
            GradientKind::Linear => GradientGeometry::Linear { from, to },
        };
        Ok(GradientOutcome::Ready(SceneGradient {
            geometry,
            stops: scene_stops,
        }))
    }

    /// engine.rs `paint_fill`: a ready gradient wins; otherwise the flat fill.
    pub(crate) fn resolve_fill(
        &self,
        gradient: Option<&GradientInput<'_>>,
        flat: Option<SceneSolid>,
        opacity: Option<f64>,
    ) -> Result<FillResolution, ScenePaintError> {
        if let Some(input) = gradient {
            return Ok(match self.prepare_gradient(input, opacity)? {
                GradientOutcome::Ready(gradient) => FillResolution::Gradient(gradient),
                GradientOutcome::Rejected(rejection) => {
                    FillResolution::Fallback { rejection, flat }
                }
            });
        }
        Ok(match flat {
            Some(solid) => FillResolution::Solid(solid),
            None => FillResolution::Empty,
        })
    }
}

/// ECMAScript `StrWhiteSpaceChar` (WhiteSpace and LineTerminator) code units.
fn is_js_space(unit: u16) -> bool {
    matches!(
        unit,
        0x09..=0x0d
            | 0x20
            | 0xa0
            | 0x1680
            | 0x2000..=0x200a
            | 0x2028
            | 0x2029
            | 0x202f
            | 0x205f
            | 0x3000
            | 0xfeff
    )
}

fn hex_digit(unit: u16) -> Option<i32> {
    match unit {
        0x30..=0x39 => Some(i32::from(unit - 0x30)),
        0x41..=0x46 => Some(i32::from(unit - 0x41) + 10),
        0x61..=0x66 => Some(i32::from(unit - 0x61) + 10),
        _ => None,
    }
}

/// `parseInt(units, 16)` over at most a few code units; `None` is NaN, and
/// `-0` reads as 0 (it serializes as `0`).
fn parse_int16(units: &[u16]) -> Option<i32> {
    let mut at = 0;
    while at < units.len() && is_js_space(units[at]) {
        at += 1;
    }
    let mut negative = false;
    if at < units.len() && (units[at] == 0x2b || units[at] == 0x2d) {
        negative = units[at] == 0x2d;
        at += 1;
    }
    if at + 1 < units.len() && units[at] == 0x30 && (units[at + 1] == 0x78 || units[at + 1] == 0x58)
    {
        at += 2;
    }
    let mut value = 0i32;
    let mut digits = 0;
    while at < units.len() {
        let Some(digit) = hex_digit(units[at]) else {
            break;
        };
        value = value * 16 + digit;
        digits += 1;
        at += 1;
    }
    if digits == 0 {
        None
    } else if negative {
        Some(-value)
    } else {
        Some(value)
    }
}

#[cfg(test)]
#[path = "../tests/object_scene_style_paint_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_style_paint.rs"]
mod tests;
