//! T12: `SMMotion.timeRemapValue` over a video placement's stored track, on the
//! shared curve kernel (an evaluation module; the record stays in the document).
use crate::object_document::placement_offset::VideoTiming;
use crate::animation_curve::{evaluate_curve, CurveSample};
use crate::object_document::OrderKey;

impl VideoTiming {
    /// `SMMotion.timeRemapValue(ld, frame)`: `None` without a track or keys
    /// (the sampler's `null`), otherwise the eased value at `at`.
    pub(crate) fn remap_value(&self, at: f64) -> Option<f64> {
        let keys = self.time_remap.as_ref()?.keys.as_slice();
        let (first, last) = (keys.first()?, keys.last()?);
        if at <= frame(first)? {
            return value(first);
        }
        if at >= frame(last)? {
            return value(last);
        }
        let right = keys.partition_point(|key| frame(key).is_some_and(|f| f <= at));
        let (a, b) = (keys.get(right.checked_sub(1)?)?, keys.get(right)?);
        if a.hold.value().copied().unwrap_or(false) {
            return value(a);
        }
        let (start, end) = (frame(a)?, frame(b)?);
        let eased = evaluate_curve(&curve(a)?, (at - start) / (end - start));
        let result = value(a)? + (value(b)? - value(a)?) * eased;
        result.is_finite().then_some(result)
    }
}

fn frame(key: &OrderKey) -> Option<f64> {
    key.frame.as_f64().filter(|f| f.is_finite())
}
fn value(key: &OrderKey) -> Option<f64> {
    key.v.first()?.as_f64().filter(|v| v.is_finite())
}
fn curve(key: &OrderKey) -> Option<Vec<CurveSample>> {
    match key.curve_points.value().and_then(Option::as_ref) {
        // The baseline default ease (`object_audio_track_sampling` keeps the same table).
        None => Some(
            [(0., 0.), (0.25, 0.156), (0.5, 0.5), (0.75, 0.844), (1., 1.)]
                .into_iter()
                .map(|(x, y)| CurveSample {
                    x,
                    y,
                    tx: None,
                    ty: None,
                })
                .collect(),
        ),
        Some(points) => points
            .iter()
            .map(|p| {
                Some(CurveSample {
                    x: p.x.as_f64()?,
                    y: p.y.as_f64()?,
                    tx: p.tx.value().and_then(|n| n.as_f64()),
                    ty: p.ty.value().and_then(|n| n.as_f64()),
                })
            })
            .collect(),
    }
}
