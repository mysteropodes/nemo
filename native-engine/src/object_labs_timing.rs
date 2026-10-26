//! Sparse timing plans of two U36/U35 Labs timing actions (#2362), over the private `LayerTiming` fields:
//! `SMLabs.pingpongCycle` and `SMLabs.retimeExposure` (`src/js/labs/pingpong-cycle.js`, `retime-exposure.js` at
//! 1065e9d8). Like Repeat, a written cell becomes exactly its source: a key copies as a key, an in-between as an
//! in-between WITHOUT its manual-edit flag (the dense copy writes `{strokes, isKeyframe, isInterpolated}` only), a hold
//! as a hold. Results are validated against the grown frame count, so a refusal leaves no partial edit.
use crate::object_document::timing::{
    LayerTiming, LayerTimingError, TimingEdit, TimingKey, TimingSample,
};
use crate::object_document::Presence;

/// A plan: the replacement timing and the `(source, destination)` cells whose records the document must copy or move.
pub(crate) type CellPlan = (TimingEdit, Vec<(u32, u32)>);

impl LayerTiming {
    fn finish_plan(
        &self,
        keys: Vec<TimingKey>,
        samples: Vec<TimingSample>,
        grown: u32,
    ) -> Result<TimingEdit, LayerTimingError> {
        let mut next = self.clone();
        next.keys = keys;
        next.samples = samples;
        next.keys.sort_by_key(|key| key.frame);
        next.samples.sort_by_key(|sample| sample.frame);
        if next == *self {
            return Ok(TimingEdit::NoChange);
        }
        next.validate(grown)?;
        Ok(TimingEdit::Changed(next))
    }

    /// pingpongCycle on `first..=last` (two frames or more), `times` rounds: round r writes the block of `last - first`
    /// cells after the range, reversed (`last - k`) on odd rounds and forward (`first + k`) on even ones, k = 1..block.
    pub(crate) fn pingpong_range(
        &self,
        first: u32,
        last: u32,
        times: u32,
        total: u32,
        grown: u32,
    ) -> Result<CellPlan, LayerTimingError> {
        self.validate(total)?;
        let invalid = || LayerTimingError::Invalid;
        let block = last
            .checked_sub(first)
            .filter(|block| *block >= 1)
            .ok_or_else(invalid)?;
        let window_end = u64::from(last) + u64::from(block) * u64::from(times);
        if last >= total || times == 0 || grown < total || window_end >= u64::from(grown) {
            return Err(invalid());
        }
        let in_window = |frame: u32| frame > last && u64::from(frame) <= window_end;
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
        let mut pairs = Vec::new();
        let mut destination = last + 1;
        for round in 1..=times {
            for k in 1..=block {
                let source = if round % 2 == 1 { last - k } else { first + k };
                if self.keys.iter().any(|key| key.frame == source) {
                    keys.push(TimingKey { frame: destination });
                    pairs.push((source, destination));
                } else if self.samples.iter().any(|sample| sample.frame == source) {
                    samples.push(TimingSample {
                        frame: destination,
                        manual_edit: Presence::Missing,
                    });
                    pairs.push((source, destination));
                }
                destination += 1;
            }
        }
        Ok((self.finish_plan(keys, samples, grown)?, pairs))
    }

    /// retimeExposure on `first..=last`: the keys inside the range, in order, move to `first + i * spacing`; every cell
    /// of `first..=max(last, end)` is wiped first (keys, in-betweens and holds alike), as the dense loop does. `None`
    /// when the range holds no key (the layer is skipped). The plan's pairs are the key moves.
    pub(crate) fn retime_keys(
        &self,
        first: u32,
        last: u32,
        spacing: u32,
        total: u32,
        grown: u32,
    ) -> Result<Option<(CellPlan, u32)>, LayerTimingError> {
        self.validate(total)?;
        let olds: Vec<u32> = self
            .keys
            .iter()
            .map(|key| key.frame)
            .filter(|f| (first..=last).contains(f))
            .collect();
        if olds.is_empty() {
            return Ok(None);
        }
        let end = u64::from(first) + (olds.len() as u64 - 1) * u64::from(spacing);
        if last >= total || end >= u64::from(grown) {
            return Err(LayerTimingError::Invalid);
        }
        let wipe_end = last.max(end as u32);
        let wiped = |frame: u32| frame >= first && frame <= wipe_end;
        let mut keys: Vec<TimingKey> = self
            .keys
            .iter()
            .filter(|key| !wiped(key.frame))
            .cloned()
            .collect();
        let samples: Vec<TimingSample> = self
            .samples
            .iter()
            .filter(|sample| !wiped(sample.frame))
            .cloned()
            .collect();
        let moves: Vec<(u32, u32)> = olds
            .iter()
            .enumerate()
            .map(|(i, old)| (*old, first + i as u32 * spacing))
            .collect();
        keys.extend(moves.iter().map(|(_, new)| TimingKey { frame: *new }));
        Ok(Some((
            (self.finish_plan(keys, samples, grown)?, moves),
            wipe_end,
        )))
    }
}
