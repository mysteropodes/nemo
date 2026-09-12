//! Descriptor-derived MCP payload admission and JSON Schema projection.
//!
//! This is deliberately separate from `contract`: transport identities and response
//! types stay there, while this module consumes feature-owned descriptor shapes. A
//! later property descriptor therefore needs no new operation arm or Rust payload
//! struct merely to be admitted and advertised.
use crate::capabilities::{self, CapabilityCatalog, CapabilityDescriptor};
use crate::contract::{Operation, RequestError};
use schemars::{Schema, SchemaGenerator};
use serde_json::{json, Map, Value};

/// Validate a wire payload from the registered descriptor selected by `property`, or
/// from the fixed v1 transport vocabulary when the operation has no capability slot.
pub(crate) fn validate_payload(
    operation: &Operation,
    payload: &Value,
    catalog: &CapabilityCatalog,
    expected: &str,
) -> Result<(), RequestError> {
    if !payload.is_object() {
        return Err(RequestError::MalformedPayload(format!(
            "payload must be a JSON object, not {}; {expected}",
            json_kind(payload)
        )));
    }
    let object = payload.as_object().expect("object checked above");
    if object.contains_key("curvePoints") {
        return Err(RequestError::MalformedPayload(format!(
            "payload.curvePoints is not exposed by this capability; {expected}"
        )));
    }
    if !operation.is_property_operation() {
        return validate_fixed_payload(operation, object, catalog, expected);
    }
    let Some(property) = object
        .get("property")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        let supplied = object.keys().map(String::as_str).collect::<Vec<_>>();
        let suffix = if supplied.is_empty() {
            String::new()
        } else {
            format!("; supplied fields: {}", supplied.join(", "))
        };
        return Err(RequestError::MalformedPayload(format!(
            "payload is missing property{suffix}; {expected}"
        )));
    };
    let Some(capability) = catalog
        .find(property)
        .filter(|capability| capability.supports_stage(operation.label()))
    else {
        return Err(RequestError::UnsupportedCapability(format!(
            "No registered capability \"{property}\" answers to {}.",
            operation.label()
        )));
    };
    if !capability.is_available() {
        let reason = capability
            .availability
            .reason
            .as_deref()
            .unwrap_or("unspecified");
        return Err(RequestError::Unavailable(format!(
            "Capability \"{property}\" is unavailable ({reason})."
        )));
    }
    validate_capability_payload(operation, object, capability, expected)
}

fn validate_capability_payload(
    operation: &Operation,
    payload: &Map<String, Value>,
    capability: &CapabilityDescriptor,
    expected: &str,
) -> Result<(), RequestError> {
    let input = capability
        .input
        .as_object()
        .expect("registered descriptor input is an object");
    let properties = input
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut required = capability.declared_required();
    required.extend(
        operation
            .required_payload_keys()
            .iter()
            .map(|key| (*key).to_owned()),
    );
    required.sort();
    required.dedup();
    let missing: Vec<_> = required
        .iter()
        .filter(|key| !payload.contains_key(key.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(RequestError::MalformedPayload(format!(
            "payload is missing {}; {expected}",
            missing.join(", ")
        )));
    }
    for (key, value) in payload {
        if key == "property" {
            continue;
        }
        let Some(shape) = properties.get(key) else {
            if input.get("additionalProperties") == Some(&Value::Bool(false)) {
                return Err(RequestError::MalformedPayload(format!(
                    "payload contains unsupported field {key}; {expected}"
                )));
            }
            continue;
        };
        if !schema_accepts_json_type(shape, value) {
            return Err(RequestError::MalformedPayload(format!(
                "payload.{key} has type {}, but capability \"{}\" declares {}; {expected}",
                json_kind(value),
                capability.id,
                declared_types(shape)
            )));
        }
    }
    Ok(())
}

fn validate_fixed_payload(
    operation: &Operation,
    payload: &Map<String, Value>,
    catalog: &CapabilityCatalog,
    expected: &str,
) -> Result<(), RequestError> {
    if *operation != Operation::DiagnosticsReplay {
        return validate_legacy_fixed_fields(payload, expected);
    }
    if payload.len() != 1 || !payload.contains_key("request") {
        return Err(RequestError::MalformedPayload(format!(
            "diagnostics.replay expects only request; {expected}"
        )));
    }
    let request = payload["request"].as_object().ok_or_else(|| {
        RequestError::MalformedPayload(format!("payload.request must be an object; {expected}"))
    })?;
    if request.len() != 2 || !request.contains_key("operation") || !request.contains_key("payload")
    {
        return Err(RequestError::MalformedPayload(format!(
            "payload.request requires operation and payload only; {expected}"
        )));
    }
    let recorded: Operation =
        serde_json::from_value(request["operation"].clone()).map_err(|_| {
            RequestError::MalformedPayload(format!(
                "payload.request.operation is not a registered transport operation; {expected}"
            ))
        })?;
    recorded
        .check_payload_with(&request["payload"], catalog)
        .map_err(|error| {
            RequestError::MalformedPayload(format!(
                "recorded request payload is invalid: {}",
                error.message()
            ))
        })
}

fn validate_legacy_fixed_fields(
    payload: &Map<String, Value>,
    expected: &str,
) -> Result<(), RequestError> {
    for (key, value) in payload {
        let accepts = match key.as_str() {
            "layerId" | "property" => value.is_string() || value.is_null(),
            "value" => value.is_number() || value.is_null(),
            "frame" => value.as_i64().is_some() || value.as_u64().is_some() || value.is_null(),
            "animated" => value.is_boolean() || value.is_null(),
            "request" => value.is_object() || value.is_null(),
            _ => {
                return Err(RequestError::MalformedPayload(format!(
                    "payload contains unsupported field {key}; {expected}"
                )));
            }
        };
        if !accepts {
            return Err(RequestError::MalformedPayload(format!(
                "payload.{key} has type {}; {expected}",
                json_kind(value)
            )));
        }
    }
    Ok(())
}

fn schema_accepts_json_type(schema: &Value, value: &Value) -> bool {
    let Some(types) = schema.get("type") else {
        return true;
    };
    let matches = |kind: &str| match kind {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        _ => false,
    };
    match types {
        Value::String(kind) => matches(kind),
        Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).any(matches),
        _ => false,
    }
}

fn declared_types(schema: &Value) -> String {
    schema
        .get("type")
        .map(Value::to_string)
        .unwrap_or_else(|| "any JSON value".to_owned())
}

pub(crate) fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a JSON-encoded string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

pub(crate) fn payload_schema(generator: &mut SchemaGenerator, operations: &[Operation]) -> Schema {
    let _ = generator;
    let has_property_operation = operations.iter().any(Operation::is_property_operation);
    let mut branches = Vec::new();
    if operations.iter().any(|operation| {
        !operation.is_property_operation() && *operation != Operation::DiagnosticsReplay
    }) {
        branches.push(legacy_fixed_payload_schema());
    }
    if has_property_operation {
        branches.extend(
            capabilities::catalog()
                .descriptors()
                .iter()
                .filter(|capability| {
                    operations
                        .iter()
                        .any(|operation| capability.supports_stage(operation.label()))
                })
                .map(property_payload_schema),
        );
    }
    if operations.contains(&Operation::DiagnosticsReplay) {
        branches.push(replay_payload_schema());
    }
    serde_json::from_value(json!({
        "type": "object",
        "description": format!(
            "Operation body. Always a JSON object, never a JSON-encoded string. \
             Copy the template for the chosen operation:{}\n\
             `property` selects a registered capability. The matching branch projects its \
             exact input fields and JSON types; each full input/output descriptor is also \
             exposed in `x-nemo-registeredCapabilities` for discovery clients.",
            payload_table(operations)
        ),
        "examples": operations.iter().map(Operation::payload_example).collect::<Vec<_>>(),
        "anyOf": branches,
        "x-nemo-registeredCapabilities": capabilities::catalog().descriptors()
    }))
    .expect("descriptor-derived payload schema is valid JSON Schema")
}

fn payload_table(operations: &[Operation]) -> String {
    operations
        .iter()
        .fold(String::new(), |mut table, operation| {
            table.push_str(&format!(
                "\n  {:<22} {}",
                operation.label(),
                operation.payload_example()
            ));
            table
        })
}

fn property_payload_schema(capability: &CapabilityDescriptor) -> Value {
    let mut input = capability.input.clone();
    let object = input
        .as_object_mut()
        .expect("registered descriptor input is an object");
    let properties = object
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("registered descriptor properties is an object");
    properties.insert("property".to_owned(), json!({"const": capability.id}));
    let required = object
        .entry("required")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("registered descriptor required is an array");
    if !required.iter().any(|key| key == "property") {
        required.push(json!("property"));
    }
    input
}

fn replay_payload_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "request": {
                "type": "object",
                "additionalProperties": false,
                "required": ["operation", "payload"],
                "properties": {
                    "operation": {"enum": Operation::labels()},
                    "payload": true
                }
            }
        },
        "required": ["request"]
    })
}

fn legacy_fixed_payload_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "layerId": {"type": ["string", "null"]},
            "property": {"type": ["string", "null"]},
            "value": {"type": ["number", "null"]},
            "frame": {"type": ["integer", "null"]},
            "animated": {"type": ["boolean", "null"]},
            "request": {"type": ["object", "null"]}
        }
    })
}
