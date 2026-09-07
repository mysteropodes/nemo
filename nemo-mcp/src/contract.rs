//! Versioned boundary shared by the SDK, desktop bridge, and MCP transport.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const API_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationRequest {
    /// Always 1; this transport speaks no other version.
    pub api_version: u32,
    /// Caller-chosen identity, 1..128 bytes. Re-sending it with an identical body
    /// returns the retained result; re-sending it with a changed body is rejected.
    pub request_id: String,
    /// Instance identity from `nemo_discover` (`instances[].instanceId`). Required
    /// by every command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// Document identity from the latest `snapshot`. Required by every command; it
    /// changes whenever the open document is replaced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// The `revision` of the latest snapshot. Required by every command, and the
    /// write is refused as stale if the document has moved on since.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    /// The operation to run; its name selects the payload template below.
    pub operation: Operation,
    // Kept a free `Value` so the document owner receives exactly what the client
    // sent; `command_payload_schema` advertises the shape and `check_payload`
    // enforces it. Both are derived from `CommandPayload`, never hand-written.
    // Deliberately not a doc comment: schemars would overwrite the generated
    // description — the per-operation templates — with this prose.
    #[schemars(schema_with = "command_payload_schema")]
    pub payload: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub enum Operation {
    #[serde(rename = "capabilities")]
    Capabilities,
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "property.get")]
    PropertyGet,
    #[serde(rename = "property.set")]
    PropertySet,
    #[serde(rename = "property.key.set")]
    PropertyKeySet,
    #[serde(rename = "property.key.remove")]
    PropertyKeyRemove,
    #[serde(rename = "property.animation.set")]
    PropertyAnimationSet,
    #[serde(rename = "history.undo")]
    HistoryUndo,
    #[serde(rename = "history.redo")]
    HistoryRedo,
    #[serde(rename = "diagnostics.trace")]
    DiagnosticsTrace,
    #[serde(rename = "diagnostics.replay")]
    DiagnosticsReplay,
}

/// The reads `nemo_query` accepts, in the order a client meets them.
pub const READ_OPERATIONS: [Operation; 4] = [
    Operation::Capabilities,
    Operation::Snapshot,
    Operation::PropertyGet,
    Operation::DiagnosticsTrace,
];

/// The writes `nemo_command` accepts, in the order a client meets them.
pub const WRITE_OPERATIONS: [Operation; 7] = [
    Operation::PropertySet,
    Operation::PropertyKeySet,
    Operation::PropertyKeyRemove,
    Operation::PropertyAnimationSet,
    Operation::HistoryUndo,
    Operation::HistoryRedo,
    Operation::DiagnosticsReplay,
];

/// Every key a payload may carry. Which of them an operation requires is listed
/// with that operation's template; no other key is accepted.
// Optional here because requiredness is per operation:
// `Operation::required_payload_keys` carries that half, and `deny_unknown_fields`
// turns a misspelled key into an error naming the alternatives rather than a
// generic rejection from the document owner.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct CommandPayload {
    /// Layer identity, copied from `layers[].layerUid` of the latest snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    /// Property identity: the `id` of a `capabilities.properties` entry, currently
    /// only `"opacity"`. Note the key is `property`, while capabilities names it `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    /// New value, in the unit and range `capabilities` advertises for the property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Timeline frame; defaults to the document's current frame where it is optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<i64>,
    /// Whether the property is animated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animated: Option<bool>,
    /// One recorded property command, as `diagnostics.trace` reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<Box<RecordedCommand>>,
    /// Unavailable: curve editing is not exposed by this capability, and a payload
    /// carrying this key is rejected.
    // Declared rather than omitted so the rejection says that, instead of reporting
    // `curvePoints` as an unknown key. The document owner refuses it as well.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve_points: Option<Value>,
}

/// One recorded command, in the form `diagnostics.trace` reports it. An entry read
/// from a trace can be handed back unchanged; its extra fields are ignored.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordedCommand {
    /// `property.set`, `property.key.set`, `property.key.remove` or `property.animation.set`.
    pub operation: Operation,
    pub payload: CommandPayload,
}

impl CommandPayload {
    fn supplies(&self, key: &str) -> bool {
        match key {
            "layerId" => self.layer_id.is_some(),
            "property" => self.property.is_some(),
            "value" => self.value.is_some(),
            "frame" => self.frame.is_some(),
            "animated" => self.animated.is_some(),
            "request" => self.request.is_some(),
            _ => false,
        }
    }
}

impl Operation {
    pub fn is_query(&self) -> bool {
        READ_OPERATIONS.contains(self)
    }

    /// The wire name, so a diagnostic quotes what the client actually sent.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::Snapshot => "snapshot",
            Self::PropertyGet => "property.get",
            Self::PropertySet => "property.set",
            Self::PropertyKeySet => "property.key.set",
            Self::PropertyKeyRemove => "property.key.remove",
            Self::PropertyAnimationSet => "property.animation.set",
            Self::HistoryUndo => "history.undo",
            Self::HistoryRedo => "history.redo",
            Self::DiagnosticsTrace => "diagnostics.trace",
            Self::DiagnosticsReplay => "diagnostics.replay",
        }
    }

    pub const fn required_payload_keys(&self) -> &'static [&'static str] {
        match self {
            Self::Capabilities
            | Self::Snapshot
            | Self::DiagnosticsTrace
            | Self::HistoryUndo
            | Self::HistoryRedo => &[],
            Self::PropertyGet => &["layerId", "property"],
            Self::PropertySet => &["layerId", "property", "value"],
            Self::PropertyKeySet => &["layerId", "property", "value", "frame"],
            Self::PropertyKeyRemove => &["layerId", "property", "frame"],
            Self::PropertyAnimationSet => &["layerId", "property", "animated"],
            Self::DiagnosticsReplay => &["request"],
        }
    }

    /// A payload a client can copy. Placeholders name where the real value comes
    /// from, so the example reads as a template rather than a literal to send.
    pub fn payload_example(&self) -> Value {
        let layer = "<layers[].layerUid from snapshot>";
        match self {
            Self::Capabilities
            | Self::Snapshot
            | Self::DiagnosticsTrace
            | Self::HistoryUndo
            | Self::HistoryRedo => json!({}),
            Self::PropertyGet => json!({"layerId": layer, "property": "opacity"}),
            Self::PropertySet => json!({"layerId": layer, "property": "opacity", "value": 40}),
            Self::PropertyKeySet => {
                json!({"layerId": layer, "property": "opacity", "value": 40, "frame": 12})
            }
            Self::PropertyKeyRemove => {
                json!({"layerId": layer, "property": "opacity", "frame": 12})
            }
            Self::PropertyAnimationSet => {
                json!({"layerId": layer, "property": "opacity", "animated": true})
            }
            Self::DiagnosticsReplay => json!({"request": {"operation": "property.set",
                "payload": {"layerId": layer, "property": "opacity", "value": 40}}}),
        }
    }

    /// Structural admission only — key presence, key spelling and JSON types. Value
    /// ranges, frame bounds, layer existence and locking stay with the document
    /// owner, so this cannot drift into a second, disagreeing rule set.
    pub fn check_payload(&self, payload: &Value) -> Result<(), String> {
        let expected = format!("{} expects {}", self.label(), self.payload_example());
        if !payload.is_object() {
            return Err(format!(
                "payload must be a JSON object, not {}; {expected}",
                json_kind(payload)
            ));
        }
        let parsed: CommandPayload = serde_json::from_value(payload.clone())
            .map_err(|error| format!("payload is not usable: {error}; {expected}"))?;
        if parsed.curve_points.is_some() {
            return Err(format!(
                "payload.curvePoints is not exposed by this capability; {expected}"
            ));
        }
        let missing = self
            .required_payload_keys()
            .iter()
            .copied()
            .filter(|key| !parsed.supplies(key))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "payload is missing {}; {expected}",
                missing.join(", ")
            ));
        }
        Ok(())
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a JSON-encoded string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
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

fn payload_schema(generator: &mut SchemaGenerator, operations: &[Operation]) -> Schema {
    let mut schema = CommandPayload::json_schema(generator);
    schema.insert(
        "description".to_string(),
        json!(format!(
            "Operation body. Always a JSON object, never a JSON-encoded string. \
             Copy the template for the chosen operation:{}\n\
             `layerId` is `layers[].layerUid` from the latest snapshot. `property` is the \
             `id` of a `capabilities.properties` entry, currently only \"opacity\" — the \
             payload key is `property` even though capabilities names it `id`. `frame` \
             defaults to the document's current frame wherever the template omits it.",
            payload_table(operations)
        )),
    );
    schema.insert(
        "examples".to_string(),
        Value::Array(
            operations
                .iter()
                .map(Operation::payload_example)
                .collect::<Vec<_>>(),
        ),
    );
    schema
}

pub fn command_payload_schema(generator: &mut SchemaGenerator) -> Schema {
    payload_schema(generator, &WRITE_OPERATIONS)
}

pub fn query_payload_schema(generator: &mut SchemaGenerator) -> Schema {
    payload_schema(generator, &READ_OPERATIONS)
}

impl ApplicationRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != API_VERSION {
            return Err(format!(
                "unsupported apiVersion {}; this transport speaks apiVersion {API_VERSION}",
                self.api_version
            ));
        }
        if self.request_id.is_empty() || self.request_id.len() > 128 {
            return Err("requestId must contain 1..128 bytes".to_string());
        }
        self.operation.check_payload(&self.payload)?;
        if !self.operation.is_query()
            && (self.instance_id.as_deref().is_none_or(str::is_empty)
                || self.document_id.as_deref().is_none_or(str::is_empty)
                || self.expected_revision.is_none())
        {
            return Err(
                "writes require instanceId, documentId and expectedRevision from snapshot"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationResponse {
    pub api_version: u32,
    pub request_id: String,
    pub instance_id: String,
    pub document_id: String,
    pub revision: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApplicationError>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ApplicationError {
    pub code: String,
    pub message: String,
}
