//! Controls for the parametric-shape kernel: deliberate mistakes the oracle replay must catch (each only in the
//! ops it can affect), a tolerance pair proving the trigonometric bound is neither vacuous nor deaf, and coverage
//! guards for the oracle branches the controls rely on.

use super::super::boxed::{
    apply_param_shape, apply_param_shape_animated, rotate_segments, sync_on_scale, AnimatedParams,
    AppliedShape, ParamBox, ParamShape, ParamShapeKind, PathBounds, ScaleSync,
};
use super::super::{
    build_arc_ellipse, build_round_rect, build_star_polygon, js_max, js_min, shape_geom,
    GeomSource, ParamGeomError, ParamSegment, ShapeGeom, ShapeTool, StarState, KAPPA,
};
use super::{geom, run, Check};

/// A mistake injected around the real kernel (inputs before the call, outputs after it).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// Every handle scaled by 1 + 1e-8: a kappa of 0.55228475 instead of 0.5522847498.
    Kappa,
    /// Corner radii not clamped (negative radii and radii above half the side survive).
    ClampNone,
    /// Corner radii clamped to the whole shorter side instead of half of it.
    ClampFullSide,
    /// A zero radius emits two coincident anchors instead of one.
    ZeroAsRounded,
    /// The donut inner ring dropped: every arc closes through the centre.
    DonutAsPie,
    /// The first star vertex at angle 0 instead of -PI/2 (a quarter turn).
    StarStartAngle,
    /// An inner ratio of 0.999 no longer makes a regular polygon.
    PolygonAbove1,
    /// A corner radius of exactly 0.01 rounds the corners.
    CornerAtEps,
    /// `boxAngle` ignored by the rebuilds.
    NoAngle,
    /// The ellipse rebuild re-applies `boxAngle` (the baseline does not).
    EllipseRotates,
    /// An animated percent read as a 0..1 ratio.
    PercentAsRatio,
    /// A scaled box not re-ordered (min/max) for a negative factor.
    BoxNoReorder,
    /// Rect from the other source (drag order against normalised).
    RectSourceSwap,
}

const BAD_KAPPA: f64 = 1.0 + 1e-8;

fn scale2(h: [f64; 2]) -> [f64; 2] {
    [h[0] * BAD_KAPPA, h[1] * BAD_KAPPA]
}

fn quarter_turn(segs: &mut [ParamSegment], cx: f64, cy: f64) {
    for s in segs.iter_mut() {
        s.point = [cx - (s.point[1] - cy), cy + (s.point[0] - cx)];
        s.handle_in = [-s.handle_in[1], s.handle_in[0]];
        s.handle_out = [-s.handle_out[1], s.handle_out[0]];
    }
}

/// An independent rounded-rectangle builder with one deliberately wrong rule.
fn mutant_round_rect(
    mode: Mutation,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    radii: [f64; 4],
) -> Vec<ParamSegment> {
    let side = js_min(x2 - x1, y2 - y1);
    let max_r = if mode == Mutation::ClampFullSide {
        side
    } else {
        side / 2.0
    };
    let clamp = |r: f64| {
        if mode == Mutation::ClampNone {
            r
        } else {
            js_max(0.0, js_min(r, max_r))
        }
    };
    let [tl, tr, br, bl] = radii.map(clamp);
    let rounded = |r: f64| {
        if mode == Mutation::ZeroAsRounded {
            r >= 0.0
        } else {
            r > 0.0
        }
    };
    let k = KAPPA;
    let s = |point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]| ParamSegment {
        point,
        handle_in,
        handle_out,
    };
    let c = |point: [f64; 2]| s(point, [0.0, 0.0], [0.0, 0.0]);
    let mut out = Vec::new();
    if rounded(tl) {
        out.push(s([x1 + tl, y1], [-tl * k, 0.0], [0.0, 0.0]));
    } else {
        out.push(c([x1, y1]));
    }
    if rounded(tr) {
        out.push(s([x2 - tr, y1], [0.0, 0.0], [tr * k, 0.0]));
        out.push(s([x2, y1 + tr], [0.0, -tr * k], [0.0, 0.0]));
    } else {
        out.push(c([x2, y1]));
    }
    if rounded(br) {
        out.push(s([x2, y2 - br], [0.0, 0.0], [0.0, br * k]));
        out.push(s([x2 - br, y2], [br * k, 0.0], [0.0, 0.0]));
    } else {
        out.push(c([x2, y2]));
    }
    if rounded(bl) {
        out.push(s([x1 + bl, y2], [0.0, 0.0], [-bl * k, 0.0]));
        out.push(s([x1, y2 - bl], [0.0, bl * k], [0.0, 0.0]));
    } else {
        out.push(c([x1, y2]));
    }
    if rounded(tl) {
        out.push(s([x1, y1 + tl], [0.0, 0.0], [0.0, -tl * k]));
    }
    out
}

impl Mutation {
    fn handles(self, segs: &mut [ParamSegment]) {
        if self == Mutation::Kappa {
            for s in segs.iter_mut() {
                s.handle_in = scale2(s.handle_in);
                s.handle_out = scale2(s.handle_out);
            }
        }
    }

    pub(crate) fn round_rect(
        self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        radii: [f64; 4],
    ) -> Result<Vec<ParamSegment>, ParamGeomError> {
        let mut segs = match self {
            Mutation::ClampNone | Mutation::ClampFullSide | Mutation::ZeroAsRounded => {
                mutant_round_rect(self, x1, y1, x2, y2, radii)
            }
            _ => build_round_rect(x1, y1, x2, y2, radii)?,
        };
        self.handles(&mut segs);
        Ok(segs)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn arc(
        self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start: f64,
        sweep: f64,
        inner: Option<f64>,
    ) -> Result<Vec<ParamSegment>, ParamGeomError> {
        let inner = if self == Mutation::DonutAsPie {
            None
        } else {
            inner
        };
        build_arc_ellipse(cx, cy, rx, ry, start, sweep, inner)
    }

    pub(crate) fn star(
        self,
        cx: f64,
        cy: f64,
        outer_r: f64,
        point_count: Option<f64>,
        inner: Option<f64>,
        corner: Option<f64>,
    ) -> Result<Vec<ParamSegment>, ParamGeomError> {
        let inner = match (self, inner) {
            (Mutation::PolygonAbove1, Some(v)) if (0.999..1.0).contains(&v) => Some(0.9989),
            _ => inner,
        };
        let corner = match (self, corner) {
            (Mutation::CornerAtEps, Some(v)) if v == 0.01 => Some(0.011),
            _ => corner,
        };
        let mut segs = build_star_polygon(cx, cy, outer_r, point_count, inner, corner)?;
        if self == Mutation::StarStartAngle {
            quarter_turn(&mut segs, cx, cy);
        }
        self.handles(&mut segs);
        Ok(segs)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shape_geom(
        self,
        tool: ShapeTool,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        star: StarState,
        source: GeomSource,
    ) -> Result<ShapeGeom, ParamGeomError> {
        let source = match (self, tool, source) {
            (Mutation::RectSourceSwap, ShapeTool::Rect, GeomSource::Wasm) => GeomSource::JsFallback,
            (Mutation::RectSourceSwap, ShapeTool::Rect, GeomSource::JsFallback) => GeomSource::Wasm,
            _ => source,
        };
        let mut g = shape_geom(tool, x0, y0, x1, y1, star, source)?;
        if self == Mutation::Kappa {
            for s in g.segments.iter_mut() {
                s.handle_in = s.handle_in.map(scale2);
                s.handle_out = s.handle_out.map(scale2);
            }
        }
        Ok(g)
    }

    pub(crate) fn apply(
        self,
        shape: &ParamShape,
        stored: Option<ParamBox>,
        bounds: Option<PathBounds>,
        angle: f64,
    ) -> Result<AppliedShape, ParamGeomError> {
        let used = if self == Mutation::NoAngle {
            0.0
        } else {
            angle
        };
        let mut a = apply_param_shape(shape, stored, bounds, used)?;
        if self == Mutation::EllipseRotates
            && shape.kind() == ParamShapeKind::Ellipse
            && angle != 0.0
        {
            let b = a.box_;
            let centre = [(b.x1 + b.x2) / 2.0, (b.y1 + b.y2) / 2.0];
            a.segments = rotate_segments(&a.segments, angle, centre)?;
        }
        Ok(a)
    }

    pub(crate) fn anim(
        self,
        shape: &ParamShape,
        stored: &[ParamSegment],
        params: &AnimatedParams,
    ) -> Result<Vec<ParamSegment>, ParamGeomError> {
        let mut a = *params;
        if self == Mutation::PercentAsRatio {
            a.arc_inner = a.arc_inner.map(|v| v / 100.0);
            a.star_inner = a.star_inner.map(|v| v / 100.0);
        }
        apply_param_shape_animated(shape, stored, &a)
    }

    pub(crate) fn sync_on_scale(
        self,
        shape: &ParamShape,
        stored: Option<ParamBox>,
        angle: f64,
        sx: f64,
        sy: f64,
        anchor: [f64; 2],
    ) -> Result<Option<ScaleSync>, ParamGeomError> {
        let used = if self == Mutation::NoAngle {
            0.0
        } else {
            angle
        };
        let mut s = sync_on_scale(shape, stored, used, sx, sy, anchor)?;
        if self == Mutation::BoxNoReorder {
            if let (Some(r), Some(b)) = (s.as_mut(), stored) {
                r.box_ = ParamBox {
                    x1: anchor[0] + (b.x1 - anchor[0]) * sx,
                    y1: anchor[1] + (b.y1 - anchor[1]) * sy,
                    x2: anchor[0] + (b.x2 - anchor[0]) * sx,
                    y2: anchor[1] + (b.y2 - anchor[1]) * sy,
                };
            }
        }
        Ok(s)
    }
}

/// Each mistake and the row labels (first word of a failure) it is allowed to break.
const CAUGHT_IN: [(Mutation, &[&str]); 13] = [
    (Mutation::Kappa, &["rr", "star", "sg"]),
    (Mutation::ClampNone, &["rr"]),
    (Mutation::ClampFullSide, &["rr"]),
    (Mutation::ZeroAsRounded, &["rr"]),
    (Mutation::DonutAsPie, &["arc"]),
    (Mutation::StarStartAngle, &["star"]),
    (Mutation::PolygonAbove1, &["star"]),
    (Mutation::CornerAtEps, &["star"]),
    (Mutation::NoAngle, &["apply", "scale"]),
    (Mutation::EllipseRotates, &["apply"]),
    (Mutation::PercentAsRatio, &["anim"]),
    (Mutation::BoxNoReorder, &["scale"]),
    (Mutation::RectSourceSwap, &["sg"]),
];

#[test]
fn every_mistake_is_caught_by_the_oracle_and_only_where_it_acts() {
    assert!(run(Mutation::None).failures.is_empty());
    for (m, ops) in CAUGHT_IN {
        let r = run(m);
        assert!(!r.failures.is_empty(), "{m:?} was not caught");
        for f in &r.failures {
            let op = f.split(' ').next().unwrap_or("");
            assert!(ops.contains(&op), "{m:?} broke an unexpected row: {f}");
        }
    }
}

#[test]
fn the_trig_bound_is_neither_vacuous_nor_deaf() {
    let got =
        geom(&build_star_polygon(100.0, 100.0, 60.0, Some(5.0), Some(0.5), Some(0.0)).unwrap());
    let mut one_ulp = got.clone();
    one_ulp[1].point[0] = f64::from_bits(one_ulp[1].point[0].to_bits() + 1);
    let mut near = Check::new("one ulp, trig");
    near.segments(&got, &one_ulp, true);
    assert!(near.bad.is_none(), "{:?}", near.bad);
    let mut exact = Check::new("one ulp, exact");
    exact.segments(&got, &one_ulp, false);
    assert!(exact.bad.is_some());
    let mut off = got.clone();
    off[1].point[0] += 1e-9;
    let mut far = Check::new("1e-9, trig");
    far.segments(&got, &off, true);
    assert!(far.bad.is_some());
    let mut zero_got = got.clone();
    let mut zero_want = got.clone();
    zero_got[0].point[0] = 0.0;
    zero_want[0].point[0] = -0.0;
    let mut sign = Check::new("signed zero, trig");
    sign.segments(&zero_got, &zero_want, true);
    assert!(sign.bad.is_some(), "a signed-zero mismatch must never pass");
}

#[test]
fn the_oracle_still_covers_the_branches_the_controls_rely_on() {
    let r = run(Mutation::None);
    let tick = |k: &str| r.ticks.get(k).copied().unwrap_or(0);
    assert_eq!(tick("bnd:straight"), 9);
    assert_eq!(tick("bnd:curved"), 4);
    assert_eq!(tick("scale:negative"), 34);
    assert_eq!(tick("scale:positive"), 56);
    assert_eq!(tick("apply:healed"), 57);
    assert_eq!(tick("apply:boxed"), 112);
    assert_eq!(tick("apply:rotated"), 127);
    assert_eq!(tick("rot:tiny-handle"), 60);
}
