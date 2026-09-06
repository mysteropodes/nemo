//! Small MCP tool family over the running application's versioned API.
use crate::{
    contract::{ApplicationRequest, Operation},
    registry, wire,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::CallToolResult,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone)]
pub struct NemoServer {
    root: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
pub struct Query {
    /// Instance returned by nemo_discover. Required when several apps are running.
    pub instance_id: String,
    /// capabilities, snapshot, property.get or diagnostics.trace.
    pub operation: Operation,
    #[serde(default = "empty_payload")]
    pub payload: serde_json::Value,
}
fn empty_payload() -> serde_json::Value {
    json!({})
}

impl NemoServer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            tool_router: Self::tool_router(),
        }
    }

    async fn call(
        &self,
        request: ApplicationRequest,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let Some(instance) = request.instance_id.as_deref() else {
            return failure(
                "invalid_request",
                "Select an instance with nemo_discover first",
            );
        };
        let endpoints = match registry::read_endpoints(&self.root) {
            Ok(endpoints) => endpoints,
            Err(_) => return failure("unavailable", "Nemo discovery registry is unavailable"),
        };
        let Some(endpoint) = endpoints
            .into_iter()
            .find(|record| record.instance_id == instance)
        else {
            return failure(
                "unavailable",
                "Selected instance is absent; discover again after reconnect",
            );
        };
        match wire::call(&endpoint, request, context.ct).await {
            Ok(response) => {
                let ok = response.ok;
                let value =
                    serde_json::to_value(response).expect("serializable application response");
                if ok {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            Err(error) => failure("unavailable", &error.to_string()),
        }
    }
}

fn failure(code: &str, message: &str) -> CallToolResult {
    CallToolResult::structured_error(
        json!({"ok": false, "error": {"code": code, "message": message}}),
    )
}

#[tool_router]
impl NemoServer {
    #[tool(
        description = "Discover running Nemo instances and their build identity. Launch Nemo if none respond. No document is edited."
    )]
    async fn nemo_discover(&self, context: RequestContext<RoleServer>) -> CallToolResult {
        let endpoints = match registry::read_endpoints(&self.root) {
            Ok(endpoints) => endpoints,
            Err(_) => return failure("unavailable", "Nemo discovery registry is unavailable"),
        };
        let mut instances = Vec::new();
        for endpoint in endpoints {
            let request = ApplicationRequest {
                api_version: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
                instance_id: Some(endpoint.instance_id.clone()),
                document_id: None,
                expected_revision: None,
                operation: Operation::Capabilities,
                payload: json!({}),
            };
            if let Ok(Ok(response)) = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                wire::call(&endpoint, request, context.ct.child_token()),
            )
            .await
            {
                if response.ok {
                    instances.push(json!({"instanceId": endpoint.instance_id, "buildId": endpoint.build_id,
                        "documentId": response.document_id, "revision": response.revision, "capabilities": response.result}));
                }
            }
        }
        CallToolResult::structured(json!({"apiVersion": 1, "instances": instances}))
    }

    #[tool(
        description = "Read Nemo capabilities, current document snapshot, opacity property, or bounded diagnostics. Always select the instance from nemo_discover."
    )]
    async fn nemo_query(
        &self,
        Parameters(query): Parameters<Query>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        if !query.operation.is_query() {
            return failure("invalid_request", "Use nemo_command for mutations");
        }
        self.call(
            ApplicationRequest {
                api_version: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
                instance_id: Some(query.instance_id),
                document_id: None,
                expected_revision: None,
                operation: query.operation,
                payload: query.payload,
            },
            context,
        )
        .await
    }

    #[tool(
        description = "Call the same application command/history service as Nemo UI. Use the latest snapshot instanceId, documentId, expectedRevision and a unique requestId. Identical retries reuse their result; changed-body retries fail. Supports opacity editing/keying, undo/redo and diagnostic replay."
    )]
    async fn nemo_command(
        &self,
        Parameters(request): Parameters<ApplicationRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        if request.operation.is_query() {
            return failure("invalid_request", "Use nemo_query for reads");
        }
        if let Err(message) = request.validate() {
            return failure("invalid_request", message);
        }
        self.call(request, context).await
    }
}

#[tool_handler(router = self.tool_router, name = "nemo", version = "0.1.0", instructions = "Discover Nemo, select an instance, read snapshot, then use its identity and revision for commands. Nemo owns document state. After a cancelled or disconnected write query state before retrying; reuse the exact requestId and body for idempotent retries.")]
impl ServerHandler for NemoServer {}
