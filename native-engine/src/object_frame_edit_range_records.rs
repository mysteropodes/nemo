//! F11 (L05-G): the stored time range of one layer, `ObjectLayer.range`: the baseline `ld.inPoint`,
//! `ld.outPoint`, `ld.markers` and `ld.timeLink` (src/js/app.js `layerInPoint`, `timeLinkSourceOf`;
//! src/js/markers.js at fbe4b8a3) as one strict storage record, plus its mapping to and from the pure
//! range kernel `object_frame_edit_ranges.rs` (L05-F), which this module owns as a private child so the
//! record, the kernel and the transactions that join them stay in the layers that may import each other.
//!
//! Pure data and strict wire shape. Admission is `object_frame_edit_range_codec.rs`, the join into the
//! frame-structure transaction is `object_frame_edit_range_join.rs`.
//!
//! Absent, `null` and explicit values stay distinct through `Presence`: a layer with no `range` member
//! never had any of the four properties; a present `range` lists exactly the properties that existed.
//! Inside it a missing `inPoint` is an undefined property, `inPoint: null` an explicit null and
//! `inPoint: 0` frame zero; `timeLink: null` keeps the baseline's falsy link; `markers: []` is an empty
//! array that differs from no array. Marker `name` and `color` are authored text kept verbatim (a marker
//! without them stays without them); a time link keeps `uid` and the optional `mode` / `srcAnchor`
//! texts. The legacy `inOffset` / `outOffset` numbers of an old link are Motion properties
//! (`timeLinkInOffset` / `timeLinkOutOffset`, `migrateTimeLinkOffsets`) and have no member here.
use crate::object_document::{object_deserialize, ObjectLayer, Presence};
use serde::{Deserialize, Serialize};

#[path = "object_frame_edit_ranges.rs"]
#[allow(dead_code)]
pub(crate) mod object_frame_edit_ranges;
use object_frame_edit_ranges::{split_cut, SplitCutOutcome, TimeLinkOutcome};
#[allow(unused_imports)]
pub(crate) use object_frame_edit_ranges::{
    FrameBound, FrameEditKind, LayerRanges, LayerRole, RangeEdit, RangeEditError, TimeLinkSlot,
};

/// One `{frame, name, color}` marker (`ld.markers[]` or `state.markers[]`, src/js/markers.js).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct MarkerRecord {
    pub(crate) frame: i64,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) color: Presence<String>,
}

/// `ld.timeLink = { uid, mode, srcAnchor }`. `mode` is `both`, `in` or `out` and `srcAnchor` `in` or
/// `out`; both stay absent when the baseline link had none (a missing mode reads as `both`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct TimeLinkRecord {
    pub(crate) uid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) mode: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) src_anchor: Presence<String>,
}

/// The four range properties of one layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LayerRange {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) in_point: Presence<Option<i64>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) out_point: Presence<Option<i64>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) time_link: Presence<Option<TimeLinkRecord>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) markers: Presence<Vec<MarkerRecord>>,
}

object_deserialize!(MarkerRecord {
    frame: i64,
    #[serde(default)] name: Presence<String>,
    #[serde(default)] color: Presence<String>
});
object_deserialize!(TimeLinkRecord {
    uid: String,
    #[serde(default)] mode: Presence<String>,
    #[serde(default)] src_anchor: Presence<String>
});
object_deserialize!(LayerRange {
    #[serde(default)] in_point: Presence<Option<i64>>,
    #[serde(default)] out_point: Presence<Option<i64>>,
    #[serde(default)] time_link: Presence<Option<TimeLinkRecord>>,
    #[serde(default)] markers: Presence<Vec<MarkerRecord>>
});

/// Why a kernel result cannot be written back onto a stored record. Each is a kernel / record
/// disagreement that admitted input cannot produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeRecordError {
    /// The kernel's marker list is not the stored list (presence or length differs).
    MarkerShape,
    /// A marker frame came back absent or null; a marker always has a frame.
    MarkerFrame,
    /// The kernel keeps a live link the stored record does not hold.
    TimeLinkShape,
}

/// The frames of a marker list as kernel bounds.
pub(crate) fn marker_bounds(markers: &[MarkerRecord]) -> Vec<FrameBound> {
    markers
        .iter()
        .map(|marker| FrameBound::At(marker.frame))
        .collect()
}

/// The list with the kernel's frames, every other authored field (name, color, order) kept.
pub(crate) fn markers_with_bounds(
    markers: &[MarkerRecord],
    bounds: &[FrameBound],
) -> Result<Vec<MarkerRecord>, RangeRecordError> {
    if markers.len() != bounds.len() {
        return Err(RangeRecordError::MarkerShape);
    }
    let mut next = Vec::with_capacity(markers.len());
    for (marker, bound) in markers.iter().zip(bounds) {
        let FrameBound::At(frame) = bound else {
            return Err(RangeRecordError::MarkerFrame);
        };
        next.push(MarkerRecord {
            frame: *frame,
            name: marker.name.clone(),
            color: marker.color.clone(),
        });
    }
    Ok(next)
}

fn bound_of(slot: &Presence<Option<i64>>) -> FrameBound {
    match slot {
        Presence::Missing => FrameBound::Absent,
        Presence::Present(None) => FrameBound::Null,
        Presence::Present(Some(frame)) => FrameBound::At(*frame),
    }
}

fn slot_of(bound: FrameBound) -> Presence<Option<i64>> {
    match bound {
        FrameBound::Absent => Presence::Missing,
        FrameBound::Null => Presence::Present(None),
        FrameBound::At(frame) => Presence::Present(Some(frame)),
    }
}

impl LayerRange {
    /// The kernel view of this record: absent, null and explicit values map to three different bounds.
    pub(crate) fn to_ranges(&self) -> LayerRanges {
        LayerRanges {
            in_point: bound_of(&self.in_point),
            out_point: bound_of(&self.out_point),
            markers: self.markers.value().map(|list| marker_bounds(list)),
            time_link: match &self.time_link {
                Presence::Missing => TimeLinkSlot::Absent,
                Presence::Present(None) => TimeLinkSlot::Null,
                Presence::Present(Some(_)) => TimeLinkSlot::Linked,
            },
        }
    }

    /// This record with the kernel's result written back: bounds and markers take the new frames, every
    /// authored text and the link object are kept, and a link the kernel drops is removed.
    pub(crate) fn with_ranges(&self, ranges: &LayerRanges) -> Result<LayerRange, RangeRecordError> {
        let markers = match (&self.markers, &ranges.markers) {
            (Presence::Missing, None) => Presence::Missing,
            (Presence::Present(stored), Some(bounds)) => {
                Presence::Present(markers_with_bounds(stored, bounds)?)
            }
            _ => return Err(RangeRecordError::MarkerShape),
        };
        let time_link = match ranges.time_link {
            TimeLinkSlot::Absent => Presence::Missing,
            TimeLinkSlot::Null => Presence::Present(None),
            TimeLinkSlot::Linked => match &self.time_link {
                Presence::Present(Some(link)) => Presence::Present(Some(link.clone())),
                _ => return Err(RangeRecordError::TimeLinkShape),
            },
        };
        Ok(LayerRange {
            in_point: slot_of(ranges.in_point),
            out_point: slot_of(ranges.out_point),
            time_link,
            markers,
        })
    }
}

impl ObjectLayer {
    /// The baseline `layerHasTimeRange` on the stored members: a non-null `inPoint` or `outPoint` (an explicit
    /// 0 counts) or a live `timeLink` decides on which frames the layer shows. Markers never do. A reader that
    /// does not evaluate the range yet must treat a layer for which this is true as unavailable.
    pub(crate) fn range_gates_frames(&self) -> bool {
        match self.range.value() {
            None => false,
            Some(range) => {
                matches!(range.in_point, Presence::Present(Some(_)))
                    || matches!(range.out_point, Presence::Present(Some(_)))
                    || matches!(range.time_link, Presence::Present(Some(_)))
            }
        }
    }
}

/// Both halves of a layer cut at the playhead, as records.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RangeSplit {
    /// The cut frame is at or before the resolved in point or after the resolved out point.
    OutsideLayer,
    Cut {
        original: LayerRange,
        second: LayerRange,
        /// A live link was deleted from the original (the cut writes hard in/out values).
        link_dropped: bool,
    },
}

/// `splitLayerAtPlayhead` on the range members: a layer with no `range` is the all-absent record. The
/// resolved edges (link and folder clamps applied) are the caller's, exactly as for the kernel.
pub(crate) fn split_layer_range(
    range: Option<&LayerRange>,
    cut: i64,
    effective_in: i64,
    effective_out: i64,
) -> Result<RangeSplit, RangeRecordError> {
    let empty = LayerRange::default();
    let base = range.unwrap_or(&empty);
    match split_cut(cut, effective_in, effective_out, &base.to_ranges()) {
        SplitCutOutcome::OutsideLayer => Ok(RangeSplit::OutsideLayer),
        SplitCutOutcome::Cut(halves) => Ok(RangeSplit::Cut {
            original: base.with_ranges(&halves.original)?,
            second: base.with_ranges(&halves.second)?,
            link_dropped: halves.time_link == TimeLinkOutcome::TimeLinkDropped,
        }),
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_edit_range_records.rs"]
mod object_frame_edit_range_records_tests;
