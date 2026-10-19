//! Tight immutable Bezier path construction for the native parent pivot.
use crate::object_render_scene::text_scene::parent_chain::ChainUnavailable;
use vello::kurbo::BezPath;

/// Tight native Bezier bounds; never the control-point hull/envelope centre.
pub(super) fn curve_path(
    segments: &[([f64; 2], [f64; 2], [f64; 2])],
    closed: bool,
) -> Result<BezPath, ChainUnavailable> {
    if segments.is_empty() {
        return Err(ChainUnavailable::VectorGeometry);
    }
    if !segments
        .iter()
        .flat_map(|s| s.0.iter().chain(s.1.iter()).chain(s.2.iter()))
        .all(|v| v.is_finite())
    {
        return Err(ChainUnavailable::VectorGeometry);
    }
    let mut path = BezPath::new();
    path.move_to((segments[0].0[0], segments[0].0[1]));
    let ends = segments.len() + usize::from(closed);
    for i in 1..ends {
        let prev = segments[(i - 1) % segments.len()];
        let next = segments[i % segments.len()];
        path.curve_to(
            (prev.0[0] + prev.2[0], prev.0[1] + prev.2[1]),
            (next.0[0] + next.1[0], next.0[1] + next.1[1]),
            (next.0[0], next.0[1]),
        );
    }
    if closed {
        path.close_path();
    }
    Ok(path)
}
