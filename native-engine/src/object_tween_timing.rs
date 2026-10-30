//! Replace generated samples without exposing mutable timing fields to the tween planner.
use crate::object_document::timing::{LayerTiming, LayerTimingError, TimingSample};
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
    /// Harmonize (baseline #126): once the spans around an edited frame are re-tweened, that frame
    /// goes back from a key to a SOFT in-between flagged as manually edited. Its drawing is untouched;
    /// only its exposure changes, so a later regeneration that skips manual frames keeps the correction.
    pub(crate) fn soften_key(&self, frame: u32, total: u32) -> Result<Self, LayerTimingError> {
        self.validate(total)?;
        let at = self
            .keys
            .binary_search_by_key(&frame, |k| k.frame)
            .map_err(|_| LayerTimingError::Invalid)?;
        let mut next = self.clone();
        next.keys.remove(at);
        next.samples.push(TimingSample {
            frame,
            manual_edit: Presence::Present(true),
        });
        next.samples.sort_by_key(|s| s.frame);
        next.validate(total)?;
        Ok(next)
    }
}
