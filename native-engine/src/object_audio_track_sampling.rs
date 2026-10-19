//! Stored AudioTrack volume sampling: actual native curve math, never a fabricated layer holder.
use super::AudioTrackRecord;
use crate::animation_curve::{evaluate_curve, CurveSample};
use crate::object_document::media_timing::object_media_timing_audio::VolumeSampler;
use crate::object_document::OrderKey;

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}
fn value(key: &OrderKey) -> Option<f64> {
    if key.v.len() != 1 {
        return None;
    }
    key.v[0].as_f64().and_then(finite)
}
fn frame(key: &OrderKey) -> Option<f64> {
    key.frame.as_f64().and_then(finite)
}
fn curve(key: &OrderKey) -> Option<Vec<CurveSample>> {
    match key.curve_points.value().and_then(Option::as_ref) {
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
impl AudioTrackRecord {
    /// Same precedence as audio-bridge.js volumeAt: animated → Motion static → legacy →1.
    /// No clamp here; gain clamping belongs to the existing audio timing consumer.
    pub(crate) fn sample_volume(&self, at: f64) -> Option<f64> {
        finite(at)?;
        let keys = self
            .motion
            .value()
            .and_then(Option::as_ref)
            .and_then(|map| map.get("volume"))
            .map(|t| t.keys.as_slice())
            .unwrap_or(&[]);
        if !keys.is_empty() {
            let first = &keys[0];
            let last = keys.last()?;
            if at <= frame(first)? {
                return value(first);
            }
            if at >= frame(last)? {
                return value(last);
            }
            let right = keys.partition_point(|k| frame(k).is_some_and(|f| f <= at));
            if right == 0 || right >= keys.len() {
                return None;
            }
            let a = &keys[right - 1];
            let b = &keys[right];
            if a.hold.value().copied().unwrap_or(false) {
                return value(a);
            }
            let (start, end) = (frame(a)?, frame(b)?);
            if end <= start {
                return None;
            }
            let eased = evaluate_curve(&curve(a)?, (at - start) / (end - start));
            return finite(value(a)? + (value(b)? - value(a)?) * eased);
        }
        if let Some(v) = self
            .motion_static
            .value()
            .and_then(Option::as_ref)
            .and_then(|m| m.get("volume"))
        {
            if v.len() != 1 {
                return None;
            }
            return v[0].as_f64().and_then(finite);
        }
        self.volume
            .value()
            .map_or(Some(1.), |v| v.as_f64().and_then(finite))
    }
}
impl VolumeSampler for AudioTrackRecord {
    fn volume_at(&self, frame: f64) -> Option<f64> {
        self.sample_volume(frame)
    }
}
#[cfg(test)]
#[path = "../tests/object_audio_track_sampling.rs"]
mod tests;
