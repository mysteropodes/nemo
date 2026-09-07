//! Versioned boundary shared by the SDK, desktop bridge, and MCP transport.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const API_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationRequest {
    pub api_version: u32,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    pub operation: Operation,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
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

impl Operation {
    pub fn is_query(&self) -> bool {
        matches!(
            self,
            Self::Capabilities | Self::Snapshot | Self::PropertyGet | Self::DiagnosticsTrace
        )
    }
}

impl ApplicationRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_version != API_VERSION {
            return Err("unsupported apiVersion");
        }
        if self.request_id.is_empty() || self.request_id.len() > 128 {
            return Err("requestId must contain 1..128 bytes");
        }
        if !self.payload.is_object() {
            return Err("payload must be an object");
        }
        if !self.operation.is_query()
            && (self.instance_id.as_deref().is_none_or(str::is_empty)
                || self.document_id.as_deref().is_none_or(str::is_empty)
                || self.expected_revision.is_none())
        {
            return Err("writes require instanceId, documentId and expectedRevision from snapshot");
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
