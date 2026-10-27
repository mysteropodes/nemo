//! Immutable evaluation ports used by tween planning, without a second evaluator.
use crate::animation_curve::{evaluate_curve, CurveSample};
pub(crate) use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::{
    nearest_on_curves, NearestLocation,
};
pub(crate) use crate::evaluation::property_follow_path_sample::{path_length, point_tangent_at};
use crate::object_codec::OrderCurvePoint;
pub(crate) fn tween_easing(points: &[OrderCurvePoint], t: f64) -> f64 {
    let points: Vec<_> = points
        .iter()
        .map(|p| CurveSample {
            x: p.x.as_f64().unwrap(),
            y: p.y.as_f64().unwrap(),
            tx: p.tx.value().and_then(|n| n.as_f64()),
            ty: p.ty.value().and_then(|n| n.as_f64()),
        })
        .collect();
    evaluate_curve(&points, t)
}

#[cfg(feature = "project_media_codec")]
#[path = "tween_png_encode.rs"]
mod png;
#[cfg(feature = "project_media_codec")]
pub(crate) use png::encode_brush_png;
