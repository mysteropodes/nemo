//! Strict, bounded wire payloads of the six lane-4 setup / convert operations
//! (L04-M). Object-only decoding (duplicate, unknown and positional members are
//! refused by `object_deserialize!`), a byte bound on the whole payload and the
//! text bounds every identity must meet before any snapshot is read.
use crate::object_document::{object_deserialize, Presence};
use serde::Deserialize;
use serde_json::Number;

/// The staged dispatcher already bounds a whole request at 4096 bytes.
pub(super) const MAX_PAYLOAD_BYTES: usize = 4096;
pub(super) const MAX_ID_BYTES: usize = 128;
pub(super) const MAX_LAYERS: usize = 256;

#[derive(Debug)]
pub(super) struct ConvertPayload {
    pub(super) command: String,
    pub(super) context_id: String,
    pub(super) layer_uids: Vec<String>,
}
object_deserialize!(ConvertPayload {
    command: String,
    context_id: String,
    layer_uids: Vec<String>
});

#[derive(Debug)]
pub(super) struct ExposePayload {
    pub(super) command: String,
    pub(super) symbol_id: String,
    pub(super) key: String,
    pub(super) target_stroke_id: String,
    pub(super) target_field: String,
    pub(super) label: String,
    pub(super) default_value: Presence<Number>,
}
object_deserialize!(ExposePayload {
    command: String,
    symbol_id: String,
    key: String,
    target_stroke_id: String,
    target_field: String,
    label: String,
    #[serde(default, rename = "default")] default_value: Presence<Number>
});

#[derive(Debug)]
pub(super) struct PlacePayload {
    pub(super) command: String,
    pub(super) symbol_id: String,
    pub(super) name: String,
}
object_deserialize!(PlacePayload {
    command: String,
    symbol_id: String,
    name: String
});

#[derive(Debug)]
pub(super) struct DuplicatorSetPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) enabled: bool,
    pub(super) seed: Presence<Number>,
}
object_deserialize!(DuplicatorSetPayload {
    command: String,
    layer_uid: String,
    enabled: bool,
    #[serde(default)] seed: Presence<Number>
});

#[derive(Debug)]
pub(super) struct DuplicatorObjectsPayload {
    pub(super) command: String,
    pub(super) layer_uids: Vec<String>,
    pub(super) seed: Number,
}
object_deserialize!(DuplicatorObjectsPayload {
    command: String,
    layer_uids: Vec<String>,
    seed: Number
});

#[derive(Debug)]
pub(super) struct DuplicatorPathPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) path_layer_uid: Option<String>,
}
object_deserialize!(DuplicatorPathPayload {
    command: String,
    layer_uid: String,
    path_layer_uid: Option<String>
});

#[derive(Debug)]
pub(super) struct StaggerPayload {
    pub(super) position: Presence<bool>,
    pub(super) rotation: Presence<bool>,
    pub(super) scale: Presence<bool>,
    pub(super) opacity: Presence<bool>,
    pub(super) hue: Presence<bool>,
}
object_deserialize!(StaggerPayload {
    #[serde(default)] position: Presence<bool>, #[serde(default)] rotation: Presence<bool>,
    #[serde(default)] scale: Presence<bool>, #[serde(default)] opacity: Presence<bool>,
    #[serde(default)] hue: Presence<bool>
});

#[derive(Debug)]
pub(super) struct TimeOffsetPayload {
    pub(super) enabled: Presence<bool>,
    pub(super) offset_frames: Presence<Number>,
    pub(super) direction: Presence<String>,
}
object_deserialize!(TimeOffsetPayload {
    #[serde(default)] enabled: Presence<bool>, #[serde(default)] offset_frames: Presence<Number>,
    #[serde(default)] direction: Presence<String>
});

/// `object.duplicator.config`: the panel's closed partial edit, every member but the layer optional.
#[derive(Debug)]
pub(super) struct DuplicatorConfigPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) mode: Presence<String>,
    pub(super) rows: Presence<Number>,
    pub(super) cols: Presence<Number>,
    pub(super) spacing_x: Presence<Number>,
    pub(super) spacing_y: Presence<Number>,
    pub(super) count: Presence<Number>,
    pub(super) radius: Presence<Number>,
    pub(super) start_angle: Presence<Number>,
    pub(super) radial_orient: Presence<bool>,
    pub(super) path_align_tangent: Presence<bool>,
    pub(super) seed: Presence<Number>,
    pub(super) stagger_random: Presence<StaggerPayload>,
    pub(super) time_offset: Presence<TimeOffsetPayload>,
}
object_deserialize!(DuplicatorConfigPayload {
    command: String, layer_uid: String,
    #[serde(default)] mode: Presence<String>, #[serde(default)] rows: Presence<Number>,
    #[serde(default)] cols: Presence<Number>, #[serde(default)] spacing_x: Presence<Number>,
    #[serde(default)] spacing_y: Presence<Number>, #[serde(default)] count: Presence<Number>,
    #[serde(default)] radius: Presence<Number>, #[serde(default)] start_angle: Presence<Number>,
    #[serde(default)] radial_orient: Presence<bool>,
    #[serde(default)] path_align_tangent: Presence<bool>, #[serde(default)] seed: Presence<Number>,
    #[serde(default)] stagger_random: Presence<StaggerPayload>,
    #[serde(default)] time_offset: Presence<TimeOffsetPayload>
});

/// `object.component.instance.set`: the Component Instance panel's closed partial edit, every member but the layer optional.
#[derive(Debug)]
pub(super) struct InstanceSetPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) mode: Presence<String>,
    pub(super) single_frame: Presence<Number>,
    pub(super) placed_at: Presence<Number>,
    pub(super) speed: Presence<Number>,
    pub(super) at_frame: Presence<Number>,
}
object_deserialize!(InstanceSetPayload {
    command: String, layer_uid: String,
    #[serde(default)] mode: Presence<String>, #[serde(default)] single_frame: Presence<Number>,
    #[serde(default)] placed_at: Presence<Number>, #[serde(default)] speed: Presence<Number>,
    #[serde(default)] at_frame: Presence<Number>
});

/// An identity is non-empty, at most 128 bytes and carries no NUL.
pub(super) fn id_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}
pub(super) fn ids_ok(values: &[String]) -> bool {
    !values.is_empty() && values.len() <= MAX_LAYERS && values.iter().all(|v| id_ok(v))
}
/// The baseline seed is `Math.floor(Math.random() * 1e6)`: a JSON integer.
pub(super) fn seed_of(value: &Number) -> Option<u32> {
    value.as_u64().and_then(|seed| u32::try_from(seed).ok())
}
