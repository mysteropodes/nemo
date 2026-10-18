//! Text animator contribution kernel (D28-K1): `_taStrokeRestBounds`,
//! `taUnitCount`, `taUnitPivotCenter`, `textAnimatorContribution` and the
//! text-animator fold of `elementMotionAt` in `src/js/motion.js` at ca931b5a,
//! as pure data in and out. Inputs are explicit and immutable: the layer's
//! stored strokes (unit indices, rest anchor points), the animators with their
//! selectors ALREADY resolved (`taSelectorAt` reads Motion holders, out of
//! scope), `frameIdx` and `state.fps`. Weights and unit indices come from the
//! native range selector (`text_glyph_selector`), never re-implemented here.
//! No document owner, history, Paper or UI.
//!
//! Reproduced on purpose, in the baseline order of operations:
//! * an animator is skipped when `enabled === false` (only), without props,
//!   without an index for its `basedOn`, on a unit count of 0 and on a weight
//!   of 0; the first one counted creates the accumulator, so empty props still
//!   give an all-zero contribution, not `null`. `time = frameIdx / (fps || 24)`;
//! * position reads `|| 0`, rotation counts only when truthy, scale defaults
//!   `== null ? 100` and sums `(s - 100) * w`, opacity counts when `!= null`
//!   as `(op - 100) * w`, colour alpha defaults to 255. Colour is a weighted
//!   sum over the summed weight with `blend = clamp(sumW, 0, 1)`; a summed
//!   weight of exactly 0 gives NO target (the baseline then leaves raw partial
//!   sums on the object that nothing reads; they are not reported);
//! * the pivot `basedOn` is the FIRST counted animator with a truthy rotation
//!   or a nonzero scale delta; the anchor is the centre of the union of the rest
//!   bounds of every stroke of that unit (JS object keys: `-0` is `0`) minus the
//!   stroke's own centre, set only when both boxes exist. Rest bounds read
//!   anchor points only, skipping missing ones;
//! * the fold: identity base when none, sums for dx/dy/rot, `s * (1 + d / 100)`
//!   for scale, `clamp(op * (1 + d / 100), 0, 1)`, `ax + (ta.ax || 0)`; colour
//!   pulls pass through. No contribution returns the base unchanged.
//!
//! Trigonometry only enters through the selector (Wiggly, SMOOTH), so its
//! oracle bound policy applies: those rows are held within a bound, every other
//! row is bit for bit. Native-only refusals, all before any output: a
//! non-finite frame, fps, time, index, rest point, prop, base or selector
//! field; more than [`MAX_ANIMATOR_STROKES`] strokes or [`MAX_TEXT_ANIMATORS`]
//! animators; a unit count above `MAX_SELECTOR_UNITS` for a `basedOn` in use;
//! a non-finite weight or result.

use crate::object_document::object_text_layout_kernels::text_glyph_selector::js::{
    js_max, js_min, js_or,
};
use crate::object_document::object_text_layout_kernels::text_glyph_selector::model::check_selector;
use crate::object_document::object_text_layout_kernels::text_glyph_selector::{
    unit_count, unit_index_of, weight_at, BasedOn, Selector, SelectorError, UnitIndices,
    MAX_SELECTOR_UNITS,
};
use std::collections::HashMap;

/// A unit can span several strokes (`:` is two contours), hence 4 per unit.
pub(crate) const MAX_ANIMATOR_STROKES: usize = 4 * MAX_SELECTOR_UNITS;
/// Bound on the per-stroke animator loop (the baseline list is unbounded).
pub(crate) const MAX_TEXT_ANIMATORS: usize = 256;

/// One stored stroke: its unit indices and its rest `segments[i].point`
/// anchors (`None` is a missing segment or point; empty is no segments).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AnimatorStroke {
    pub units: UnitIndices,
    pub rest_points: Vec<Option<[f64; 2]>>,
}

/// `[r, g, b, a]` 0-255; `alpha: None` is JS `null` / `undefined` (255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnimatorColor {
    pub rgb: [f64; 3],
    pub alpha: Option<f64>,
}

/// `an.props`; every `None` is an absent JS field, a `None` component of
/// `position` / `scale` an absent array entry.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AnimatorProps {
    pub position: Option<[Option<f64>; 2]>,
    pub rotation: Option<f64>,
    pub scale: Option<[Option<f64>; 2]>,
    pub opacity: Option<f64>,
    pub fill_color: Option<AnimatorColor>,
    pub stroke_color: Option<AnimatorColor>,
}

/// One animator; `enabled` is false only for a stored `enabled === false`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextAnimator {
    pub enabled: bool,
    pub props: Option<AnimatorProps>,
    pub selector: Selector,
}

/// A colour pull: lerp(base, target, blend) at the render site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColorPull {
    pub target: [f64; 4],
    pub blend: f64,
}

impl ColorPull {
    /// `[t0, t1, t2, t3, blend]`.
    pub(crate) fn values(&self) -> [f64; 5] {
        let t = self.target;
        [t[0], t[1], t[2], t[3], self.blend]
    }
}

/// `textAnimatorContribution`'s object: `sx`, `sy`, `op` are percent deltas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Contribution {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
    pub op: f64,
    pub anchor: Option<[f64; 2]>,
    pub fill: Option<ColorPull>,
    pub stroke: Option<ColorPull>,
}

/// The `computeMotionMat` shape the fold reads (scale and opacity as factors).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ElementMotion {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
    pub op: f64,
    pub ax: f64,
    pub ay: f64,
}

impl ElementMotion {
    pub(crate) const IDENTITY: ElementMotion =
        Self::from_values([0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0]);

    /// From `[dx, dy, rot, sx, sy, op, ax, ay]`.
    pub(crate) const fn from_values(v: [f64; 8]) -> Self {
        let [dx, dy, rot, sx, sy, op, ax, ay] = v;
        ElementMotion {
            dx,
            dy,
            rot,
            sx,
            sy,
            op,
            ax,
            ay,
        }
    }

    pub(crate) fn values(&self) -> [f64; 8] {
        [
            self.dx, self.dy, self.rot, self.sx, self.sy, self.op, self.ax, self.ay,
        ]
    }
}

/// `elementMotionAt`'s answer: the motion plus the colour pulls passed through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FoldedMotion {
    pub motion: ElementMotion,
    pub fill: Option<ColorPull>,
    pub stroke: Option<ColorPull>,
}

/// One layer at one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextAnimatorInput<'a> {
    pub strokes: &'a [AnimatorStroke],
    pub animators: &'a [TextAnimator],
    pub frame_idx: f64,
    pub fps: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAnimatorError {
    Selector(SelectorError),
    NonFinite(&'static str),
    TooManyStrokes,
    TooManyAnimators,
    TooManyUnits,
    NonFiniteResult,
    /// `bases` does not hold exactly one entry per stroke.
    BaseCountMismatch,
}

impl From<SelectorError> for TextAnimatorError {
    fn from(e: SelectorError) -> Self {
        TextAnimatorError::Selector(e)
    }
}

fn finite(name: &'static str, values: &[f64]) -> Result<(), TextAnimatorError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(TextAnimatorError::NonFinite(name))
    }
}

/// JS `if (v < cur) cur = v` and `if (v > cur) cur = v` over `[x0, y0, x1, y1]`.
fn widen(cur: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let lo = |v: f64, c: f64| if v < c { v } else { c };
    let hi = |v: f64, c: f64| if v > c { v } else { c };
    [
        lo(b[0], cur[0]),
        lo(b[1], cur[1]),
        hi(b[2], cur[2]),
        hi(b[3], cur[3]),
    ]
}

/// `_taStrokeRestBounds`: `[x0, y0, x1, y1]` of the anchor points, `None` without one.
pub(crate) fn stroke_rest_bounds(points: &[Option<[f64; 2]>]) -> Option<[f64; 4]> {
    let (inf, neg) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut b = [inf, inf, neg, neg];
    for p in points.iter().flatten() {
        b = widen(b, [p[0], p[1], p[0], p[1]]);
    }
    (b[0] <= b[2]).then_some(b)
}

/// The map of `taUnitPivotCenter`: union of rest bounds per unit index (keyed as
/// JS object keys are, so `-0` and `0` are one unit), in stroke order.
fn pivot_boxes(strokes: &[AnimatorStroke], based_on: BasedOn) -> HashMap<u64, [f64; 4]> {
    let mut map: HashMap<u64, [f64; 4]> = HashMap::new();
    for s in strokes {
        let u = unit_index_of(Some(&s.units), based_on);
        if let (Some(u), Some(b)) = (u, stroke_rest_bounds(&s.rest_points)) {
            let m = map.entry((u + 0.0).to_bits()).or_insert(b);
            *m = widen(*m, b);
        }
    }
    map
}

/// `taUnitPivotCenter(li, f, basedOn, idx)` over the supplied strokes: `[cx, cy]`.
pub(crate) fn unit_pivot_center(
    strokes: &[AnimatorStroke],
    based_on: BasedOn,
    idx: f64,
) -> Option<[f64; 2]> {
    pivot_center(&pivot_boxes(strokes, based_on), idx)
}

fn pivot_center(map: &HashMap<u64, [f64; 4]>, idx: f64) -> Option<[f64; 2]> {
    map.get(&(idx + 0.0).to_bits())
        .map(|e| [(e[0] + e[2]) / 2.0, (e[1] + e[3]) / 2.0])
}

fn check_input(input: &TextAnimatorInput) -> Result<f64, TextAnimatorError> {
    finite("frameIdx", &[input.frame_idx])?;
    finite("fps", &[input.fps])?;
    if input.strokes.len() > MAX_ANIMATOR_STROKES {
        return Err(TextAnimatorError::TooManyStrokes);
    }
    if input.animators.len() > MAX_TEXT_ANIMATORS {
        return Err(TextAnimatorError::TooManyAnimators);
    }
    for s in input.strokes {
        let u = [s.units.char_index, s.units.word_index, s.units.line_index];
        finite("unitIndex", &u.map(|v| v.unwrap_or(0.0)))?;
        let points: Vec<f64> = s.rest_points.iter().flatten().flatten().copied().collect();
        finite("restPoint", &points)?;
    }
    for an in input.animators {
        check_selector(&an.selector)?;
        if let Some(p) = &an.props {
            check_props(p)?;
        }
    }
    let time = input.frame_idx / js_or(input.fps, 24.0);
    finite("timeSec", &[time])?;
    Ok(time)
}

fn check_props(p: &AnimatorProps) -> Result<(), TextAnimatorError> {
    let pair = |v: Option<[Option<f64>; 2]>| v.map_or([0.0; 2], |a| a.map(|c| c.unwrap_or(0.0)));
    let color = |c: Option<AnimatorColor>| {
        c.map_or([0.0; 4], |c| {
            [c.rgb[0], c.rgb[1], c.rgb[2], c.alpha.unwrap_or(0.0)]
        })
    };
    finite("position", &pair(p.position))?;
    finite("rotation", &[p.rotation.unwrap_or(0.0)])?;
    finite("scale", &pair(p.scale))?;
    finite("opacity", &[p.opacity.unwrap_or(0.0)])?;
    finite("fillColor", &color(p.fill_color))?;
    finite("strokeColor", &color(p.stroke_color))
}

fn based_on_slot(b: BasedOn) -> usize {
    match b {
        BasedOn::Chars => 0,
        BasedOn::Words => 1,
        BasedOn::Lines => 2,
    }
}

/// Per-layer memo, the native form of `_taCountCache` / `_taPivotCache`.
struct Layer<'a> {
    strokes: &'a [AnimatorStroke],
    counts: [f64; 3],
    pivots: [Option<HashMap<u64, [f64; 4]>>; 3],
}

impl<'a> Layer<'a> {
    fn new(strokes: &'a [AnimatorStroke]) -> Self {
        let u: Vec<UnitIndices> = strokes.iter().map(|s| s.units).collect();
        let counts = [BasedOn::Chars, BasedOn::Words, BasedOn::Lines].map(|b| unit_count(&u, b));
        let pivots = [None, None, None];
        Layer {
            strokes,
            counts,
            pivots,
        }
    }

    fn pivot(&mut self, based_on: BasedOn, idx: f64) -> Option<[f64; 2]> {
        let strokes = self.strokes;
        let map = self.pivots[based_on_slot(based_on)]
            .get_or_insert_with(|| pivot_boxes(strokes, based_on));
        pivot_center(map, idx)
    }
}

#[derive(Default)]
struct Acc {
    c: Contribution,
    fill: Option<[f64; 5]>,
    stroke: Option<[f64; 5]>,
}

fn add_color(sum: &mut Option<[f64; 5]>, c: &AnimatorColor, w: f64) {
    let s = sum.get_or_insert([0.0; 5]);
    s[0] += c.rgb[0] * w;
    s[1] += c.rgb[1] * w;
    s[2] += c.rgb[2] * w;
    s[3] += c.alpha.unwrap_or(255.0) * w;
    s[4] += w;
}

/// One counted animator; answers whether it needs a pivot (rotation or scale).
fn add_props(acc: &mut Acc, p: &AnimatorProps, w: f64) -> bool {
    let c = &mut acc.c;
    let mut pivots = false;
    if let Some(pos) = p.position {
        c.dx += js_or(pos[0].unwrap_or(0.0), 0.0) * w;
        c.dy += js_or(pos[1].unwrap_or(0.0), 0.0) * w;
    }
    if let Some(r) = p.rotation.filter(|r| *r != 0.0) {
        c.rot += r * w;
        pivots = true;
    }
    if let Some(s) = p.scale {
        let dsx = s[0].unwrap_or(100.0) - 100.0;
        let dsy = s[1].unwrap_or(100.0) - 100.0;
        c.sx += dsx * w;
        c.sy += dsy * w;
        pivots |= dsx != 0.0 || dsy != 0.0;
    }
    if let Some(op) = p.opacity {
        c.op += (op - 100.0) * w;
    }
    if let Some(fc) = &p.fill_color {
        add_color(&mut acc.fill, fc, w);
    }
    if let Some(sc) = &p.stroke_color {
        add_color(&mut acc.stroke, sc, w);
    }
    pivots
}

fn color_pull(sum: Option<[f64; 5]>) -> Option<ColorPull> {
    let s = sum.filter(|s| s[4] != 0.0)?;
    Some(ColorPull {
        target: [s[0] / s[4], s[1] / s[4], s[2] / s[4], s[3] / s[4]],
        blend: js_max(0.0, js_min(1.0, s[4])),
    })
}

fn contribution_of(
    layer: &mut Layer,
    animators: &[TextAnimator],
    time: f64,
    sd: &AnimatorStroke,
) -> Result<Option<Contribution>, TextAnimatorError> {
    let mut acc: Option<Acc> = None;
    let mut pivot_based_on: Option<BasedOn> = None;
    for an in animators {
        let based_on = an.selector.based_on;
        let idx = unit_index_of(Some(&sd.units), based_on);
        let (Some(p), Some(idx)) = (an.props.as_ref().filter(|_| an.enabled), idx) else {
            continue;
        };
        let total = layer.counts[based_on_slot(based_on)];
        let w = if total == 0.0 {
            0.0
        } else {
            weight_at(&an.selector, idx, total, time)?
        };
        if w == 0.0 {
            continue;
        }
        if add_props(acc.get_or_insert_with(Acc::default), p, w) {
            pivot_based_on = pivot_based_on.or(Some(based_on));
        }
    }
    let Some(mut acc) = acc else { return Ok(None) };
    if let Some(b) = pivot_based_on {
        if let Some(idx) = unit_index_of(Some(&sd.units), b) {
            let own = stroke_rest_bounds(&sd.rest_points);
            if let (Some(own), Some(shared)) = (own, layer.pivot(b, idx)) {
                acc.c.anchor = Some([
                    shared[0] - (own[0] + own[2]) / 2.0,
                    shared[1] - (own[1] + own[3]) / 2.0,
                ]);
            }
        }
    }
    let sums = [acc.fill, acc.stroke].map(|s| s.unwrap_or([0.0; 5]));
    acc.c.fill = color_pull(acc.fill);
    acc.c.stroke = color_pull(acc.stroke);
    finite_result(&acc.c, sums.as_flattened())?;
    Ok(Some(acc.c))
}

fn finite_result(c: &Contribution, extra: &[f64]) -> Result<(), TextAnimatorError> {
    let a = c.anchor.unwrap_or([0.0; 2]);
    let pulls = [c.fill, c.stroke].map(|p| p.map_or([0.0; 5], |p| p.values()));
    let fixed = [c.dx, c.dy, c.rot, c.sx, c.sy, c.op, a[0], a[1]];
    match finite("", &[&fixed[..], pulls.as_flattened(), extra].concat()) {
        Ok(()) => Ok(()),
        Err(_) => Err(TextAnimatorError::NonFiniteResult),
    }
}

/// `textAnimatorContribution` for every supplied stroke (each one as `sd`).
pub(crate) fn layer_contributions(
    input: &TextAnimatorInput,
) -> Result<Vec<Option<Contribution>>, TextAnimatorError> {
    let time = check_input(input)?;
    let mut layer = Layer::new(input.strokes);
    for an in input.animators {
        if layer.counts[based_on_slot(an.selector.based_on)] > MAX_SELECTOR_UNITS as f64 {
            return Err(TextAnimatorError::TooManyUnits);
        }
    }
    input
        .strokes
        .iter()
        .map(|sd| contribution_of(&mut layer, input.animators, time, sd))
        .collect()
}

/// The text-animator fold of `elementMotionAt` for one element.
pub(crate) fn fold_element_motion(
    base: Option<ElementMotion>,
    ta: Option<&Contribution>,
) -> Result<Option<FoldedMotion>, TextAnimatorError> {
    if let Some(b) = &base {
        finite("baseMotion", &b.values())?;
    }
    let Some(ta) = ta else {
        return Ok(base.map(|motion| FoldedMotion {
            motion,
            fill: None,
            stroke: None,
        }));
    };
    finite_result(ta, &[])?;
    let m = base.unwrap_or(ElementMotion::IDENTITY);
    let a = ta.anchor.map_or([0.0; 2], |a| a.map(|v| js_or(v, 0.0)));
    let motion = ElementMotion::from_values([
        m.dx + ta.dx,
        m.dy + ta.dy,
        m.rot + ta.rot,
        m.sx * (1.0 + ta.sx / 100.0),
        m.sy * (1.0 + ta.sy / 100.0),
        js_max(0.0, js_min(1.0, m.op * (1.0 + ta.op / 100.0))),
        m.ax + a[0],
        m.ay + a[1],
    ]);
    finite("foldedMotion", &motion.values()).map_err(|_| TextAnimatorError::NonFiniteResult)?;
    Ok(Some(FoldedMotion {
        motion,
        fill: ta.fill,
        stroke: ta.stroke,
    }))
}

/// `elementMotionAt` for every supplied stroke, `bases[i]` its own element motion.
pub(crate) fn layer_element_motions(
    input: &TextAnimatorInput,
    bases: &[Option<ElementMotion>],
) -> Result<Vec<Option<FoldedMotion>>, TextAnimatorError> {
    if bases.len() != input.strokes.len() {
        return Err(TextAnimatorError::BaseCountMismatch);
    }
    // All or nothing: one refused base refuses the whole layer.
    layer_contributions(input)?
        .iter()
        .zip(bases)
        .map(|(ta, base)| fold_element_motion(*base, ta.as_ref()))
        .collect()
}

#[cfg(test)]
#[path = "../tests/property_text_animator.rs"]
mod tests;
