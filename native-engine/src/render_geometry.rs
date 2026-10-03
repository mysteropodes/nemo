//! Bounded immutable cubic contours. Handles are relative to their anchors.
//! Control bounds are conservative clipping metadata, never the painted shape.

use vello::kurbo::{Affine, BezPath, PathEl, Rect};

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
        if !valid_bounds(bounds) {
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
        if !transform.iter().all(|v| v.is_finite())
            || !determinant.is_finite()
            || determinant == 0.0
        {
            return Err("cubic transform must be finite and invertible");
        }
        let [x0, y0, x1, y1] = self.control_bounds;
        let envelope = Affine::new(transform).transform_rect_bbox(Rect::new(x0, y0, x1, y1));
        let bounds = [envelope.x0, envelope.y0, envelope.x1, envelope.y1];
        if !valid_bounds(bounds) || !bounds.iter().all(|v| (*v as f32).is_finite()) {
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LayerShape {
    Rectangle,
    ClosedPath(ClosedCubicPath),
}
