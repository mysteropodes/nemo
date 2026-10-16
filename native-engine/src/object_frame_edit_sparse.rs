//! Sparse frame-edit kernel for `LayerTiming` (F09/F10): the timing half of the dense editor's
//! insert-frame, remove-frame(s), insert-keyframe, convert-to-keyframes, clear-keyframe, delete-frames
//! and remove-tween operations (src/js/app.js, src/js/timeline.js at fbe4b8a3).
//!
//! Pure data transforms: each edit reads one validated timing and the document frame count that
//! applies to it, and returns either a complete replacement timing validated against the NEW frame
//! count, `NoChange` (nothing to publish), or a typed error. The input is never mutated, so a refusal
//! cannot leave a partial edit. Document totals, per-frame content, lock/shy policy, markers and
//! in/out points belong to the callers; this kernel owns only which frames carry keys and samples.
use crate::object_document::timing::{LayerTiming, LayerTimingError, TimingKey, MAX_ENTRIES};

/// Result of a sparse edit. `NoChange` means history must not publish and redo must survive.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TimingEdit {
    Changed(LayerTiming),
    NoChange,
}

/// The only exit for a changed plan: the whole timing invariant (ordering, bounds, key/sample
/// exclusion, entry cap) is re-run against the frame count the result belongs to.
fn finish(timing: LayerTiming, total: u32) -> Result<TimingEdit, LayerTimingError> {
    timing.validate(total)?;
    Ok(TimingEdit::Changed(timing))
}

impl LayerTiming {
    /// F5 on a targeted layer: a blank slot spliced in at `cf + 1`. Entries strictly after `cf`
    /// move one frame later; the new cell carries neither key nor sample, so it holds the previous
    /// key even when a tween span straddles it. `total` is the frame count BEFORE the insert.
    pub(crate) fn insert_gap(&self, cf: u32, total: u32) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        let grown = total.checked_add(1).ok_or(LayerTimingError::Invalid)?;
        if cf >= total {
            return Err(LayerTimingError::Invalid);
        }
        let mut next = self.clone();
        for key in &mut next.keys {
            if key.frame > cf {
                key.frame += 1;
            }
        }
        for sample in &mut next.samples {
            if sample.frame > cf {
                sample.frame += 1;
            }
        }
        finish(next, grown)
    }

    /// F5 on a non-targeted layer: one blank slot appended at the END, so every authored frame keeps
    /// its number. The timing is unchanged but must hold for the grown count.
    pub(crate) fn insert_pad(&self, total: u32) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        let grown = total.checked_add(1).ok_or(LayerTimingError::Invalid)?;
        finish(self.clone(), grown)
    }

    /// Remove frames `min..=max` on every layer. Entries inside vanish, later ones move down by the
    /// span length. Removing every frame is refused as `NoChange` (the dense editor keeps one).
    /// Dropping the only key leaves an explicit empty key list: the layer is unexposed, not absent.
    pub(crate) fn remove_span(
        &self,
        min: u32,
        max: u32,
        total: u32,
    ) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        if min > max || max >= total {
            return Err(LayerTimingError::Invalid);
        }
        let count = max - min + 1;
        if count >= total {
            return Ok(TimingEdit::NoChange);
        }
        let mut next = self.clone();
        next.keys.retain(|key| key.frame < min || key.frame > max);
        next.samples
            .retain(|sample| sample.frame < min || sample.frame > max);
        for key in &mut next.keys {
            if key.frame > max {
                key.frame -= count;
            }
        }
        for sample in &mut next.samples {
            if sample.frame > max {
                sample.frame -= count;
            }
        }
        finish(next, total - count)
    }

    /// F6 / convert-to-keyframes on one cell.
    pub(crate) fn promote_cell(
        &self,
        cell: u32,
        total: u32,
    ) -> Result<TimingEdit, LayerTimingError> {
        self.promote_cells(&[cell], total)
    }

    /// Make every listed cell a key. A held cell gains a key; a sample is replaced by the key (its
    /// manual-edit flag goes with it); a cell that already is a key is left alone. All-keys input is
    /// `NoChange`. Any cell outside `0..total` or any overflow of the entry cap refuses the whole edit.
    pub(crate) fn promote_cells(
        &self,
        cells: &[u32],
        total: u32,
    ) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        if cells.iter().any(|cell| *cell >= total) {
            return Err(LayerTimingError::Invalid);
        }
        let mut next = self.clone();
        let mut changed = false;
        for &cell in cells {
            if let Err(at) = next.keys.binary_search_by_key(&cell, |key| key.frame) {
                if let Ok(index) = next
                    .samples
                    .binary_search_by_key(&cell, |sample| sample.frame)
                {
                    next.samples.remove(index);
                }
                next.keys.insert(at, TimingKey { frame: cell });
                changed = true;
                if next.keys.len() + next.samples.len() > MAX_ENTRIES {
                    return Err(LayerTimingError::Invalid);
                }
            }
        }
        if !changed {
            return Ok(TimingEdit::NoChange);
        }
        finish(next, total)
    }

    /// Clear Keyframe: only a key is cleared; any other cell is `NoChange`. The cell then holds the
    /// previous key, or is unexposed when none precedes it.
    pub(crate) fn clear_key(&self, cell: u32, total: u32) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        if cell >= total {
            return Err(LayerTimingError::Invalid);
        }
        match self.keys.binary_search_by_key(&cell, |key| key.frame) {
            Ok(index) => {
                let mut next = self.clone();
                next.keys.remove(index);
                finish(next, total)
            }
            Err(_) => Ok(TimingEdit::NoChange),
        }
    }

    /// Delete Frames: each listed cell becomes a plain hold, whether it was a key or a sample.
    /// Cells that were already holds change nothing; nothing changed at all is `NoChange`.
    pub(crate) fn clear_cells(
        &self,
        cells: &[u32],
        total: u32,
    ) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        if cells.iter().any(|cell| *cell >= total) {
            return Err(LayerTimingError::Invalid);
        }
        let mut next = self.clone();
        for &cell in cells {
            next.keys.retain(|key| key.frame != cell);
            next.samples.retain(|sample| sample.frame != cell);
        }
        if next == *self {
            return Ok(TimingEdit::NoChange);
        }
        finish(next, total)
    }

    /// Remove Tween: everything STRICTLY between `from` and `to` becomes a plain hold of the start
    /// key. Both endpoints stay. Like the dense editor, a key sitting inside the span goes too.
    pub(crate) fn demote_span(
        &self,
        from: u32,
        to: u32,
        total: u32,
    ) -> Result<TimingEdit, LayerTimingError> {
        self.validate(total)?;
        if from > to || to >= total {
            return Err(LayerTimingError::Invalid);
        }
        let inside = |frame: u32| frame > from && frame < to;
        let mut next = self.clone();
        next.keys.retain(|key| !inside(key.frame));
        next.samples.retain(|sample| !inside(sample.frame));
        if next == *self {
            return Ok(TimingEdit::NoChange);
        }
        finish(next, total)
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_edit_sparse_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_frame_edit_sparse_literals.rs"]
mod literals;
#[cfg(test)]
#[path = "../tests/object_frame_edit_sparse.rs"]
mod tests;
