//! Sparse timing plan for the dense editor's Repeat Selection (F13): the timing half of `repeatSelection`
//! (src/js/timeline.js at fbe4b8a3), over the private `LayerTiming` fields.
//!
//! The cycle copies each cell of the frame range `first..=last` to `f + r * span` for `r` in `1..=times`
//! (`span = last - first + 1`; the baseline `dst = maxF + (r - 1) * span + (f - minF) + 1` is the same frame). A
//! destination cell is REPLACED whole: whatever key or sample it held is gone, and it becomes exactly what the source
//! cell is. A key copies as a key; an in-between copies as an in-between WITHOUT its manual-edit flag (the baseline
//! writes `{strokes, isKeyframe, isInterpolated}` and nothing else, so the flag is absent on the copy, which is a
//! different fact from `false`); a hold copies as a hold. The source cells are never written, because every
//! destination lies after `last`.
//!
//! Only the sparse entries are visited: a range of holds costs nothing however long it is, and the work is bounded by
//! the entries the timing already carries times `times`. The input is only borrowed and the result is validated
//! against the grown frame count by the shared timing invariant (ordering, bounds, key/sample exclusion, the
//! 4096-entry cap, which the dense editor does not have), so a refusal cannot leave a partial edit.
use crate::object_document::timing::{
    LayerTiming, LayerTimingError, TimingEdit, TimingKey, TimingSample,
};
use crate::object_document::Presence;

/// What a cycle does to one layer's timing: the replacement (or nothing to publish) and the cells that carry content.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RepeatPlan {
    /// The replacement timing, validated for `grown`; `NoChange` when the copies equal what is already there.
    pub(crate) edit: TimingEdit,
    /// Source frames inside the range that hold a key or a sample, ascending. Holds have no content to copy.
    pub(crate) sources: Vec<u32>,
}

impl LayerTiming {
    /// Plan the cycle of `first..=last` repeated `times` times on a timeline of `total` frames that grows to `grown`.
    /// `times` is the already clamped count (1 or more). The caller computes `grown` as
    /// `max(total, last + 1 + span * times)`; anything smaller than the last destination frame is `Invalid`.
    pub(crate) fn repeat_range(
        &self,
        first: u32,
        last: u32,
        times: u32,
        total: u32,
        grown: u32,
    ) -> Result<RepeatPlan, LayerTimingError> {
        self.validate(total)?;
        let invalid = || LayerTimingError::Invalid;
        let span = last
            .checked_sub(first)
            .and_then(|distance| distance.checked_add(1))
            .ok_or_else(invalid)?;
        if last >= total || times == 0 || grown < total {
            return Err(invalid());
        }
        let window_end = u64::from(last) + u64::from(span) * u64::from(times);
        if window_end >= u64::from(grown) {
            return Err(invalid());
        }
        let in_window = |frame: u32| frame > last && u64::from(frame) <= window_end;
        let in_range = |frame: u32| frame >= first && frame <= last;

        let mut keys: Vec<TimingKey> = self
            .keys
            .iter()
            .filter(|key| !in_window(key.frame))
            .cloned()
            .collect();
        let mut samples: Vec<TimingSample> = self
            .samples
            .iter()
            .filter(|sample| !in_window(sample.frame))
            .cloned()
            .collect();
        for round in 1..=times {
            // span * round <= window_end - last < grown, so the shift and every destination fit in u32.
            let shift = span * round;
            for key in self.keys.iter().filter(|key| in_range(key.frame)) {
                keys.push(TimingKey {
                    frame: key.frame + shift,
                });
            }
            for sample in self.samples.iter().filter(|sample| in_range(sample.frame)) {
                samples.push(TimingSample {
                    frame: sample.frame + shift,
                    manual_edit: Presence::Missing,
                });
            }
        }
        keys.sort_by_key(|key| key.frame);
        samples.sort_by_key(|sample| sample.frame);

        let mut sources: Vec<u32> = self
            .keys
            .iter()
            .map(|key| key.frame)
            .chain(self.samples.iter().map(|sample| sample.frame))
            .filter(|frame| in_range(*frame))
            .collect();
        sources.sort_unstable();

        let mut next = self.clone();
        next.keys = keys;
        next.samples = samples;
        if next == *self {
            return Ok(RepeatPlan {
                edit: TimingEdit::NoChange,
                sources,
            });
        }
        next.validate(grown)?;
        Ok(RepeatPlan {
            edit: TimingEdit::Changed(next),
            sources,
        })
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_repeat_timing.rs"]
mod tests;
