//! Versioned boundary shared by the SDK, desktop bridge, and MCP transport.
use crate::capabilities;
use crate::capability_contract;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[cfg(test)]
use crate::capability_contract::json_kind;

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
    // sent; `command_payload_schema` advertises the registered descriptor shapes
    // and `check_payload` projects their structural checks. A fixed Rust struct
    // would make every later property capability resemble opacity.
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

/// A validation failure, typed so a caller can distinguish a malformed request body
/// from an unknown or currently-unavailable capability instead of matching on prose
/// (P07/#1009 outcome check 3).
#[derive(Clone, Debug)]
pub enum RequestError {
    InvalidRequest(String),
    MalformedPayload(String),
    UnsupportedCapability(String),
    Unavailable(String),
}

impl RequestError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::MalformedPayload(_) => "malformed_payload",
            Self::UnsupportedCapability(_) => "unsupported_capability",
            Self::Unavailable(_) => "unavailable",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRequest(message)
            | Self::MalformedPayload(message)
            | Self::UnsupportedCapability(message)
            | Self::Unavailable(message) => message,
        }
    }
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message())
    }
}

impl std::error::Error for RequestError {}

impl Operation {
    pub const fn labels() -> &'static [&'static str] {
        &[
            "capabilities",
            "snapshot",
            "property.get",
            "property.set",
            "property.key.set",
            "property.key.remove",
            "property.animation.set",
            "history.undo",
            "history.redo",
            "diagnostics.trace",
            "diagnostics.replay",
        ]
    }

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

    /// Whether this operation names a lifecycle stage a property-style capability
    /// registers (engineering/application/capabilities/*.json), as opposed to a fixed
    /// transport primitive (capabilities/snapshot/history/diagnostics). A `payload`
    /// naming a subsequent property capability answers to these same five verbs, so a
    /// new capability never needs a new `Operation` variant.
    pub(crate) const fn is_property_operation(&self) -> bool {
        matches!(
            self,
            Self::PropertyGet
                | Self::PropertySet
                | Self::PropertyKeySet
                | Self::PropertyKeyRemove
                | Self::PropertyAnimationSet
        )
    }

    /// The fixed portion of what a payload must carry for this verb, independent of
    /// which capability `payload.property` names. A property operation's full
    /// requirement also includes that capability's own declared `input.required`
    /// (checked in `check_payload`, not representable here since it is per-capability
    /// and this list is `'static`).
    pub const fn required_payload_keys(&self) -> &'static [&'static str] {
        match self {
            Self::Capabilities
            | Self::Snapshot
            | Self::DiagnosticsTrace
            | Self::HistoryUndo
            | Self::HistoryRedo => &[],
            Self::PropertyGet => &[],
            Self::PropertySet => &["value"],
            Self::PropertyKeySet => &["value", "frame"],
            Self::PropertyKeyRemove => &["frame"],
            Self::PropertyAnimationSet => &["animated"],
            Self::DiagnosticsReplay => &["request"],
        }
    }

    /// A payload a client can copy, built from the actual registered capability's own
    /// fixture/examples rather than a literal Rust-side value, so a subsequent
    /// property capability is advertised correctly with no new code here.
    pub fn payload_example(&self) -> Value {
        match self {
            Self::Capabilities
            | Self::Snapshot
            | Self::DiagnosticsTrace
            | Self::HistoryUndo
            | Self::HistoryRedo => json!({}),
            Self::DiagnosticsReplay => json!({"request": {
                "operation": Self::PropertySet.label(),
                "payload": Self::PropertySet.payload_example(),
            }}),
            // Every remaining variant answers `is_property_operation`; a wildcard
            // here (rather than naming all five) avoids a structurally-unreachable
            // match arm that a coverage report would only ever see as dead.
            _ => self.property_payload_example_with(capabilities::catalog()),
        }
    }

    /// The first registered capability answering to this stage's own example inputs,
    /// with `property` set to that capability's id — a template built from the
    /// descriptor's reviewed fixture, not a hand-maintained literal. Takes an
    /// explicit catalog so a test can exercise the no-capability-registered
    /// fallback without a matching real descriptor.
    fn property_payload_example_with(&self, catalog: &capabilities::CapabilityCatalog) -> Value {
        let placeholder = || json!({"property": "<id of a registered property capability>"});
        let Some(capability) = catalog.first_supporting(self.label()) else {
            return placeholder();
        };
        let required = self.required_payload_keys();
        // A registered capability's fixture/examples need not individually cover
        // every verb it supports (opacity's do, but that is not guaranteed) — when
        // none does, fall back to the fixture anyway rather than the placeholder:
        // `example_inputs()` always yields at least it, so `.next()` here is
        // infallible for any descriptor capability-v1.schema.json actually accepts
        // (fixture is a required field), even though it may be missing this verb's
        // own keys.
        let chosen = capability
            .example_inputs()
            .find(|input| required.iter().all(|key| input.get(*key).is_some()))
            .unwrap_or_else(|| {
                capability
                    .example_inputs()
                    .next()
                    .expect("a registered descriptor always declares a fixture")
            });
        let mut example = chosen.clone();
        if let Some(object) = example.as_object_mut() {
            object.insert("property".to_string(), json!(capability.id));
        }
        example
    }

    /// Structural admission only — key presence, key spelling, JSON types and (for a
    /// property operation) that `payload.property` names a registered, available
    /// capability. Value ranges, frame bounds, layer existence and locking stay with
    /// the document owner, so this cannot drift into a second, disagreeing rule set.
    pub fn check_payload(&self, payload: &Value) -> Result<(), RequestError> {
        self.check_payload_with(payload, capabilities::catalog())
    }

    /// `check_payload`, against an explicit catalog — a test seam so an unavailable
    /// or stage-unsupported resolution outcome can be exercised without a matching
    /// real descriptor under `engineering/application/capabilities/`.
    pub(crate) fn check_payload_with(
        &self,
        payload: &Value,
        catalog: &capabilities::CapabilityCatalog,
    ) -> Result<(), RequestError> {
        let expected = format!("{} expects {}", self.label(), self.payload_example());
        capability_contract::validate_payload(self, payload, catalog, &expected)
    }
}

pub fn command_payload_schema(generator: &mut SchemaGenerator) -> Schema {
    capability_contract::payload_schema(generator, &WRITE_OPERATIONS)
}

pub fn query_payload_schema(generator: &mut SchemaGenerator) -> Schema {
    capability_contract::payload_schema(generator, &READ_OPERATIONS)
}

impl ApplicationRequest {
    pub fn validate(&self) -> Result<(), RequestError> {
        if self.api_version != API_VERSION {
            return Err(RequestError::InvalidRequest(format!(
                "unsupported apiVersion {}; this transport speaks apiVersion {API_VERSION}",
                self.api_version
            )));
        }
        if self.request_id.is_empty() || self.request_id.len() > 128 {
            return Err(RequestError::InvalidRequest(
                "requestId must contain 1..128 bytes".to_string(),
            ));
        }
        self.operation.check_payload(&self.payload)?;
        if !self.operation.is_query()
            && (self.instance_id.as_deref().is_none_or(str::is_empty)
                || self.document_id.as_deref().is_none_or(str::is_empty)
                || self.expected_revision.is_none())
        {
            return Err(RequestError::InvalidRequest(
                "writes require instanceId, documentId and expectedRevision from snapshot"
                    .to_string(),
            ));
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

#[cfg(test)]
#[path = "contract_tests.rs"]
mod tests;
