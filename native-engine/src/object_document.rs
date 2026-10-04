//! Staged N25A records; distinct from the active opacity project family.
use serde::{Deserialize, Serialize};
use serde_json::Number;

pub const OBJECT_DOCUMENT_FORMAT: &str = "nemo.native-object-document";
pub const OBJECT_DOCUMENT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectDocument {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) total_frames: u32,
    pub(crate) layers: Vec<ObjectLayer>,
    pub(crate) objects: Vec<ObjectRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectLayer {
    pub(crate) layer_uid: String,
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
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
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
                        struct Fields { $($field: $ty),* }
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
    layers: Vec<ObjectLayer>, objects: Vec<ObjectRecord> });
object_deserialize!(ObjectLayer { layer_uid: String });
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
impl ObjectLayer {
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
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
