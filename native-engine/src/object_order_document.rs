//! Strict v2 storage DTOs. They carry no live owner, evaluator or painter state.
use crate::object_document::{
    object_deserialize, FrameScope, LayerFolders, ObjectDocument, ObjectLayer, ObjectRecord,
    PropertyHolder,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;

/// Missing differs from present null. T chooses where null is actually allowed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Presence<T> {
    Missing,
    Present(T),
}
impl<T> Default for Presence<T> {
    fn default() -> Self {
        Self::Missing
    }
}
impl<T> Presence<T> {
    pub(crate) fn missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
    pub(crate) fn value(&self) -> Option<&T> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value),
        }
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Presence<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Self::Present)
    }
}
impl<T: Serialize> Serialize for Presence<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Missing => s.serialize_unit(),
            Self::Present(value) => value.serialize(s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrderedObjectDocument {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) total_frames: u32,
    pub(crate) layers: Vec<ObjectLayer>,
    pub(crate) objects: Vec<ObjectRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) layer_folders: Presence<Option<LayerFolders>>,
    pub(crate) composition_order: OrderPlan,
}
object_deserialize!(OrderedObjectDocument {
    format: String, format_version: u32, total_frames: u32,
    layers: Vec<ObjectLayer>, objects: Vec<ObjectRecord>,
    #[serde(default)] property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(default)] layer_folders: Presence<Option<LayerFolders>>, composition_order: OrderPlan
});
impl OrderedObjectDocument {
    pub(crate) fn into_storage(self) -> ObjectDocument {
        ObjectDocument {
            format: self.format,
            format_version: self.format_version,
            total_frames: self.total_frames,
            layers: self.layers,
            objects: self.objects,
            property_holders: self.property_holders,
            layer_folders: self.layer_folders,
            composition_order: Presence::Present(self.composition_order),
        }
    }
    pub fn total_frames(&self) -> u32 {
        self.total_frames
    }
    pub fn layers(&self) -> &[ObjectLayer] {
        &self.layers
    }
    pub fn objects(&self) -> &[ObjectRecord] {
        &self.objects
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderPlan {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) document_format_version: u32,
    pub(crate) contexts: Vec<OrderContext>,
    pub(crate) migration: String,
    pub(crate) reference_policy: String,
}
object_deserialize!(OrderPlan {
    format: String, format_version: u32, document_format_version: u32,
    contexts: Vec<OrderContext>, migration: String, reference_policy: String
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderContext {
    pub(crate) context_id: String,
    pub(crate) layers: Vec<OrderLayer>,
    pub(crate) scopes: Vec<OrderScope>,
}
object_deserialize!(OrderContext { context_id: String, layers: Vec<OrderLayer>, scopes: Vec<OrderScope> });

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderLayer {
    pub(crate) layer_uid: String,
    pub(crate) natural_position: u32,
    pub(crate) order: AuthoredOrder,
}
object_deserialize!(OrderLayer {
    layer_uid: String,
    natural_position: u32,
    order: AuthoredOrder
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderScope {
    pub(crate) frame_scope: FrameScope,
    pub(crate) elements: Vec<OrderElement>,
}
object_deserialize!(OrderScope { frame_scope: FrameScope, elements: Vec<OrderElement> });

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderElement {
    pub(crate) layer_uid: String,
    pub(crate) stroke_id: String,
    pub(crate) natural_position: u32,
    pub(crate) order: AuthoredOrder,
}
object_deserialize!(OrderElement {
    layer_uid: String,
    stroke_id: String,
    natural_position: u32,
    order: AuthoredOrder
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthoredOrder {
    #[serde(rename = "static", skip_serializing_if = "Presence::missing")]
    pub(crate) static_value: Presence<Vec<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) track: Presence<OrderTrack>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) expression: Presence<OrderExpression>,
}
object_deserialize!(AuthoredOrder {
    #[serde(default, rename = "static")] static_value: Presence<Vec<Number>>,
    #[serde(default)] track: Presence<OrderTrack>,
    #[serde(default)] expression: Presence<OrderExpression>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderTrack {
    pub(crate) keys: Vec<OrderKey>,
}
object_deserialize!(OrderTrack { keys: Vec<OrderKey> });

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderKey {
    pub(crate) frame: Number,
    pub(crate) v: Vec<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) hold: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) curve_points: Presence<Option<Vec<OrderCurvePoint>>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) h_in: Presence<Vec<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) h_out: Presence<Vec<Number>>,
}
object_deserialize!(OrderKey {
    frame: Number, v: Vec<Number>,
    #[serde(default)] hold: Presence<bool>,
    #[serde(default)] curve_points: Presence<Option<Vec<OrderCurvePoint>>>,
    #[serde(default)] h_in: Presence<Vec<Number>>,
    #[serde(default)] h_out: Presence<Vec<Number>>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderCurvePoint {
    pub(crate) x: Number,
    pub(crate) y: Number,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) tx: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) ty: Presence<Number>,
}
object_deserialize!(OrderCurvePoint {
    x: Number, y: Number,
    #[serde(default)] tx: Presence<Number>,
    #[serde(default)] ty: Presence<Number>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrderExpression {
    pub(crate) enabled: bool,
    pub(crate) code: String,
}
object_deserialize!(OrderExpression {
    enabled: bool,
    code: String
});
