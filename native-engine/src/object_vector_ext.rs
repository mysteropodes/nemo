//! L01-E (G14/G22): the one extension slot of an `editable-vector-path` record. It carries the pressure
//! fields (object_vector_pressure.rs), the `paramShape` record (object_parametric_record.rs) and the
//! stroke-level `boxAngle` (baseline `serP`: the rotation of the shape's frame, maintained by every rotate
//! gesture for any stroke, so it is independent of `paramShape`). The member is absent for plain vectors
//! and for the legacy `closed-solid-fill-cubic-path` family, which refuses it (admission:
//! object_vector_ext_codec.rs).
use crate::object_document::{object_deserialize, ParamShape, Presence, PressureFields};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VectorExt {
    /// Present exactly for a vector-brush stroke (the baseline `isVectorBrush`).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pressure: Presence<PressureFields>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) param_shape: Presence<ParamShape>,
    /// Absent differs from an explicit 0 (the baseline serializer drops a falsy angle).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) box_angle: Presence<Number>,
}
object_deserialize!(VectorExt {
    #[serde(default)] pressure: Presence<PressureFields>,
    #[serde(default)] param_shape: Presence<ParamShape>,
    #[serde(default)] box_angle: Presence<Number>
});
