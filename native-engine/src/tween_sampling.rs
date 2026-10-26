//! #2358: tween sampling; adapted from geometry-wasm at 46442ea9.
use super::*;

// ---- shared stroke shape (mirrors the app's frame.strokes[i] records) ----
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SegIn {
    pub point: [f64; 2],
    #[serde(default)]
    pub handle_in: [f64; 2],
    #[serde(default)]
    pub handle_out: [f64; 2],
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CenterSegIn {
    pub point: [f64; 2],
    #[serde(default)]
    pub handle_in: [f64; 2],
    #[serde(default)]
    pub handle_out: [f64; 2],
    #[serde(default = "default_width")]
    pub width: f64,
}
pub(super) fn default_width() -> f64 {
    3.0
}

#[derive(Deserialize, Clone)]
pub struct WidthSample {
    pub t: f64,
    pub width: f64,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StrokeIn {
    #[serde(default)]
    pub segments: Vec<SegIn>,
    pub center_segments: Option<Vec<CenterSegIn>>,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
    pub width_profile: Option<Vec<WidthSample>>,
    pub has_real_stroke: Option<bool>,
    #[serde(default)]
    pub is_vector_brush: bool,
    #[serde(default)]
    pub closed: bool,
}

pub(super) fn is_vb(s: &StrokeIn) -> bool {
    s.is_vector_brush && s.center_segments.as_ref().map_or(false, |c| c.len() > 1)
}

// ---- arc-length sampling over a whole (possibly multi-segment) BezPath ----
// The active JS generation path uses Paper sampling. Reuse its already-ported
// D30 sampler: geometry-wasm's 0.1 inverse-arclength tolerance moves even a
// straight-line sample by 0.01848 px (independent baseline fixture), and finite
// differences also change tangents. Do not loosen the oracle to hide that drift.
pub(super) struct PathSampler {
    curves: Vec<[f64; 8]>,
    closed: bool,
    pub(super) total: f64,
}
impl PathSampler {
    pub(super) fn new(path: &BezPath) -> Self {
        use crate::evaluation::property_follow_path_sample::path_length;
        let curves: Vec<_> = path
            .segments()
            .map(|seg| {
                let c = match seg {
                    PathSeg::Line(l) => [l.p0, l.p0, l.p1, l.p1],
                    PathSeg::Quad(q) => {
                        let c = q.raise();
                        [c.p0, c.p1, c.p2, c.p3]
                    }
                    PathSeg::Cubic(c) => [c.p0, c.p1, c.p2, c.p3],
                };
                [
                    c[0].x, c[0].y, c[1].x, c[1].y, c[2].x, c[2].y, c[3].x, c[3].y,
                ]
            })
            .collect();
        let total = path_length(&curves);
        let closed = matches!(path.elements().last(), Some(kurbo::PathEl::ClosePath));
        Self {
            curves,
            closed,
            total,
        }
    }
    pub(super) fn point_at(&self, dist: f64) -> Point {
        use crate::evaluation::property_follow_path_sample::point_tangent_at;
        let point = point_tangent_at(&self.curves, self.closed, dist.clamp(0.0, self.total))
            .map(|p| p.point)
            .unwrap_or([0.0; 2]);
        Point::new(point[0], point[1])
    }
    pub(super) fn nearest(
        &self,
        p: [f64; 2],
    ) -> Result<
        Option<crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::NearestLocation>,
        crate::request_receipts::DispatchErrorCode,
    > {
        crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::nearest_on_curves(
            &self.curves,
            self.closed,
            p,
        )
        .map_err(|_| crate::request_receipts::DispatchErrorCode::Unavailable)
    }
    pub(super) fn tangent_at(&self, dist: f64) -> (f64, f64) {
        use crate::evaluation::property_follow_path_sample::point_tangent_at;
        let tangent = point_tangent_at(&self.curves, self.closed, dist.clamp(0.0, self.total))
            .and_then(|p| p.tangent)
            .unwrap_or([1.0, 0.0]);
        (tangent[0], tangent[1])
    }
}

// Bug found by stress-testing (2026-07-17): never calling `close_path()`
// here meant every downstream feature (stroke_feat's dense samples,
// turning-angle profile, centroid, Fourier descriptor — AND
// resample_stroke_inner's own resampling, which shares this same path
// builder) measured a CLOSED shape's own length short by its implicit
// closing segment — the same root cause fixed in tweens.js's
// buildTPFeat/resamplePJS/resamplePairFeatureAware, but here it's the
// WASM path real users actually hit (tried before the JS fallback). A
// vector-brush centerline stays open regardless (it's the drawn stroke,
// never a closed loop even when its rendered ribbon outline is) — mirrors
// stroke_feat's own `is_vb(s)` exemption a few lines below.
pub(super) fn build_feat_path(s: &StrokeIn) -> BezPath {
    let mut path = BezPath::new();
    let vb = is_vb(s);
    if vb {
        let cs = s.center_segments.as_ref().unwrap();
        add_segs(
            &mut path,
            cs.iter().map(|c| (c.point, c.handle_in, c.handle_out)),
        );
    } else {
        add_segs(
            &mut path,
            s.segments
                .iter()
                .map(|c| (c.point, c.handle_in, c.handle_out)),
        );
        if s.closed && s.segments.len() > 1 {
            let a = s.segments.last().unwrap();
            let b = &s.segments[0];
            path.curve_to(
                (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]),
                (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]),
                (b.point[0], b.point[1]),
            );
            path.close_path();
        }
    }
    path
}
pub(super) fn add_segs(
    path: &mut BezPath,
    iter: impl Iterator<Item = ([f64; 2], [f64; 2], [f64; 2])>,
) {
    let items: Vec<_> = iter.collect();
    if items.is_empty() {
        return;
    }
    path.move_to((items[0].0[0], items[0].0[1]));
    for i in 0..items.len() - 1 {
        let (a_pt, _a_in, a_out) = items[i];
        let (b_pt, b_in, _b_out) = items[i + 1];
        let c1 = (a_pt[0] + a_out[0], a_pt[1] + a_out[1]);
        let c2 = (b_pt[0] + b_in[0], b_pt[1] + b_in[1]);
        path.curve_to(c1, c2, (b_pt[0], b_pt[1]));
    }
}

// ---- color parsing / distance (parseHexColor / colorDist) ----
