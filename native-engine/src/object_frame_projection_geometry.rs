//! True cubic bounds and finite owned-vector conversion. No anchor/control-hull pivot.
use crate::evaluation::object_vector_kernels::render_vector_eval::{EvalSegment, EvaluatedVector};
use crate::evaluation::property_projection_3d::Bounds;
use crate::object_frame_packet::projection::ProjectionError;
use kurbo::{BezPath, Rect, Shape};

pub(super) fn finite(vector: &EvaluatedVector) -> Result<(), ProjectionError> {
    if vector.segments.iter().all(|s| {
        s.point
            .iter()
            .chain(&s.handle_in)
            .chain(&s.handle_out)
            .all(|n| n.is_finite())
            && [s.handle_in, s.handle_out]
                .iter()
                .all(|h| (s.point[0] + h[0]).is_finite() && (s.point[1] + h[1]).is_finite())
    }) {
        Ok(())
    } else {
        Err(ProjectionError::NonFinite)
    }
}

fn path(vector: &EvaluatedVector) -> Result<Option<BezPath>, ProjectionError> {
    finite(vector)?;
    let Some(first) = vector.segments.first() else {
        return Ok(None);
    };
    let mut path = BezPath::new();
    path.move_to((first.point[0], first.point[1]));
    // A move-only kurbo path has Rect::ZERO bounds; the actual one-anchor path is not at the origin.
    if vector.segments.len() == 1 && !vector.closed {
        path.line_to((first.point[0], first.point[1]));
    }
    let mut append = |a: &EvalSegment, b: &EvalSegment| {
        path.curve_to(
            (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]),
            (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]),
            (b.point[0], b.point[1]),
        );
    };
    for pair in vector.segments.windows(2) {
        append(&pair[0], &pair[1]);
    }
    if vector.closed {
        append(vector.segments.last().unwrap(), first);
        path.close_path();
    }
    Ok(Some(path))
}

/// One layer-wide bound after the caller's actual preceding geometry stages.
/// An empty layer owns no fabricated origin/identity pivot.
pub(super) fn bounds(vectors: &[EvaluatedVector]) -> Result<Option<Bounds>, ProjectionError> {
    let mut union: Option<Rect> = None;
    for vector in vectors {
        if let Some(path) = path(vector)? {
            let b = path.bounding_box();
            if [b.x0, b.y0, b.x1, b.y1].iter().any(|n| !n.is_finite()) {
                return Err(ProjectionError::NonFinite);
            }
            union = Some(union.map_or(b, |u| u.union(b)));
        }
    }
    union
        .map(|b| {
            let bounds = Bounds {
                x: b.x0,
                y: b.y0,
                width: b.width(),
                height: b.height(),
            };
            if [bounds.x, bounds.y, bounds.width, bounds.height]
                .iter()
                .all(|n| n.is_finite())
            {
                Ok(bounds)
            } else {
                Err(ProjectionError::NonFinite)
            }
        })
        .transpose()
}

pub(super) fn mapped(
    vector: &EvaluatedVector,
    map: impl Fn([f64; 2]) -> [f64; 2],
) -> Result<EvaluatedVector, ProjectionError> {
    finite(vector)?;
    let out = EvaluatedVector {
        source_object_index: vector.source_object_index,
        closed: vector.closed,
        segments: vector
            .segments
            .iter()
            .map(|s| {
                let point = map(s.point);
                let relative = |h: [f64; 2]| {
                    let absolute = map([s.point[0] + h[0], s.point[1] + h[1]]);
                    [absolute[0] - point[0], absolute[1] - point[1]]
                };
                EvalSegment {
                    point,
                    handle_in: relative(s.handle_in),
                    handle_out: relative(s.handle_out),
                }
            })
            .collect(),
    };
    finite(&out)?;
    Ok(out)
}
