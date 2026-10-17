//! Range and time-link shift kernel (F11): the pure integer half of what the dense editor does to
//! `inPoint`, `outPoint`, layer markers, composition markers, the work-area end and `timeLink`
//! when frames are inserted, removed or a layer is cut (src/js/app.js `insertFrame`,
//! `removeFrame`, `removeFrameSpan`; src/js/timeline.js `duplicateKeyframe`, `extendExposure`,
//! `splitLayerAtPlayhead`, all at fbe4b8a3).
//!
//! No document record exists for these fields yet, so the caller hands over plain values and gets
//! complete replacements back; nothing is mutated, so a refusal cannot leave a partial edit.
//! The baseline quirks are kept, each pinned by `tests/fixtures/object-frame-edit-baseline/ranges.txt`:
//! only F5 (`InsertFrame`) and single-frame removal shift ranges; span removal, extend exposure and
//! duplicate keyframe leave every range untouched (`shift_ranges`). F5 shifts only layers that
//! really received the slot (`LayerRole::Targeted`), removal shifts every layer, locked or not.
//! A value AT the cursor never moves. Absent, null and an explicit 0 stay three different values.
//! `timeLink` and its Motion offsets are untouched by insert and remove; a cut drops the link.

/// A frame-number slot as the dense editor stores it. `Absent` is an undefined property, `Null` an
/// explicit null, `At(0)` an explicit frame 0; none of them is ever turned into another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameBound {
    Absent,
    Null,
    At(i64),
}

/// The state of a layer's `timeLink` property: undefined, null (kept as is) or a link object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeLinkSlot {
    Absent,
    Null,
    Linked,
}

/// Every frame-number field of one layer that an insert, a removal or a cut can move. `markers` is
/// `None` when the layer has no marker array and `Some(empty)` when it has an empty one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerRanges {
    pub(crate) in_point: FrameBound,
    pub(crate) out_point: FrameBound,
    pub(crate) markers: Option<Vec<FrameBound>>,
    pub(crate) time_link: TimeLinkSlot,
}

/// Whether F5 really spliced a slot into the layer (`Targeted`) or only padded its length
/// (`Padded`: not selected, or locked). The caller decides targeting and lock policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerRole {
    Targeted,
    Padded,
}

/// The frame operations that touch ranges or the work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameEditKind {
    /// F5: a slot spliced in at `cursor + 1`.
    InsertFrame,
    /// Remove the frame at `cursor` on every layer.
    RemoveFrame,
    /// Remove `count` frames starting at `cursor`.
    RemoveFrameSpan { count: u32 },
    /// Extend exposure: `count` slots spliced in at `cursor + 1`.
    ExtendExposure { count: u32 },
    /// Duplicate keyframe: one slot spliced in at `cursor + 1`.
    DuplicateKeyframe,
}

impl FrameEditKind {
    /// True only for the operations that move layer and composition ranges in the baseline.
    pub(crate) const fn shift_ranges(self) -> bool {
        matches!(self, Self::InsertFrame | Self::RemoveFrame)
    }
}

/// Typed refusals; no partial result accompanies any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeEditError {
    /// `cursor` is not a frame of the `total`-frame timeline.
    CursorOutsideTimeline,
    /// A removed span runs past the last frame.
    SpanOutsideTimeline,
    /// A span or an extension of zero frames.
    EmptyEdit,
    /// The dense editor always keeps one frame; removing the last one is refused.
    WouldEmptyTimeline,
    /// A frame number left the integer range.
    Overflow,
}

/// One frame operation on a timeline of `total` frames, anchored at frame `cursor` (the current
/// frame; the first removed frame for a span).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RangeEdit {
    pub(crate) kind: FrameEditKind,
    pub(crate) cursor: u32,
    pub(crate) total: u32,
}

fn shift_bound(bound: FrameBound, cursor: i64, delta: i64) -> Result<FrameBound, RangeEditError> {
    match bound {
        FrameBound::At(frame) if frame > cursor => frame
            .checked_add(delta)
            .map(FrameBound::At)
            .ok_or(RangeEditError::Overflow),
        other => Ok(other),
    }
}

fn shift_markers(
    markers: &Option<Vec<FrameBound>>,
    cursor: i64,
    delta: i64,
) -> Result<Option<Vec<FrameBound>>, RangeEditError> {
    match markers {
        None => Ok(None),
        Some(list) => {
            let mut shifted = Vec::with_capacity(list.len());
            for marker in list {
                shifted.push(shift_bound(*marker, cursor, delta)?);
            }
            Ok(Some(shifted))
        }
    }
}

impl RangeEdit {
    /// The frame count after the operation, or the typed refusal. Every other method runs this
    /// first, so an invalid edit changes nothing anywhere.
    pub(crate) fn new_total(&self) -> Result<u32, RangeEditError> {
        if self.cursor >= self.total {
            return Err(RangeEditError::CursorOutsideTimeline);
        }
        match self.kind {
            FrameEditKind::InsertFrame | FrameEditKind::DuplicateKeyframe => {
                self.total.checked_add(1).ok_or(RangeEditError::Overflow)
            }
            FrameEditKind::RemoveFrame => {
                if self.total <= 1 {
                    Err(RangeEditError::WouldEmptyTimeline)
                } else {
                    Ok(self.total - 1)
                }
            }
            FrameEditKind::RemoveFrameSpan { count } => {
                if count == 0 {
                    return Err(RangeEditError::EmptyEdit);
                }
                let end = self
                    .cursor
                    .checked_add(count)
                    .ok_or(RangeEditError::Overflow)?;
                if end > self.total {
                    return Err(RangeEditError::SpanOutsideTimeline);
                }
                if count == self.total {
                    return Err(RangeEditError::WouldEmptyTimeline);
                }
                Ok(self.total - count)
            }
            FrameEditKind::ExtendExposure { count } => {
                if count == 0 {
                    return Err(RangeEditError::EmptyEdit);
                }
                self.total
                    .checked_add(count)
                    .ok_or(RangeEditError::Overflow)
            }
        }
    }

    /// The ranges of one layer after the operation. F5 shifts a `Targeted` layer by +1 and leaves a
    /// `Padded` one alone; single-frame removal shifts every layer by -1 whatever its role; the other
    /// operations return the input unchanged. Only values strictly greater than the cursor move.
    pub(crate) fn layer_ranges(
        &self,
        role: LayerRole,
        layer: &LayerRanges,
    ) -> Result<LayerRanges, RangeEditError> {
        self.new_total()?;
        let delta = match self.kind {
            FrameEditKind::InsertFrame if role == LayerRole::Targeted => 1,
            FrameEditKind::RemoveFrame => -1,
            _ => return Ok(layer.clone()),
        };
        let cursor = i64::from(self.cursor);
        Ok(LayerRanges {
            in_point: shift_bound(layer.in_point, cursor, delta)?,
            out_point: shift_bound(layer.out_point, cursor, delta)?,
            markers: shift_markers(&layer.markers, cursor, delta)?,
            time_link: layer.time_link,
        })
    }

    /// The composition markers (`state.markers`) after the operation: shifted like a targeted layer
    /// by F5 and by single-frame removal, unchanged otherwise.
    pub(crate) fn comp_markers(
        &self,
        markers: &Option<Vec<FrameBound>>,
    ) -> Result<Option<Vec<FrameBound>>, RangeEditError> {
        self.new_total()?;
        let delta = match self.kind {
            FrameEditKind::InsertFrame => 1,
            FrameEditKind::RemoveFrame => -1,
            _ => return Ok(markers.clone()),
        };
        shift_markers(markers, i64::from(self.cursor), delta)
    }

    /// The work-area end after the operation. F5 and duplicate keyframe add one unless it already
    /// is the last frame (whatever the cursor); removals pull it back to the new last frame only
    /// when it would fall outside; extend exposure raises it to the new last frame when below.
    pub(crate) fn work_area_out(&self, wa_out: i64) -> Result<i64, RangeEditError> {
        let new_total = i64::from(self.new_total()?);
        let last = new_total - 1;
        Ok(match self.kind {
            FrameEditKind::InsertFrame | FrameEditKind::DuplicateKeyframe => {
                if wa_out < last {
                    wa_out + 1
                } else {
                    wa_out
                }
            }
            FrameEditKind::RemoveFrame | FrameEditKind::RemoveFrameSpan { .. } => {
                if wa_out >= new_total {
                    last
                } else {
                    wa_out
                }
            }
            FrameEditKind::ExtendExposure { .. } => {
                if wa_out < last {
                    last
                } else {
                    wa_out
                }
            }
        })
    }
}

/// What a cut did to the layer's time link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeLinkOutcome {
    /// Absent or null: nothing to drop, the slot is kept exactly as it was.
    Unchanged,
    /// A live link was deleted from the original: the cut writes hard in/out values that a link
    /// would otherwise override.
    TimeLinkDropped,
}

/// Both halves of a cut layer and what happened to the link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitCut {
    pub(crate) original: LayerRanges,
    pub(crate) second: LayerRanges,
    pub(crate) time_link: TimeLinkOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SplitCutOutcome {
    /// The cut frame is at or before the resolved in point or after the resolved out point.
    OutsideLayer,
    Cut(SplitCut),
}

/// Split at the playhead: the original keeps its stored in point and ends at `cut - 1`; the second
/// layer runs `cut ..= effective_out` and never carries a link; markers are copied unchanged to both.
/// `effective_in` / `effective_out` are the RESOLVED edges (time link and folder clamps applied by the
/// caller); the stored in point of the original is left as it is, even when a dropped link was what
/// supplied it.
pub(crate) fn split_cut(
    cut: i64,
    effective_in: i64,
    effective_out: i64,
    original: &LayerRanges,
) -> SplitCutOutcome {
    if cut <= effective_in || cut > effective_out {
        return SplitCutOutcome::OutsideLayer;
    }
    let dropped = original.time_link == TimeLinkSlot::Linked;
    SplitCutOutcome::Cut(SplitCut {
        original: LayerRanges {
            in_point: original.in_point,
            out_point: FrameBound::At(cut - 1),
            markers: original.markers.clone(),
            time_link: if dropped {
                TimeLinkSlot::Absent
            } else {
                original.time_link
            },
        },
        second: LayerRanges {
            in_point: FrameBound::At(cut),
            out_point: FrameBound::At(effective_out),
            markers: original.markers.clone(),
            time_link: TimeLinkSlot::Absent,
        },
        time_link: if dropped {
            TimeLinkOutcome::TimeLinkDropped
        } else {
            TimeLinkOutcome::Unchanged
        },
    })
}

#[cfg(test)]
#[path = "../tests/object_frame_edit_ranges_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_frame_edit_ranges_literals.rs"]
mod literals;
#[cfg(test)]
#[path = "../tests/object_frame_edit_ranges.rs"]
mod tests;
