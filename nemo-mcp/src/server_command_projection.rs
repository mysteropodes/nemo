//! Transport-friendly root payload projection; versioned contracts retain authority.
use serde_json::{json, Value};

pub(super) fn payload(legacy: &Value, native_transport: &Value) -> Value {
    let mut payload = legacy["properties"]["payload"].clone();
    let payload_object = payload
        .as_object_mut()
        .expect("payload schema is an object");
    payload_object.remove("anyOf");
    let mut payload_properties = serde_json::Map::new();
    for branch in legacy["properties"]["payload"]["anyOf"].as_array().unwrap() {
        if let Some(fields) = branch["properties"].as_object() {
            for (key, field) in fields {
                payload_properties
                    .entry(key.clone())
                    .or_insert_with(|| field.clone());
            }
        }
    }
    // This root is a transport-friendly projection. The selected native
    // branch, not a second root-level feature switch, constrains commands.
    payload_properties.insert("command".into(), json!({"type": "string"}));
    // Legacy numeric values and native layer-policy booleans share this projection.
    // The selected version branch below still validates its exact value type.
    payload_properties.insert(
        "value".into(),
        json!({"type": ["number", "boolean", "null"]}),
    );
    payload_properties.insert(
        "stableTarget".into(),
        json!({
            "type": "object", "required": ["layerUid"],
            "properties": {"layerUid": {"type": "string", "minLength": 1}},
            "anyOf": [
                {"$ref": format!("{}#/$defs/StableTarget", native_transport["$id"].as_str().unwrap())},
                {"$ref": format!("{}#/$defs/ObjectTarget", native_transport["$id"].as_str().unwrap())}
            ]
        }),
    );
    payload_object.insert("properties".into(), payload_properties.into());
    payload
}
