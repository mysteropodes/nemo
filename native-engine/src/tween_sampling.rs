//! #2358: tween sampling; adapted from geometry-wasm at 46442ea9.
use crate::tween_kernels::*;

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

// Nearest-point admission remains in the tween command read port.
impl PathSampler {
    pub(super) fn nearest(
        &self,
        p: [f64; 2],
    ) -> Result<
        Option<crate::object_snapshot::tween_read::NearestLocation>,
        crate::request_receipts::DispatchErrorCode,
    > {
        crate::object_snapshot::tween_read::nearest_on_curves(&self.curves, self.closed, p)
            .map_err(|_| crate::request_receipts::DispatchErrorCode::Unavailable)
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
// ---- color parsing / distance (parseHexColor / colorDist) ----
