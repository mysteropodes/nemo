//! L01-E (G14): the pressure fields of an `editable-vector-path` record. A pressure ("vector brush") stroke
//! keeps its visible ribbon as the record's closed cubic geometry, and the editable centreline beside it:
//! `centerSegments` (point, handles and a per-anchor width), the arc-length `widthProfile`, the
//! `strokeProfile` intent and the `profileBase` width (baseline `serP`/`desP`, app.js:609/846; tools.js
//! `buildCenterSegmentsFromRawStroke`, `buildWidthProfile`, `applyStrokeProfileToPath`).
//!
//! Presence of this object is the baseline `isVectorBrush` flag. There is no scalar pressure member: the
//! baseline never stores one and nothing here synthesizes one. Absent, explicit and zero values stay
//! distinct (`profileBase` 0 is not an absent `profileBase`); nothing is clamped or defaulted.
use crate::object_document::{object_deserialize, ObjectPoint, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

/// Profile intents `strokeProfileWidthFn` (tools.js) knows. The baseline silently treats any other string
/// like `even`; the native record refuses it instead of reinterpreting it.
pub(crate) const STROKE_PROFILE_KINDS: [&str; 5] =
    ["taper-in", "taper-out", "taper-both", "bulge", "even"];

/// One anchor of the editable centreline; relative handles, as the geometry segments.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CenterSegment {
    pub(crate) point: ObjectPoint,
    pub(crate) handle_in: ObjectPoint,
    pub(crate) handle_out: ObjectPoint,
    pub(crate) width: Number,
}
object_deserialize!(CenterSegment {
    point: ObjectPoint,
    handle_in: ObjectPoint,
    handle_out: ObjectPoint,
    width: Number
});

/// One sample of the arc-length width curve (`t` is the fraction of the centreline length).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidthStop {
    pub(crate) t: Number,
    pub(crate) width: Number,
}
object_deserialize!(WidthStop {
    t: Number,
    width: Number
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PressureFields {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) center_segments: Presence<Vec<CenterSegment>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) width_profile: Presence<Vec<WidthStop>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) stroke_profile: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) profile_base: Presence<Number>,
}
object_deserialize!(PressureFields {
    #[serde(default)] center_segments: Presence<Vec<CenterSegment>>,
    #[serde(default)] width_profile: Presence<Vec<WidthStop>>,
    #[serde(default)] stroke_profile: Presence<String>,
    #[serde(default)] profile_base: Presence<Number>
});
