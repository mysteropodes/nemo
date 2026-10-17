//! Drawn geometry of the editable vector family: open or closed cubic paths (G14, #2107, leaf L01-I).
//!
//! `ClosedCubicPath` (render_geometry.rs) admits only closed contours with a positive-area hull, which an open line, a
//! stroke-only line or an unfilled polyline never has, and it is not weakened here. A vector record is instead drawn from a
//! `BezPath` built exactly as the baseline engine builds it (geometry-wasm engine.rs `build_bezpath_from_segments`: a
//! `move_to` the first anchor, one cubic per consecutive pair, the closing cubic and `close_path` only when `closed`), so
//! the same evaluated anchors give the same fill and stroke in the scene, the viewport preview and the export.
//!
//! The scene entry still carries a `ClosedCubicPath` (`ObjectRenderEntry::contour`) and a conservative `envelope`, so every
//! existing reader keeps working:
//! * a CLOSED path whose contour `ClosedCubicPath` admits keeps that real contour (byte-identical to what the closed
//!   fill-only vector of L01-H already drew); its envelope is the contour envelope without a stroke, and the bounds
//!   padded by the stroke reach with one;
//! * every other vector path (open, or closed but hull-less such as a straight stroked line) carries the rectangle of its
//!   control bounds padded by the stroke reach (or the anti-aliasing margin) as its clipping contour. That rectangle is
//!   clipping metadata, never the painted shape: the painted shape is always `VectorShape::path`.
//!
//! Typed refusals, never defaults: an anchor count outside 2..=256 (the codec's and `ClosedCubicPath`'s bound, unchanged),
//! a non-finite coordinate, a path with no extent at all (every anchor and handle at one point), and an envelope the
//! GPU encoding cannot hold (the same checks `ClosedCubicPath` applies, reached through the padded rectangle).
use crate::evaluation::object_vector_kernels::render_vector_stroke::ENVELOPE_MARGIN;
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use vello::kurbo::BezPath;

/// Scene bound on anchors of one vector path (equal to the codec bound and to `ClosedCubicPath`).
pub(crate) const MAX_ANCHORS: usize = 256;
const IDENTITY: [f64; 6] = [1., 0., 0., 1., 0., 0.];

/// One vector path ready for the scene: what is drawn, and the metadata every scene entry carries.
#[derive(Debug)]
pub(crate) struct VectorShape {
    pub(crate) contour: ClosedCubicPath,
    pub(crate) envelope: [f64; 4],
    pub(crate) path: BezPath,
}

/// geometry-wasm engine.rs `build_bezpath_from_segments`, handles relative to their anchors.
pub(crate) fn build_path(segments: &[CubicSegment], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let Some(first) = segments.first() else {
        return path;
    };
    path.move_to((first.point[0], first.point[1]));
    let count = segments.len();
    let curves = if closed { count } else { count - 1 };
    for index in 0..curves {
        let from = &segments[index];
        let to = &segments[(index + 1) % count];
        let c1 = (
            from.point[0] + from.handle_out[0],
            from.point[1] + from.handle_out[1],
        );
        let c2 = (to.point[0] + to.handle_in[0], to.point[1] + to.handle_in[1]);
        path.curve_to(c1, c2, (to.point[0], to.point[1]));
    }
    if closed {
        path.close_path();
    }
    path
}

/// Min/max over every anchor and absolute control point.
fn control_bounds(segments: &[CubicSegment]) -> Result<[f64; 4], &'static str> {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for segment in segments {
        let controls = [
            segment.point,
            [
                segment.point[0] + segment.handle_in[0],
                segment.point[1] + segment.handle_in[1],
            ],
            [
                segment.point[0] + segment.handle_out[0],
                segment.point[1] + segment.handle_out[1],
            ],
        ];
        for point in controls {
            if !point.iter().all(|value| value.is_finite()) {
                return Err("vector anchors and handles must be finite");
            }
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
        }
    }
    if bounds[0] == bounds[2] && bounds[1] == bounds[3] {
        return Err("vector path has no extent");
    }
    Ok(bounds)
}

/// The padded control-bounds rectangle as a closed contour (four corners, no handles).
fn bounds_contour(bounds: [f64; 4], reach: f64) -> Result<ClosedCubicPath, &'static str> {
    let [x0, y0, x1, y1] = [
        bounds[0] - reach,
        bounds[1] - reach,
        bounds[2] + reach,
        bounds[3] + reach,
    ];
    let corner = |x: f64, y: f64| CubicSegment {
        point: [x, y],
        handle_in: [0., 0.],
        handle_out: [0., 0.],
    };
    ClosedCubicPath::new(vec![
        corner(x0, y0),
        corner(x1, y0),
        corner(x1, y1),
        corner(x0, y1),
    ])
}

impl VectorShape {
    /// `reach` is the stroke reach (`stroke_reach`); `None` means the item has no stroke.
    pub(crate) fn new(
        segments: &[CubicSegment],
        closed: bool,
        reach: Option<f64>,
    ) -> Result<Self, &'static str> {
        if !(2..=MAX_ANCHORS).contains(&segments.len()) {
            return Err("vector path requires 2..=256 anchors in the scene");
        }
        let bounds = control_bounds(segments)?;
        let path = build_path(segments, closed);
        let real = if closed {
            ClosedCubicPath::new(segments.to_vec()).ok()
        } else {
            None
        };
        let (contour, envelope) = match (real, reach) {
            (Some(real), None) => {
                let envelope = real.transformed_envelope(IDENTITY)?;
                (real, envelope)
            }
            (real, reach) => {
                let rect = bounds_contour(bounds, reach.unwrap_or(ENVELOPE_MARGIN))?;
                let envelope = rect.transformed_envelope(IDENTITY)?;
                (real.unwrap_or(rect), envelope)
            }
        };
        Ok(Self {
            contour,
            envelope,
            path,
        })
    }
}

#[cfg(test)]
#[path = "../tests/render_vector_path.rs"]
mod tests;
