//! Sparse key-retime plan for `LayerTiming` (F10): the timing half of the dense editor's `moveKeyframe`
//! with its tween-span retime (`captureTweenInbetweens`, `retimeTweenSpans`), src/js/timeline.js at fbe4b8a3.
//!
//! One public function: it reads one validated timing and the frame count, and returns the complete replacement
//! timing (validated against the same count), `NoChange`, or a typed error. It also returns the CONTENT plan the
//! document half applies: `writes` lists, per written frame, which old frame's content the new cell carries
//! (`Some(source)`) or that the cell was emptied (`None`), and `pairs` lists every consecutive key pair with its
//! old and new frames, which is what the baseline hands the motion-arc owner (`rekeyTweenPairData`, not owned
//! here). The input is never mutated, so a refusal cannot leave a partial edit.
//!
//! Baseline arithmetic kept as it is: the dragged key and every later key move by the same delta; a positive
//! delta is clamped so the last key stays on the last frame; a destination replaces whatever the cell was (key,
//! sample or hold); a vacated key cell becomes a hold; an in-between span whose end keys moved is cleared across
//! its old and new extent and laid again by nearest normalized position (later write wins, never onto a key), and
//! a fully dense span fills the holes between the laid samples with copies of the nearest sample, manual flag
//! dropped. The one deliberate deviation: a dense fill that would exceed the 4096-entry timing cap is refused as
//! `Invalid` before it is built, where the dense editor has no cap.
use crate::object_document::timing::{
    LayerTiming, LayerTimingError, TimingKey, TimingSample, MAX_ENTRIES,
};
use crate::object_document::Presence;
use std::collections::{BTreeMap, BTreeSet};

/// A consecutive key pair of the OLD timing and where its two keys are afterwards (equal when it did not move).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RetimePair {
    pub(crate) from_a: u32,
    pub(crate) from_b: u32,
    pub(crate) to_a: u32,
    pub(crate) to_b: u32,
}

/// The complete outcome of a changed retime. `writes` is ascending by frame with one entry per written frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RetimePlan {
    pub(crate) timing: LayerTiming,
    /// The frame the dragged key landed on after the ripple clamp.
    pub(crate) applied_to: u32,
    /// (frame, Some(old frame whose content it now carries) | None for an emptied cell).
    pub(crate) writes: Vec<(u32, Option<u32>)>,
    pub(crate) pairs: Vec<RetimePair>,
}

/// `NoChange` means history must not publish and redo must survive.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RetimeEdit {
    Changed(RetimePlan),
    NoChange,
}

#[derive(Clone)]
enum Cell {
    Key,
    Sample(Presence<bool>),
}

/// One in-between span whose end keys moved: the captured samples and the span's old and new ends.
struct Span {
    from_a: u32,
    from_b: u32,
    to_a: u32,
    to_b: u32,
    list: Vec<(u32, Presence<bool>)>,
}

/// The working copy of one layer's keys and samples, plus the content writes made so far (a later write to a
/// frame replaces an earlier one, as a later assignment does in the dense editor).
struct Working {
    cells: BTreeMap<u32, Cell>,
    writes: BTreeMap<u32, Option<u32>>,
}
impl Working {
    fn of(timing: &LayerTiming) -> Self {
        let mut cells = BTreeMap::new();
        for key in &timing.keys {
            cells.insert(key.frame, Cell::Key);
        }
        for sample in &timing.samples {
            cells.insert(sample.frame, Cell::Sample(sample.manual_edit.clone()));
        }
        Self {
            cells,
            writes: BTreeMap::new(),
        }
    }
    /// Every destination becomes a key carrying its source's content, replacing whatever was there; a vacated
    /// source that no move lands on becomes a hold with no content.
    fn relocate(&mut self, moves: &[(u32, u32)]) {
        let destinations: BTreeSet<u32> = moves.iter().map(|(_, dest)| *dest).collect();
        for &(source, dest) in moves {
            self.cells.insert(dest, Cell::Key);
            self.writes.insert(dest, Some(source));
        }
        for &(source, _) in moves {
            if !destinations.contains(&source) {
                self.cells.remove(&source);
                self.writes.insert(source, None);
            }
        }
    }
    fn key_inside(&self, start: u32, end: u32) -> bool {
        self.cells
            .range((start + 1)..end)
            .any(|(_, cell)| matches!(cell, Cell::Key))
    }
    /// Phase 1: clear every sample across the span's old and new extent, before anything is laid.
    fn clear_stale(&mut self, span: &Span) {
        let low = span.from_a.min(span.to_a);
        let high = span.from_b.max(span.to_b);
        let stale: Vec<u32> = self
            .cells
            .range((low + 1)..high)
            .filter(|(_, cell)| matches!(cell, Cell::Sample(_)))
            .map(|(frame, _)| *frame)
            .collect();
        for frame in stale {
            self.cells.remove(&frame);
            self.writes.insert(frame, None);
        }
    }
    /// Phase 2: each captured sample lands at its normalized position in the new span, strictly inside it, never
    /// on a key and never outside the timeline; a later sample on the same frame wins.
    fn lay(&mut self, span: &Span, total: u32) {
        let old_len = f64::from(span.from_b - span.from_a);
        let new_len = f64::from(span.to_b - span.to_a);
        for (frame, manual) in &span.list {
            let at = f64::from(*frame - span.from_a) / old_len;
            let mut lay = (f64::from(span.to_a) + at * new_len).round() as i64;
            if lay <= i64::from(span.to_a) {
                lay = i64::from(span.to_a) + 1;
            }
            if lay >= i64::from(span.to_b) {
                lay = i64::from(span.to_b) - 1;
            }
            if lay < 0 || lay >= i64::from(total) {
                continue;
            }
            let lay = lay as u32;
            if matches!(self.cells.get(&lay), Some(Cell::Key)) {
                continue;
            }
            self.cells.insert(lay, Cell::Sample(manual.clone()));
            self.writes.insert(lay, Some(*frame));
        }
    }
    /// Phase 3, only for a fully dense span: every hole of the new span takes a copy of the nearest captured
    /// sample, manual flag dropped. A fill past the timing entry cap is refused before it is built.
    fn fill_holes(&mut self, span: &Span) -> Result<(), LayerTimingError> {
        if span.list.len() as u64 != u64::from(span.from_b - span.from_a - 1) {
            return Ok(());
        }
        if (span.to_b - span.to_a - 1) as usize > MAX_ENTRIES {
            return Err(LayerTimingError::Invalid);
        }
        let old_len = f64::from(span.from_b - span.from_a);
        let new_len = f64::from(span.to_b - span.to_a);
        for hole in (span.to_a + 1)..span.to_b {
            if self.cells.contains_key(&hole) {
                continue;
            }
            let want = f64::from(hole - span.to_a) / new_len;
            let mut best: Option<u32> = None;
            let mut best_distance = f64::INFINITY;
            for (frame, _) in &span.list {
                let distance = (f64::from(*frame - span.from_a) / old_len - want).abs();
                if distance < best_distance {
                    best_distance = distance;
                    best = Some(*frame);
                }
            }
            if let Some(frame) = best {
                self.cells.insert(hole, Cell::Sample(Presence::Missing));
                self.writes.insert(hole, Some(frame));
            }
        }
        Ok(())
    }
    /// The replacement timing (validated against `total`) and the ascending write list.
    fn finish(
        self,
        base: &LayerTiming,
        total: u32,
    ) -> Result<(LayerTiming, Vec<(u32, Option<u32>)>), LayerTimingError> {
        let mut next = base.clone();
        next.keys = Vec::new();
        next.samples = Vec::new();
        for (frame, cell) in &self.cells {
            match cell {
                Cell::Key => next.keys.push(TimingKey { frame: *frame }),
                Cell::Sample(manual) => next.samples.push(TimingSample {
                    frame: *frame,
                    manual_edit: manual.clone(),
                }),
            }
        }
        next.validate(total)?;
        Ok((next, self.writes.into_iter().collect()))
    }
}

impl LayerTiming {
    /// Move the key at `from` to `to` and ripple every later key by the same delta (see the module header).
    /// `from == to`, or a ripple clamp that leaves a zero move, is `NoChange`; a source that is not a key and
    /// any frame outside `0..total` refuse the edit as `Invalid`.
    pub(crate) fn retime_key(
        &self,
        from: u32,
        to: u32,
        total: u32,
    ) -> Result<RetimeEdit, LayerTimingError> {
        let Some(moves) = self.retime_moves(from, to, total)? else {
            return Ok(RetimeEdit::NoChange);
        };
        let mut work = Working::of(self);
        work.relocate(&moves);
        let (pairs, spans) = self.retime_spans(&moves, &work);
        for span in &spans {
            work.clear_stale(span);
        }
        for span in &spans {
            work.lay(span, total);
            work.fill_holes(span)?;
        }
        let (timing, writes) = work.finish(self, total)?;
        Ok(RetimeEdit::Changed(RetimePlan {
            timing,
            applied_to: moves[0].1,
            writes,
            pairs,
        }))
    }

    /// The dragged key's move first, then every later key's, or `None` when nothing moves.
    fn retime_moves(
        &self,
        from: u32,
        to: u32,
        total: u32,
    ) -> Result<Option<Vec<(u32, u32)>>, LayerTimingError> {
        self.validate(total)?;
        if from >= total || to >= total {
            return Err(LayerTimingError::Invalid);
        }
        if from == to {
            return Ok(None);
        }
        if self
            .keys
            .binary_search_by_key(&from, |key| key.frame)
            .is_err()
        {
            return Err(LayerTimingError::Invalid);
        }
        let after = self.keys.partition_point(|key| key.frame <= from);
        let later: Vec<u32> = self.keys[after..].iter().map(|key| key.frame).collect();
        let mut delta = i64::from(to) - i64::from(from);
        if delta > 0 {
            if let Some(last) = later.last() {
                let room = i64::from(total - 1) - i64::from(*last);
                if delta > room {
                    delta = room.max(0);
                }
            }
        }
        if delta == 0 {
            return Ok(None);
        }
        // Every destination lies inside 0..total: `to` is checked, a later key plus a negative delta stays
        // above `to`, and a later key plus a positive delta is clamped to the last frame.
        let shift = |frame: u32| -> u32 { (i64::from(frame) + delta) as u32 };
        let mut moves: Vec<(u32, u32)> = vec![(from, shift(from))];
        moves.extend(later.iter().map(|frame| (*frame, shift(*frame))));
        Ok(Some(moves))
    }

    /// Every consecutive pair of the OLD keys with its new frames, and the spans among them that must be retimed:
    /// samples between the old ends, ends that moved, a new span of at least one frame with no key strictly inside.
    fn retime_spans(&self, moves: &[(u32, u32)], work: &Working) -> (Vec<RetimePair>, Vec<Span>) {
        let moved: BTreeMap<u32, u32> = moves.iter().copied().collect();
        let mut pairs: Vec<RetimePair> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        for window in self.keys.windows(2) {
            let (from_a, from_b) = (window[0].frame, window[1].frame);
            let to_a = moved.get(&from_a).copied().unwrap_or(from_a);
            let to_b = moved.get(&from_b).copied().unwrap_or(from_b);
            pairs.push(RetimePair {
                from_a,
                from_b,
                to_a,
                to_b,
            });
            let low = self.samples.partition_point(|s| s.frame <= from_a);
            let high = self.samples.partition_point(|s| s.frame < from_b);
            let unmoved = to_a == from_a && to_b == from_b;
            if low >= high || unmoved || to_b <= to_a || work.key_inside(to_a, to_b) {
                continue;
            }
            spans.push(Span {
                from_a,
                from_b,
                to_a,
                to_b,
                list: self.samples[low..high]
                    .iter()
                    .map(|sample| (sample.frame, sample.manual_edit.clone()))
                    .collect(),
            });
        }
        (pairs, spans)
    }
}

#[cfg(test)]
#[path = "../tests/object_exposure_edit_retime_timing.rs"]
mod tests;
