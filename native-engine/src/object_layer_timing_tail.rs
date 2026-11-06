//! The timing half of a retained tail (Frames field shrunk below the authored cells, then grown again). Baseline
//! `setTotalFrames` (src/js/timeline.js at 1065e9d8) never truncates the frames array, so 120 -> 30 -> 120 brings
//! every cell back. The sparse timing keeps keys and samples at tail-relative frames while the live timing stays
//! inside the project length.
use crate::object_document::object_deserialize;
use crate::object_document::timing::{LayerTiming, LayerTimingError, TimingKey, TimingSample};
use serde::{Deserialize, Serialize};

/// Keys and samples past a cut, as frames relative to the cut (0 is the first frame past the new end).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub(crate) struct TimingTail {
    keys: Vec<TimingKey>,
    samples: Vec<TimingSample>,
}
object_deserialize!(TimingTail { keys: Vec<TimingKey>, samples: Vec<TimingSample> });

impl TimingTail {
    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.samples.is_empty()
    }
    /// Every entry moves `by` frames later; entries that would pass `limit` are dropped (they could never return).
    pub(crate) fn shift(&mut self, by: u32, limit: u32) {
        self.keys
            .retain_mut(|key| shift_frame(&mut key.frame, by, limit));
        self.samples
            .retain_mut(|sample| shift_frame(&mut sample.frame, by, limit));
    }
    /// Split at `k`: the entries before it (kept as they are) and the rest re-based on `k`.
    pub(crate) fn split_front(self, k: u32) -> (Self, Self) {
        let (front_keys, mut rest_keys): (Vec<_>, Vec<_>) =
            self.keys.into_iter().partition(|key| key.frame < k);
        let (front_samples, mut rest_samples): (Vec<_>, Vec<_>) = self
            .samples
            .into_iter()
            .partition(|sample| sample.frame < k);
        rest_keys.iter_mut().for_each(|key| key.frame -= k);
        rest_samples.iter_mut().for_each(|sample| sample.frame -= k);
        (
            Self {
                keys: front_keys,
                samples: front_samples,
            },
            Self {
                keys: rest_keys,
                samples: rest_samples,
            },
        )
    }
    /// `front` precedes this tail (its entries are all earlier than anything already here).
    pub(crate) fn prepend(&mut self, mut front: Self) {
        front.keys.append(&mut self.keys);
        front.samples.append(&mut self.samples);
        *self = front;
    }
    /// Ordered and unique, as the live timing requires.
    pub(crate) fn well_formed(&self, limit: u32) -> bool {
        let ordered = |frames: Vec<u32>| {
            frames.windows(2).all(|pair| pair[0] < pair[1]) && frames.iter().all(|f| *f < limit)
        };
        ordered(self.keys.iter().map(|key| key.frame).collect())
            && ordered(self.samples.iter().map(|sample| sample.frame).collect())
            && self
                .samples
                .iter()
                .all(|sample| self.keys.iter().all(|key| key.frame != sample.frame))
    }
}

fn shift_frame(frame: &mut u32, by: u32, limit: u32) -> bool {
    match frame.checked_add(by) {
        Some(next) if next < limit => {
            *frame = next;
            true
        }
        _ => false,
    }
}

impl LayerTiming {
    /// The entries at or after `at`, re-based on `at`.
    pub(crate) fn tail_from(&self, at: u32) -> TimingTail {
        TimingTail {
            keys: self
                .keys
                .iter()
                .filter(|key| key.frame >= at)
                .map(|key| TimingKey {
                    frame: key.frame - at,
                })
                .collect(),
            samples: self
                .samples
                .iter()
                .filter(|sample| sample.frame >= at)
                .map(|sample| TimingSample {
                    frame: sample.frame - at,
                    manual_edit: sample.manual_edit.clone(),
                })
                .collect(),
        }
    }
    /// This timing with `tail` laid after frame `at - 1`; the result is validated against `total`.
    pub(crate) fn with_tail(
        &self,
        tail: &TimingTail,
        at: u32,
        total: u32,
    ) -> Result<LayerTiming, LayerTimingError> {
        let mut next = self.clone();
        next.keys.extend(tail.keys.iter().map(|key| TimingKey {
            frame: key.frame + at,
        }));
        next.samples
            .extend(tail.samples.iter().map(|sample| TimingSample {
                frame: sample.frame + at,
                manual_edit: sample.manual_edit.clone(),
            }));
        next.validate(total)?;
        Ok(next)
    }
}
