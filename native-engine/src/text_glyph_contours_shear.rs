//! Faux-italic shear and the scene-join flag report of the glyph contour kernel
//! (G38b / L06-B). See `text_glyph_contours.rs` for the baseline contract.

use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    GlyphContourError, GlyphSegment,
};

/// Faux italic: `Matrix(1, 0, -skew, 1, skew * baselineY, 0)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItalicShear {
    skew: f64,
    baseline_y: f64,
}

impl ItalicShear {
    pub(crate) fn new(skew: f64, baseline_y: f64) -> Result<Self, GlyphContourError> {
        if skew.is_finite() && baseline_y.is_finite() {
            Ok(Self { skew, baseline_y })
        } else {
            Err(GlyphContourError::NonFiniteShear)
        }
    }

    pub(crate) fn skew(&self) -> f64 {
        self.skew
    }

    /// `[a, b, c, d, tx, ty]`; Paper's `Item.transform` prepends it onto an
    /// identity item matrix, which reproduces these values exactly.
    pub(crate) fn matrix(&self) -> [f64; 6] {
        [1.0, 0.0, -self.skew, 1.0, self.skew * self.baseline_y, 0.0]
    }
}

/// What the scene join must know before building a `ClosedCubicPath`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathFlag {
    /// The path has this many segments, outside the admitted 2..=256.
    SegmentCount(usize),
    /// Curve `n` (to the next anchor, the last one closing the ring) has its
    /// controls and end within 1e-12 of its start at f32 precision.
    DegenerateCurve(usize),
}

/// Paper `Numerical.isZero` on a point: both axes within 1e-12.
fn paper_zero(v: [f64; 2]) -> bool {
    v[0].abs() <= 1e-12 && v[1].abs() <= 1e-12
}

fn apply_matrix(m: &[f64; 6], x: f64, y: f64) -> [f64; 2] {
    [x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5]]
}

/// Paper `Path.transform` of a segment list: anchors by the matrix, a
/// non-zero handle as `T(anchor + handle) - T(anchor)`.
pub(crate) fn shear_segments(segments: &[GlyphSegment], shear: &ItalicShear) -> Vec<GlyphSegment> {
    let m = shear.matrix();
    segments
        .iter()
        .map(|s| {
            let p = apply_matrix(&m, s.point[0], s.point[1]);
            let handle = |h: [f64; 2]| {
                if paper_zero(h) {
                    h
                } else {
                    let q = apply_matrix(&m, h[0] + s.point[0], h[1] + s.point[1]);
                    [q[0] - p[0], q[1] - p[1]]
                }
            };
            GlyphSegment {
                point: p,
                handle_in: handle(s.handle_in),
                handle_out: handle(s.handle_out),
            }
        })
        .collect()
}

/// The report for the scene join. See [`PathFlag`].
pub(crate) fn path_flags(segments: &[GlyphSegment]) -> Vec<PathFlag> {
    let mut flags = Vec::new();
    if !(2..=256).contains(&segments.len()) {
        flags.push(PathFlag::SegmentCount(segments.len()));
    }
    let count = segments.len();
    for (index, from) in segments.iter().enumerate() {
        let to = &segments[(index + 1) % count];
        let start = [from.point[0] as f32, from.point[1] as f32];
        let c1 = [
            from.point[0] + from.handle_out[0],
            from.point[1] + from.handle_out[1],
        ];
        let c2 = [to.point[0] + to.handle_in[0], to.point[1] + to.handle_in[1]];
        if [c1, c2, to.point].iter().all(|p| {
            (p[0] as f32 - start[0]).abs() <= 1e-12_f32
                && (p[1] as f32 - start[1]).abs() <= 1e-12_f32
        }) {
            flags.push(PathFlag::DegenerateCurve(index));
        }
    }
    flags
}
