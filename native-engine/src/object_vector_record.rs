//! L01-D (G14): storage DTO for the `editable-vector-path` record body. Pure data and strict wire
//! shape; admission rules live in object_vector_codec.rs and the single native owner stays the
//! staged object document (this body is one more member of `ObjectRecord`, not a second document).
//!
//! The body carries what the baseline `serP`/`desP` pair (app.js:609/846) keeps beside the cubic
//! geometry: authored stroke paint with alpha, `hasRealStroke`, width/cap/join/miter limit, dash
//! array and offset, paint order and opacity. Fill is the record's top-level nullable member.
//! Absent, `null` and explicit values stay distinct through `Presence`; nothing is defaulted here.
use crate::object_document::{object_deserialize, Presence, SolidFill};
use serde::{Deserialize, Serialize};
use serde_json::Number;

/// Family tag of records that carry open or closed cubic geometry plus nullable paint.
pub(crate) const VECTOR_FAMILY: &str = "editable-vector-path";

/// Stroke and paint-order members of one `editable-vector-path` record.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VectorBody {
    /// Explicit `null` means no authored stroke; absent is refused by admission.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) stroke: Presence<Option<SolidFill>>,
    /// Authoritative flag (baseline `desP` restores no stroke when it is false); never inferred.
    pub(crate) has_real_stroke: bool,
    /// Stored as authored. The baseline importer rule `width || 3` is not a storage rule.
    pub(crate) stroke_width: Number,
    pub(crate) stroke_cap: String,
    pub(crate) stroke_join: String,
    /// Absent differs from an explicit value, including an explicit zero.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) miter_limit: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) dash_array: Presence<Vec<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) dash_offset: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) paint_order: Presence<String>,
    pub(crate) opacity: Number,
}

object_deserialize!(VectorBody {
    #[serde(default)] stroke: Presence<Option<SolidFill>>,
    has_real_stroke: bool,
    stroke_width: Number,
    stroke_cap: String,
    stroke_join: String,
    #[serde(default)] miter_limit: Presence<Number>,
    #[serde(default)] dash_array: Presence<Vec<Number>>,
    #[serde(default)] dash_offset: Presence<Number>,
    #[serde(default)] paint_order: Presence<String>,
    opacity: Number
});

impl VectorBody {
    /// The authored stroke paint, or `None` for an explicit no-stroke (or an absent slot).
    pub(crate) fn solid_stroke(&self) -> Option<&SolidFill> {
        self.stroke.value().and_then(Option::as_ref)
    }
    pub(crate) fn has_real_stroke(&self) -> bool {
        self.has_real_stroke
    }
}
