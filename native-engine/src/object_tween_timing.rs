//! Replace generated samples without exposing mutable timing fields to the tween planner.
use super::{LayerTiming, LayerTimingError, TimingSample};
use crate::object_document::Presence;
impl LayerTiming {
    pub(crate) fn tween_samples(
        &self,
        from: u32,
        to: u32,
        frames: &[u32],
        skip_manual: bool,
        total: u32,
    ) -> Result<Self, LayerTimingError> {
        self.validate(total)?;
        if from >= to
            || to >= total
            || frames.windows(2).any(|f| f[0] >= f[1])
            || frames.iter().any(|&f| f <= from || f >= to)
        {
            return Err(LayerTimingError::Invalid);
        }
        let mut next = self.clone();
        // Blank keys inside two drawn endpoints are overwritten exactly as generateTweens does.
        next.keys.retain(|k| k.frame <= from || k.frame >= to);
        next.samples.retain(|s| {
            s.frame <= from
                || s.frame >= to
                || (skip_manual && s.manual_edit.value() == Some(&true))
        });
        for &frame in frames {
            if next.samples.iter().any(|s| s.frame == frame) {
                continue;
            }
            next.samples.push(TimingSample {
                frame,
                manual_edit: Presence::Missing,
            });
        }
        next.samples.sort_by_key(|s| s.frame);
        next.validate(total)?;
        Ok(next)
    }
}
