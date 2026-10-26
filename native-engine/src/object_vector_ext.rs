//! L01-E (G14/G22): the one extension slot of an `editable-vector-path` record. It carries the pressure
//! fields (object_vector_pressure.rs), the `paramShape` record (object_parametric_record.rs) and the
//! stroke-level `boxAngle` (baseline `serP`: the rotation of the shape's frame, maintained by every rotate
//! gesture for any stroke, so it is independent of `paramShape`), plus the original per-Path mask tags.
//! The member is absent for plain vectors
//! and for the legacy `closed-solid-fill-cubic-path` family, which refuses it (admission:
//! object_vector_ext_codec.rs).
use crate::object_document::{object_deserialize, ParamShape, Presence, PressureFields};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[path = "object_fill_trace_record.rs"]
mod fill_trace;
pub(crate) use fill_trace::FillTrace;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VectorExt {
    /// Present exactly for a vector-brush stroke (the baseline `isVectorBrush`).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pressure: Presence<PressureFields>,
    /// Shared original link; the companion is a separate painted record.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) linked_fill_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_linked_fill_companion: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) param_shape: Presence<ParamShape>,
    /// Absent differs from an explicit 0 (the baseline serializer drops a falsy angle).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) box_angle: Presence<Number>,
    /// Baseline serP fillGradient, kept in document-world coordinates and original stop order.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) fill_gradient: Presence<StoredGradient>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) fill_trace: Presence<FillTrace>,
    /// Original Path.data tags, never a stored layer-level mask list.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_mask: Presence<Option<bool>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) mask_mode: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) mask_feather: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) bitmap_brush: Presence<crate::object_document::BitmapBrushFields>,
    /// G16: the brush-texture-preset tags of a vector anchor or one of its dabs.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) brush_texture: Presence<BrushTexture>,
}
object_deserialize!(VectorExt {
    #[serde(default)] pressure: Presence<PressureFields>,
    #[serde(default)] linked_fill_id: Presence<String>,
    #[serde(default)] is_linked_fill_companion: Presence<bool>,
    #[serde(default)] param_shape: Presence<ParamShape>,
    #[serde(default)] box_angle: Presence<Number>,
    #[serde(default)] fill_gradient: Presence<StoredGradient>,
    #[serde(default)] fill_trace: Presence<FillTrace>,
    #[serde(default)] is_mask: Presence<Option<bool>>,
    #[serde(default)] mask_mode: Presence<Option<String>>,
    #[serde(default)] mask_feather: Presence<Option<Number>>,
    #[serde(default)] bitmap_brush: Presence<crate::object_document::BitmapBrushFields>,
    #[serde(default)] brush_texture: Presence<BrushTexture>
});

impl VectorExt {
    pub(crate) fn is_empty(&self) -> bool {
        self.linked_fill_id.missing()
            && self.is_linked_fill_companion.missing()
            && self.pressure.missing()
            && self.param_shape.missing()
            && self.box_angle.missing()
            && self.fill_gradient.missing()
            && self.fill_trace.missing()
            && self.is_mask.missing()
            && self.mask_mode.missing()
            && self.mask_feather.missing()
            && self.bitmap_brush.missing()
            && self.brush_texture.missing()
    }
}

/// G16: the baseline `serP` brush-texture members, stored as serialized. An anchor (`applyBrushTexture`)
/// carries its preset key, group id and the pre-texture opacity or stroke it hid; each dab carries
/// `isBrushTextureCopy` and the same group id. serP drops falsy members, so a present string is
/// non-empty and a present copy flag is `true`; `preTextureStroke` may be an explicit null.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrushTexture {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) brush_group_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_brush_texture_copy: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) brush_texture_preset: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pre_texture_opacity: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pre_texture_stroke: Presence<Option<String>>,
    /// G16 option C: the anchor's stamping seed (`strokeSeed`, a uint32), from which native produces its dabs.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) stroke_seed: Presence<Number>,
}
object_deserialize!(BrushTexture {
    #[serde(default)] brush_group_id: Presence<String>,
    #[serde(default)] is_brush_texture_copy: Presence<bool>,
    #[serde(default)] brush_texture_preset: Presence<String>,
    #[serde(default)] pre_texture_opacity: Presence<Number>,
    #[serde(default)] pre_texture_stroke: Presence<Option<String>>,
    #[serde(default)] stroke_seed: Presence<Number>
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
