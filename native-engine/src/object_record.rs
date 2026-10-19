//! Object record DTOs of the staged native object document (target, frame scope, geometry, solid fill and the
//! record itself), moved verbatim out of object_document.rs (#2106); the parent re-exports every public name.
use crate::object_document::{object_deserialize, LayerEffect, Presence, VectorBody, VectorExt};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectTarget {
    pub(crate) context_id: String,
    pub(crate) frame_scope: FrameScope,
    pub(crate) layer_uid: String,
    pub(crate) stroke_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrameScope {
    pub(crate) kind: FrameScopeKind,
    pub(crate) frame: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameScopeKind {
    Authored,
    Reference,
}

impl<'de> Deserialize<'de> for FrameScopeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Derived enum decoding also admits externally tagged object variants.
        // The frozen contract permits only these two literal JSON strings.
        match String::deserialize(deserializer)?.as_str() {
            "authored" => Ok(Self::Authored),
            "reference" => Ok(Self::Reference),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["authored", "reference"],
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectRecord {
    pub(crate) schema_version: u32,
    pub(crate) family: String,
    pub(crate) target: ObjectTarget,
    pub(crate) geometry: ObjectGeometry,
    /// Legacy family: a required solid fill. Vector family: explicit null (no fill) or solid.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) fill: Presence<Option<SolidFill>>,
    /// Present exactly for the `editable-vector-path` family.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) vector: Presence<VectorBody>,
    /// Pressure fields, paramShape and boxAngle of the `editable-vector-path` family (L01-E).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) vector_ext: Presence<VectorExt>,
    /// Original stroke data.effects, shared by storage/copy/retime; never a rendering cache.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) effects: Presence<Option<Vec<LayerEffect>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectGeometry {
    pub(crate) kind: String,
    pub(crate) closed: bool,
    pub(crate) coordinate_space: String,
    pub(crate) handle_space: String,
    pub(crate) segments: Vec<ObjectSegment>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectSegment {
    pub(crate) point: ObjectPoint,
    pub(crate) handle_in: ObjectPoint,
    pub(crate) handle_out: ObjectPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectPoint {
    pub(crate) x: Number,
    pub(crate) y: Number,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SolidFill {
    pub(crate) kind: String,
    pub(crate) r: Number,
    pub(crate) g: Number,
    pub(crate) b: Number,
    pub(crate) a: Number,
}

object_deserialize!(ObjectTarget {
    context_id: String,
    frame_scope: FrameScope,
    layer_uid: String,
    stroke_id: String
});
object_deserialize!(FrameScope {
    kind: FrameScopeKind,
    frame: u32
});
object_deserialize!(ObjectRecord {
    schema_version: u32,
    family: String,
    target: ObjectTarget,
    geometry: ObjectGeometry,
    #[serde(default)] fill: Presence<Option<SolidFill>>,
    #[serde(default)] vector: Presence<VectorBody>,
    #[serde(default)] vector_ext: Presence<VectorExt>,
    #[serde(default)] effects: Presence<Option<Vec<LayerEffect>>>
});
object_deserialize!(ObjectGeometry { kind: String, closed: bool, coordinate_space: String,
    handle_space: String, segments: Vec<ObjectSegment> });
object_deserialize!(ObjectSegment {
    point: ObjectPoint,
    handle_in: ObjectPoint,
    handle_out: ObjectPoint
});
object_deserialize!(ObjectPoint {
    x: Number,
    y: Number
});
object_deserialize!(SolidFill {
    kind: String,
    r: Number,
    g: Number,
    b: Number,
    a: Number
});

impl ObjectRecord {
    pub(crate) fn effects(&self) -> Option<&[LayerEffect]> {
        self.effects.value().and_then(Option::as_deref)
    }
    pub(crate) fn has_enabled_effects(&self) -> bool {
        self.effects()
            .is_some_and(|stack| stack.iter().any(|effect| effect.enabled))
    }

    pub fn target(&self) -> &ObjectTarget {
        &self.target
    }
    /// The solid fill; `None` for an explicit null (no fill) or an absent slot.
    pub(crate) fn fill_gradient(&self) -> Option<&crate::object_document::StoredGradient> {
        self.vector_ext
            .value()
            .and_then(|ext| ext.fill_gradient.value())
    }
    pub(crate) fn solid_fill(&self) -> Option<&SolidFill> {
        self.fill.value().and_then(Option::as_ref)
    }
    /// The vector body of an `editable-vector-path` record.
    #[allow(dead_code)]
    pub(crate) fn vector(&self) -> Option<&VectorBody> {
        self.vector.value()
    }
    /// A pressure ribbon (the baseline `isVectorBrush`): its `vectorExt` carries the pressure fields. The
    /// centreline and width profile are stored beside the outline in `geometry.segments`, so an edit that
    /// moves only the outline leaves them stale; L01-G (`object_parametric_edit.rs`) refuses it by the same
    /// test (`vectorExt.pressure` present), and so do the rig writers.
    #[allow(dead_code)]
    pub(crate) fn is_pressure_ribbon(&self) -> bool {
        self.vector_ext
            .value()
            .is_some_and(|ext| ext.pressure.value().is_some())
    }
}
impl SolidFill {
    pub(crate) fn valid_channels(&self) -> bool {
        [&self.r, &self.g, &self.b, &self.a].into_iter().all(|n| {
            n.as_f64()
                .is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        })
    }
}
impl ObjectTarget {
    pub fn context_id(&self) -> &str {
        &self.context_id
    }
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub fn stroke_id(&self) -> &str {
        &self.stroke_id
    }
    pub fn frame(&self) -> u32 {
        self.frame_scope.frame
    }
    pub fn scope_kind(&self) -> &FrameScopeKind {
        &self.frame_scope.kind
    }
    pub(crate) fn valid(&self) -> bool {
        self.context_id == "scene-root" && !self.layer_uid.is_empty() && !self.stroke_id.is_empty()
    }
}
