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
    /// Baseline serP fillGradient, kept in document-world coordinates and original stop order.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) fill_gradient: Presence<StoredGradient>,
}
object_deserialize!(VectorExt {
    #[serde(default)] pressure: Presence<PressureFields>,
    #[serde(default)] param_shape: Presence<ParamShape>,
    #[serde(default)] box_angle: Presence<Number>,
    #[serde(default)] fill_gradient: Presence<StoredGradient>
});

/// Named L07-I positive subset: exactly two stops, linear or radial. No coercion or defaults.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredGradient {
    pub(crate) kind: String,
    pub(crate) from: [Number; 2],
    pub(crate) to: [Number; 2],
    pub(crate) stops: [StoredGradientStop; 2],
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredGradientStop {
    pub(crate) offset: Number,
    pub(crate) color: String,
}
object_deserialize!(StoredGradient {
    kind: String,
    from: [Number; 2],
    to: [Number; 2],
    stops: [StoredGradientStop; 2]
});
object_deserialize!(StoredGradientStop {
    offset: Number,
    color: String
});
impl StoredGradient {
    pub(crate) fn valid(&self) -> bool {
        let finite = |n: &Number| {
            n.as_f64()
                .is_some_and(|v| v.is_finite() && (v as f32).is_finite())
        };
        ["linear", "radial"].contains(&self.kind.as_str())
            && self.from.iter().chain(&self.to).all(finite)
            && (((self.to[0].as_f64().unwrap() - self.from[0].as_f64().unwrap()).powi(2)
                + (self.to[1].as_f64().unwrap() - self.from[1].as_f64().unwrap()).powi(2))
            .sqrt() as f32)
                .is_finite()
            && self.stops.iter().all(|stop| {
                finite(&stop.offset)
                    && [4, 7, 9].contains(&stop.color.len())
                    && stop.color.starts_with('#')
                    && stop.color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            })
    }
}
