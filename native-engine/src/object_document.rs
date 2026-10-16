//! Staged N25A records; distinct from the active opacity project family.
use serde::{Deserialize, Serialize};
use serde_json::Number;

pub const OBJECT_DOCUMENT_FORMAT: &str = "nemo.native-object-document";
pub const OBJECT_DOCUMENT_FORMAT_VERSION: u32 = 1;

#[path = "object_order_document.rs"]
mod order;
pub use order::OrderedObjectDocument;
pub(crate) use order::*;

#[path = "object_properties.rs"]
mod properties;
pub(crate) use properties::{validate_property_holders, PropertyAdmissionError};
pub use properties::{
    ExpressionControl, PropertyHolder, PropertyHolderTarget, PropertyMetadata, PropertyRecord,
};
#[cfg(test)]
#[path = "../tests/object_properties.rs"]
mod property_tests;

#[path = "project_asset_inventory.rs"]
mod assets;
pub(crate) use assets::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetSource, AssetTarget,
};

#[path = "project_media_envelope.rs"]
mod project;
pub(crate) use project::{
    ProjectEnvelope, ProjectMetadata, ProjectMetadataError, ProjectReferenceState,
};

#[path = "object_layer_timing.rs"]
mod timing;
pub(crate) use timing::{LayerTiming, LayerTimingError};
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_timing.rs"]
mod timing_tests;
// L09-A (#2126): private lane-9 media timing aggregator; its kernels are private children there.
#[path = "object_media_timing_core.rs"]
#[allow(dead_code)]
pub(crate) mod media_timing;
// L02-A (#2117): lane-2 group/boolean/clipboard/lifecycle family aggregator.
#[path = "object_group_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod object_group_kernels;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectDocument {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) total_frames: u32,
    pub(crate) layers: Vec<ObjectLayer>,
    pub(crate) objects: Vec<ObjectRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) composition_order: Presence<OrderPlan>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectLayer {
    pub(crate) layer_uid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) visible: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) locked: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) color: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) parent_layer_uid: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) timing: Presence<LayerTiming>,
}

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
    pub(crate) fill: SolidFill,
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

// Serde's derived struct decoder also accepts positional sequences. These
// schema objects must enter through visit_map; Fields retains strict duplicate
// and unknown-member checks while nested DTOs apply the same object-only rule.
macro_rules! object_deserialize {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct ObjectVisitor;
                impl<'de> serde::de::Visitor<'de> for ObjectVisitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("a JSON object")
                    }
                    fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<$name, A::Error> {
                        #[derive(serde::Deserialize)]
                        #[serde(deny_unknown_fields, rename_all = "camelCase")]
                        struct Fields { $($(#[$attr])* $field: $ty),* }
                        let fields = Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                        Ok($name { $($field: fields.$field),* })
                    }
                }
                deserializer.deserialize_map(ObjectVisitor)
            }
        }
    };
}
pub(crate) use object_deserialize;

object_deserialize!(ObjectDocument { format: String, format_version: u32, total_frames: u32,
    layers: Vec<ObjectLayer>, objects: Vec<ObjectRecord>,
    #[serde(default)] property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(default)] composition_order: Presence<OrderPlan> });
object_deserialize!(ObjectLayer { layer_uid: String, #[serde(default)] name: Presence<String>,
    #[serde(default)] visible: Presence<bool>, #[serde(default)] locked: Presence<bool>,
    #[serde(default)] color: Presence<String>,
    #[serde(default)] parent_layer_uid: Presence<Option<String>>,
    #[serde(default)] timing: Presence<LayerTiming> });
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
    fill: SolidFill
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

impl ObjectDocument {
    /// Immutable authored holder metadata; absence is not an empty authored table.
    pub fn property_holders(&self) -> Option<&[PropertyHolder]> {
        self.property_holders.value().map(Vec::as_slice)
    }
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    pub(crate) fn valid_target(&self, target: &ObjectTarget) -> bool {
        if self.format_version == 2 {
            !target.context_id.is_empty()
                && !target.layer_uid.is_empty()
                && !target.stroke_id.is_empty()
        } else {
            target.valid()
        }
    }
    /// Storage accepts Reference records; v2 behavior is not designed yet.
    pub(crate) fn reference_unavailable(&self, target: &ObjectTarget) -> bool {
        self.format_version == 2 && target.frame_scope.kind == FrameScopeKind::Reference
    }
    pub fn total_frames(&self) -> u32 {
        self.total_frames
    }
    pub fn objects(&self) -> &[ObjectRecord] {
        &self.objects
    }
    pub fn layers(&self) -> &[ObjectLayer] {
        &self.layers
    }
}
impl ObjectRecord {
    pub fn target(&self) -> &ObjectTarget {
        &self.target
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
impl ObjectLayer {
    /// Native New and Add share the existing desktop creation defaults. The
    /// palette is selected by the number of layers before insertion, not revision.
    pub(crate) fn blank_drawing(layer_uid: String, name: String, existing_layers: usize) -> Self {
        const COLORS: [&str; 8] = [
            "#5FA875", "#B49BE6", "#F0A585", "#E88BAE", "#E7C74C", "#EE7C58", "#7EA0FA", "#5FD1C4",
        ];
        Self {
            layer_uid,
            name: Presence::Present(name),
            visible: Presence::Present(true),
            locked: Presence::Present(false),
            color: Presence::Present(COLORS[existing_layers % COLORS.len()].into()),
            parent_layer_uid: Presence::Present(None),
            timing: Presence::Present(LayerTiming::blank_drawing()),
        }
    }
    /// Identified, unnamed layer with earlier native-file visibility semantics.
    pub(crate) fn identified(layer_uid: String) -> Self {
        Self {
            layer_uid,
            name: Presence::Missing,
            visible: Presence::Missing,
            locked: Presence::Missing,
            color: Presence::Missing,
            parent_layer_uid: Presence::Missing,
            timing: Presence::Missing,
        }
    }
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    /// Native stored name, without inventing one for earlier unnamed files.
    pub fn name(&self) -> Option<&str> {
        self.name.value().map(String::as_str)
    }
    pub fn visible(&self) -> bool {
        self.visible.value().copied().unwrap_or(true)
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
