//! Bounded immutable cubic contours. Handles are relative to their anchors.
//! Control bounds are conservative clipping metadata, never the painted shape.

use vello::kurbo::{Affine, BezPath, PathEl, Rect};

const MAX_CONTROL_DRIFT: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicSegment {
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClosedCubicPath {
    segments: Box<[CubicSegment]>,
    path: BezPath,
    control_bounds: [f64; 4],
}

impl ClosedCubicPath {
    /// Own one contour of 2..=256 segments. Each segment must survive Vello's
    /// local f32 encoding: at least one control/end differs from its start by
    /// more than 1e-12 on an axis. Degenerate segments are rejected, not dropped.
    pub fn new(segments: Vec<CubicSegment>) -> Result<Self, &'static str> {
        if !(2..=256).contains(&segments.len()) {
            return Err("closed cubic paths require 2..=256 segments");
        }
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for segment in &segments {
            if !segment
                .point
                .iter()
                .chain(&segment.handle_in)
                .chain(&segment.handle_out)
                .all(|value| value.is_finite())
            {
                return Err("cubic anchors and relative handles must be finite");
            }
            for point in [
                segment.point,
                control(segment, segment.handle_in),
                control(segment, segment.handle_out),
            ] {
                if !point.iter().all(|value| value.is_finite()) {
                    return Err("absolute cubic controls must be finite");
                }
                bounds[0] = bounds[0].min(point[0]);
                bounds[1] = bounds[1].min(point[1]);
                bounds[2] = bounds[2].max(point[0]);
                bounds[3] = bounds[3].max(point[1]);
            }
        }
        if !valid_bounds(bounds) || !valid_gpu_bounds(bounds) {
            return Err("cubic control bounds must have finite positive extent");
        }
        // A diagonal line also has a positive bounding box but a zero-area hull.
        let origin = segments[0].point;
        let normalized = |p: [f64; 2]| {
            [
                (p[0] - origin[0]) / (bounds[2] - bounds[0]),
                (p[1] - origin[1]) / (bounds[3] - bounds[1]),
            ]
        };
        let points: Vec<_> = segments
            .iter()
            .flat_map(|s| [s.point, control(s, s.handle_in), control(s, s.handle_out)])
            .map(normalized)
            .collect();
        let direction = points.iter().find(|p| **p != [0.0, 0.0]).unwrap();
        if points
            .iter()
            .all(|p| direction[0] * p[1] - direction[1] * p[0] == 0.0)
        {
            return Err("cubic control hull must have positive area");
        }
        let mut path = BezPath::new();
        path.move_to((origin[0], origin[1]));
        for index in 0..segments.len() {
            let from = &segments[index];
            let to = &segments[(index + 1) % segments.len()];
            let c1 = control(from, from.handle_out);
            let c2 = control(to, to.handle_in);
            // Vello 0.9 drops locally degenerate curves before applying the
            // affine. Check the closing curve too, regardless of magnification.
            let start = from.point.map(|v| v as f32);
            if [c1, c2, to.point].iter().all(|p| {
                (p[0] as f32 - start[0]).abs() <= 1e-12 && (p[1] as f32 - start[1]).abs() <= 1e-12
            }) {
                return Err("cubic segment is degenerate at Vello's local encoding precision");
            }
            path.curve_to((c1[0], c1[1]), (c2[0], c2[1]), (to.point[0], to.point[1]));
        }
        path.close_path();
        Ok(Self {
            segments: segments.into_boxed_slice(),
            path,
            control_bounds: bounds,
        })
    }

    pub fn segments(&self) -> &[CubicSegment] {
        &self.segments
    }
    pub fn commands(&self) -> &[PathEl] {
        self.path.elements()
    }
    pub fn control_bounds(&self) -> [f64; 4] {
        self.control_bounds
    }
    pub(crate) fn path(&self) -> &BezPath {
        &self.path
    }

    pub(crate) fn transformed_envelope(
        &self,
        transform: [f64; 6],
    ) -> Result<[f64; 4], &'static str> {
        let determinant = transform[0] * transform[3] - transform[1] * transform[2];
        // Vello encodes both local controls and affine coefficients as f32.
        // A finite f64 envelope cannot rescue an overflowing or singular input
        // at that encoding boundary.
        let gpu = transform.map(|v| v as f32);
        let gpu_determinant = gpu[0] * gpu[3] - gpu[1] * gpu[2];
        if !transform.iter().all(|v| v.is_finite())
            || !determinant.is_finite()
            || determinant == 0.0
            || !gpu.iter().all(|v| v.is_finite())
            || !gpu_determinant.is_finite()
            || gpu_determinant == 0.0
        {
            return Err("cubic transform must be finite and invertible");
        }
        // Reject cancellation/rounding that would move an encoded control by
        // more than 1/4 output pixel. The clip is derived in f64, while Vello
        // encodes local coordinates and then applies the encoded affine.
        for point in self
            .segments
            .iter()
            .flat_map(|s| [s.point, control(s, s.handle_in), control(s, s.handle_out)])
        {
            let [x, y] = point.map(|v| v as f32);
            let encoded = [
                gpu[0].mul_add(x, gpu[2].mul_add(y, gpu[4])),
                gpu[1].mul_add(x, gpu[3].mul_add(y, gpu[5])),
            ];
            let exact = [
                transform[0] * point[0] + transform[2] * point[1] + transform[4],
                transform[1] * point[0] + transform[3] * point[1] + transform[5],
            ];
            // Four f32 roundings conservatively cover two products and two
            // additions (including a shader choosing fused multiply-add).
            let rounding = [
                f64::from(gpu[0]).abs() * f64::from(x).abs()
                    + f64::from(gpu[2]).abs() * f64::from(y).abs()
                    + f64::from(gpu[4]).abs(),
                f64::from(gpu[1]).abs() * f64::from(x).abs()
                    + f64::from(gpu[3]).abs() * f64::from(y).abs()
                    + f64::from(gpu[5]).abs(),
            ]
            .map(|magnitude| magnitude * 4.0 * f64::from(f32::EPSILON));
            if encoded
                .iter()
                .zip(exact)
                .zip(rounding)
                .any(|((encoded, exact), margin)| {
                    !encoded.is_finite()
                        || !exact.is_finite()
                        || (f64::from(*encoded) - exact).abs() + margin > MAX_CONTROL_DRIFT
                })
            {
                return Err("cubic GPU rounding bound exceeds the output precision budget");
            }
        }
        let [x0, y0, x1, y1] = self.control_bounds;
        let envelope = Affine::new(transform).transform_rect_bbox(Rect::new(x0, y0, x1, y1));
        let bounds = [envelope.x0, envelope.y0, envelope.x1, envelope.y1];
        let padded = [
            bounds[0].floor() - 1.0,
            bounds[1].floor() - 1.0,
            bounds[2].ceil() + 1.0,
            bounds[3].ceil() + 1.0,
        ];
        if !valid_bounds(bounds) || !valid_gpu_bounds(bounds) || !valid_gpu_bounds(padded) {
            return Err("transformed cubic envelope must have finite positive GPU extent");
        }
        Ok(bounds)
    }
}

fn control(segment: &CubicSegment, handle: [f64; 2]) -> [f64; 2] {
    [segment.point[0] + handle[0], segment.point[1] + handle[1]]
}

fn valid_bounds([x0, y0, x1, y1]: [f64; 4]) -> bool {
    [x0, y0, x1, y1, x1 - x0, y1 - y0]
        .iter()
        .all(|v| v.is_finite())
        && x1 > x0
        && y1 > y0
}

fn valid_gpu_bounds(bounds: [f64; 4]) -> bool {
    valid_bounds(bounds.map(|v| f64::from(v as f32)))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LayerShape {
    Rectangle,
    ClosedPath(ClosedCubicPath),
}
