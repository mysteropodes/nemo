//! Typed scene stroke kernel (L07-B / F16), pure and I/O free.
//!
//! Joins the two baseline stages that decide what a scene stroke is:
//! - the JS bridge (`buildSceneJson`, `src/js/engine-bridge.js` at fbe4b8a3):
//!   the stroke is absent entirely when `cssColorToRgba(strokeColor)` is null;
//!   otherwise `strokeWidth = (c.strokeWidth || 1) * (pathTransform ? 1 : strokeScale)`
//!   (`||` also replaces `0`, `-0` and `NaN`), `strokeCap`/`strokeJoin` pass only
//!   when they are strings, `miterLimit` passes as is, and `dashPattern` /
//!   `dashOffset` are emitted only for a non-empty `dashArray`;
//! - the engine (`geometry-wasm/src/engine.rs` at fbe4b8a3): `stroke_width`
//!   defaults to 1.0 only when absent, `cap_from` maps `butt`/`square` and
//!   everything else to Round, `join_from` maps `bevel`/`miter` and everything
//!   else to Round, a present `miter_limit` is applied and an absent one keeps
//!   kurbo's default (4.0), a dash applies only for a non-empty pattern and a
//!   non-zero offset is baked into the pattern by `dash_pattern_with_offset`
//!   (so the scene stroke always carries dash offset 0).
//!
//! `dash_pattern_with_offset` is ported bit for bit including its observed
//! defect: it rotates the pattern but drops the consumed `remaining` part of
//! the start segment, so the baked array reproduces the original only for the
//! first baked cycle (`x < sum(baked)`); the tests pin that boundary, they do
//! not hide it. The gap case prepends a zero-length dash, which makes the baked
//! array odd-length for even-length patterns.
//!
//! Interpretations (the baseline serialises through JSON and Rust):
//! - a non-finite width, or a non-finite dash entry, is JSON `null` for a
//!   required `f64`, which fails serde and takes the whole baseline render down;
//!   here it is a typed `StrokeError`. A negative dash entry, or a dash period
//!   whose sum overflows to infinity, is also a typed error (the baseline's
//!   bake spins forever on an infinite period with a negative offset);
//!   `bake_dash_offset` called directly with a non-finite offset is a typed
//!   error too (`rem_euclid` would hand back NaN); `engine_stroke` never gets
//!   there because a non-finite offset is absent first;
//! - a non-finite `miter_limit` / `dash_offset` is JSON `null` for an
//!   `Option<f64>`, which serde reads as absent; it is treated as absent.
//!
//! `StrokeRules` carries the baseline constants so the controls can mutate each
//! one and prove a named test notices. `BASELINE` is the only value callers use.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SceneCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SceneJoin {
    Bevel,
    Miter,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StrokeError {
    NonFiniteWidth,
    NonFiniteDashEntry,
    NegativeDashEntry,
    NonFiniteDashPeriod,
    NonFiniteDashOffset,
}

impl StrokeError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NonFiniteWidth => "stroke width must be finite",
            Self::NonFiniteDashEntry => "dash pattern entries must be finite",
            Self::NegativeDashEntry => "dash pattern entries must not be negative",
            Self::NonFiniteDashPeriod => "dash pattern period must be finite",
            Self::NonFiniteDashOffset => "dash offset must be finite to be baked",
        }
    }
}

/// kurbo `Stroke::new` defaults used when a style field is absent.
pub(crate) const KURBO_DEFAULT_CAP: SceneCap = SceneCap::Round;
pub(crate) const KURBO_DEFAULT_JOIN: SceneJoin = SceneJoin::Round;
pub(crate) const KURBO_DEFAULT_MITER_LIMIT: f64 = 4.0;

/// The stroke-relevant fields of one path item as `buildSceneJson` sees them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StrokeSource {
    /// Resolved `cssColorToRgba(strokeColor, opacity)`; `None` = null = no stroke.
    pub(crate) color: Option<[u8; 4]>,
    /// `c.strokeWidth` (`None` = undefined/null).
    pub(crate) width: Option<f64>,
    /// `item.pathTransform` is set (the engine strokes through the affine).
    pub(crate) path_transform: bool,
    pub(crate) stroke_scale: f64,
    /// `c.strokeCap` / `c.strokeJoin` already filtered by `typeof === 'string'`.
    pub(crate) cap: Option<String>,
    pub(crate) join: Option<String>,
    pub(crate) miter_limit: Option<f64>,
    pub(crate) dash_array: Option<Vec<f64>>,
    pub(crate) dash_offset: Option<f64>,
}

/// The stroke fields of the engine's `ItemIn` (engine.rs `stroke_from` input).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EngineStrokeItem {
    pub(crate) width: Option<f64>,
    pub(crate) cap: Option<String>,
    pub(crate) join: Option<String>,
    pub(crate) miter_limit: Option<f64>,
    pub(crate) dash_pattern: Option<Vec<f64>>,
    pub(crate) dash_offset: Option<f64>,
}

/// Resolved stroke style. `miter_limit == None` means kurbo's default; `dash`
/// is the already-baked pattern and the dash offset is always 0.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SceneStroke {
    pub(crate) width: f64,
    pub(crate) cap: SceneCap,
    pub(crate) join: SceneJoin,
    pub(crate) miter_limit: Option<f64>,
    pub(crate) dash: Option<Vec<f64>>,
}

impl SceneStroke {
    pub(crate) fn effective_miter_limit(&self) -> f64 {
        self.miter_limit.unwrap_or(KURBO_DEFAULT_MITER_LIMIT)
    }

    /// Baked patterns are always drawn from offset 0.
    pub(crate) fn dash_offset(&self) -> f64 {
        0.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PaintedStroke {
    pub(crate) color: [u8; 4],
    pub(crate) stroke: SceneStroke,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StrokeRules {
    /// engine.rs `default_stroke_width`: used only when `stroke_width` is absent.
    pub(crate) absent_width: f64,
    /// The `1` of `c.strokeWidth || 1`.
    pub(crate) js_fallback_width: f64,
    /// `||` treats `0`, `-0` and `NaN` as missing.
    pub(crate) zero_is_falsy: bool,
    /// `(pathTransform ? 1 : strokeScale)`.
    pub(crate) path_transform_skips_scale: bool,
    pub(crate) fallback_cap: SceneCap,
    pub(crate) fallback_join: SceneJoin,
    /// `if !pattern.is_empty()` in `stroke_from`.
    pub(crate) empty_dash_skips: bool,
    /// `offset.rem_euclid(total)` instead of the truncating `%`.
    pub(crate) euclid_wrap: bool,
    /// Offset inside a gap prepends a zero-length dash.
    pub(crate) gap_zero_dash: bool,
}

pub(crate) const BASELINE: StrokeRules = StrokeRules {
    absent_width: 1.0,
    js_fallback_width: 1.0,
    zero_is_falsy: true,
    path_transform_skips_scale: true,
    fallback_cap: SceneCap::Round,
    fallback_join: SceneJoin::Round,
    empty_dash_skips: true,
    euclid_wrap: true,
    gap_zero_dash: true,
};

/// JSON `null` for an `Option<f64>`: a non-finite number reads as absent.
fn finite_or_absent(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

fn pattern_total(pattern: &[f64]) -> f64 {
    pattern.iter().fold(0.0, |sum, entry| sum + entry)
}

fn validate_dash(pattern: &[f64]) -> Result<(), StrokeError> {
    for entry in pattern {
        if !entry.is_finite() {
            return Err(StrokeError::NonFiniteDashEntry);
        }
        if *entry < 0.0 {
            return Err(StrokeError::NegativeDashEntry);
        }
    }
    if !pattern_total(pattern).is_finite() {
        return Err(StrokeError::NonFiniteDashPeriod);
    }
    Ok(())
}

impl StrokeRules {
    pub(crate) fn cap_from(&self, name: &str) -> SceneCap {
        match name {
            "butt" => SceneCap::Butt,
            "square" => SceneCap::Square,
            _ => self.fallback_cap,
        }
    }

    pub(crate) fn join_from(&self, name: &str) -> SceneJoin {
        match name {
            "bevel" => SceneJoin::Bevel,
            "miter" => SceneJoin::Miter,
            _ => self.fallback_join,
        }
    }

    /// engine.rs `ItemIn::stroke_width` serde default: only an absent width
    /// defaults; an explicit 0 stays 0.
    pub(crate) fn engine_width(&self, width: Option<f64>) -> f64 {
        width.unwrap_or(self.absent_width)
    }

    /// engine-bridge.js: `(c.strokeWidth || 1) * (pathTransform ? 1 : strokeScale)`.
    pub(crate) fn scene_width(
        &self,
        width: Option<f64>,
        path_transform: bool,
        stroke_scale: f64,
    ) -> f64 {
        let base = match width {
            None => self.js_fallback_width,
            Some(w) if self.zero_is_falsy && (w == 0.0 || w.is_nan()) => self.js_fallback_width,
            Some(w) => w,
        };
        if path_transform && self.path_transform_skips_scale {
            base
        } else {
            base * stroke_scale
        }
    }

    /// engine.rs `dash_pattern_with_offset`, bit for bit (see the module docs
    /// for the observed defect). Entries are validated first so the `while`
    /// below terminates: finite, non-negative, finite total.
    pub(crate) fn bake_dash_offset(
        &self,
        pattern: &[f64],
        offset: f64,
    ) -> Result<Vec<f64>, StrokeError> {
        validate_dash(pattern)?;
        if !offset.is_finite() {
            return Err(StrokeError::NonFiniteDashOffset);
        }
        let total = pattern_total(pattern);
        if total <= 0.0 || pattern.is_empty() {
            return Ok(pattern.to_vec());
        }
        let mut remaining = if self.euclid_wrap {
            offset.rem_euclid(total)
        } else {
            offset % total
        };
        let n = pattern.len();
        let mut start = 0usize;
        while remaining >= pattern[start] {
            remaining -= pattern[start];
            start = (start + 1) % n;
        }
        let first_seg_left = pattern[start] - remaining;
        let mut result = Vec::with_capacity(n + 1);
        if start % 2 == 0 {
            result.push(first_seg_left);
        } else {
            if self.gap_zero_dash {
                result.push(0.0);
            }
            result.push(first_seg_left);
        }
        let mut i = (start + 1) % n;
        while i != start {
            result.push(pattern[i]);
            i = (i + 1) % n;
        }
        Ok(result)
    }

    /// engine.rs `stroke_from` as a typed record.
    pub(crate) fn engine_stroke(
        &self,
        item: &EngineStrokeItem,
    ) -> Result<SceneStroke, StrokeError> {
        let width = self.engine_width(item.width);
        if !width.is_finite() {
            return Err(StrokeError::NonFiniteWidth);
        }
        let cap = item
            .cap
            .as_deref()
            .map_or(KURBO_DEFAULT_CAP, |name| self.cap_from(name));
        let join = item
            .join
            .as_deref()
            .map_or(KURBO_DEFAULT_JOIN, |name| self.join_from(name));
        let dash = match &item.dash_pattern {
            Some(pattern) if !pattern.is_empty() || !self.empty_dash_skips => {
                validate_dash(pattern)?;
                let offset = finite_or_absent(item.dash_offset).unwrap_or(0.0);
                if offset != 0.0 {
                    Some(self.bake_dash_offset(pattern, offset)?)
                } else {
                    Some(pattern.clone())
                }
            }
            _ => None,
        };
        Ok(SceneStroke {
            width,
            cap,
            join,
            miter_limit: finite_or_absent(item.miter_limit),
            dash,
        })
    }

    /// The whole baseline path for one item: `None` when the stroke is absent.
    pub(crate) fn scene_stroke(
        &self,
        source: &StrokeSource,
    ) -> Result<Option<PaintedStroke>, StrokeError> {
        let Some(color) = source.color else {
            return Ok(None);
        };
        let width = self.scene_width(source.width, source.path_transform, source.stroke_scale);
        let dash_pattern = source.dash_array.clone().filter(|array| !array.is_empty());
        let dash_offset = if dash_pattern.is_some() {
            source.dash_offset
        } else {
            None
        };
        let item = EngineStrokeItem {
            width: Some(width),
            cap: source.cap.clone(),
            join: source.join.clone(),
            miter_limit: source.miter_limit,
            dash_pattern,
            dash_offset,
        };
        let stroke = self.engine_stroke(&item)?;
        Ok(Some(PaintedStroke { color, stroke }))
    }
}

pub(crate) fn scene_stroke(source: &StrokeSource) -> Result<Option<PaintedStroke>, StrokeError> {
    BASELINE.scene_stroke(source)
}

#[cfg(test)]
#[path = "../tests/object_scene_style_stroke_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_style_stroke.rs"]
mod tests;
