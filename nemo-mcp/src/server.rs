//! Small MCP tool family over the running application's versioned API.
use crate::{
    capabilities,
    contract::{
        ApplicationRequest, NativeApplicationRequest, NativeHostStatus, NativeStatusRequest,
        Operation, NATIVE_API_VERSION,
    },
    registry, wire,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::CallToolResult,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de::Error as _, Deserialize, Deserializer};
use serde_json::{json, Value};
use std::{borrow::Cow, path::PathBuf};

#[derive(Clone)]
pub struct NemoServer {
    root: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Query {
    /// Instance returned by nemo_discover as `instances[].instanceId`. Required when
    /// several apps are running. `nemo_command` spells this field the same way.
    #[serde(alias = "instance_id")]
    pub instance_id: String,
    /// capabilities, snapshot, property.get or diagnostics.trace.
    pub operation: Operation,
    #[serde(default = "empty_payload")]
    #[schemars(schema_with = "crate::contract::query_payload_schema")]
    pub payload: serde_json::Value,
}
fn empty_payload() -> serde_json::Value {
    json!({})
}

/// One raw command tool with standards-valid v1 and v2 request branches.
/// `apiVersion` selects deserialization before overlapping operation names do.
#[derive(Clone, Debug)]
pub enum CommandRequest {
    Legacy(ApplicationRequest),
    Native(NativeApplicationRequest),
}

impl<'de> Deserialize<'de> for CommandRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value.get("apiVersion").and_then(Value::as_u64) {
            Some(1) => serde_json::from_value(value)
                .map(Self::Legacy)
                .map_err(D::Error::custom),
            Some(2) => serde_json::from_value(value)
                .map(Self::Native)
                .map_err(D::Error::custom),
            version => Err(D::Error::custom(format!(
                "unsupported apiVersion {}; expected 1 or 2",
                version.map_or_else(|| "missing".to_owned(), |value| value.to_string())
            ))),
        }
    }
}

impl JsonSchema for CommandRequest {
    fn schema_name() -> Cow<'static, str> {
        ApplicationRequest::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        Cow::Borrowed("nemo::CommandRequest:v2-extension")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        let mut legacy = schemars::schema_for!(ApplicationRequest).to_value();
        let legacy_object = legacy
            .as_object_mut()
            .expect("legacy command schema is an object");
        legacy_object.remove("title");
        legacy_object.insert("$id".into(), json!("urn:nemo:mcp:command-request:v1"));
        legacy_object.insert(
            "allOf".into(),
            json!([{
                "type": "object",
                "required": ["apiVersion"],
                "properties": {"apiVersion": {"const": 1}}
            }]),
        );

        let native_transport: Value = serde_json::from_str(include_str!(
            "../../engineering/application/native-transport-v2.schema.json"
        ))
        .expect("native transport schema is valid JSON");
        let mut native = native_transport.clone();
        let native_object = native
            .as_object_mut()
            .expect("native transport schema is an object");
        native_object
            .remove("oneOf")
            .expect("native transport declares its request/response union");
        native_object.insert("$ref".into(), json!("#/$defs/Request"));

        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"legacy": legacy, "native": native},
            "oneOf": [
                {"$ref": "#/$defs/legacy"},
                {"$ref": "#/$defs/native"}
            ],
            "x-nemo-nativeApiVersion": NATIVE_API_VERSION,
            "x-nemo-nativeTransportV2": native_transport,
            "x-nemo-registeredNativeCapabilities": capabilities::native_catalog().descriptors()
        })
        .try_into()
        .expect("command union is a schema object")
    }
}

impl NemoServer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            tool_router: Self::tool_router(),
        }
    }

    async fn call_legacy(
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

    async fn call_native(
        &self,
        request: NativeApplicationRequest,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let endpoints = match registry::read_endpoints(&self.root) {
            Ok(endpoints) => endpoints,
            Err(_) => return failure("unavailable", "Nemo discovery registry is unavailable"),
        };
        let Some(endpoint) = endpoints
            .into_iter()
            .find(|record| record.instance_id == request.instance_id)
        else {
            return failure(
                "unavailable",
                "Selected instance is absent; discover again after reconnect",
            );
        };
        match wire::call_native(&endpoint, request, context.ct).await {
            Ok(response) => {
                let ok = response.ok;
                let value = serde_json::to_value(response).expect("serializable native response");
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
                    let status_request = NativeStatusRequest {
                        api_version: NATIVE_API_VERSION,
                        request_id: uuid::Uuid::new_v4().to_string(),
                        instance_id: endpoint.instance_id.clone(),
                    };
                    let advertises_native = response.result.as_ref().is_some_and(|result| {
                        result.get("nativeApiVersion").and_then(Value::as_u64)
                            == Some(NATIVE_API_VERSION.into())
                    });
                    let native_status = if advertises_native {
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            wire::native_status(
                                &endpoint,
                                status_request.clone(),
                                context.ct.child_token(),
                            ),
                        )
                        .await
                        {
                            Ok(Ok(status)) => status,
                            Ok(Err(error)) => NativeHostStatus::unavailable(
                                &status_request,
                                format!("native host status unavailable: {error}"),
                            ),
                            Err(_) => NativeHostStatus::unavailable(
                                &status_request,
                                "native host status timed out",
                            ),
                        }
                    } else {
                        NativeHostStatus::unavailable(
                            &status_request,
                            "endpoint does not advertise nativeApiVersion 2",
                        )
                    };
                    instances.push(
                        json!({"instanceId": endpoint.instance_id, "buildId": endpoint.build_id,
                        "documentId": response.document_id, "revision": response.revision,
                        "capabilities": response.result, "nativeHostStatus": native_status}),
                    );
                }
            }
        }
        // These embedded declarations are transport-owned contract metadata, not a
        // second application registry. They are present even when no desktop instance
        // is running, so a client can discover exact input/output contracts before it
        // chooses an instance and the live application result remains authoritative
        // for that instance's current state.
        CallToolResult::structured(json!({
            "apiVersion": 1,
            "registeredCapabilities": capabilities::catalog().descriptors(),
            "nativeApiVersion": NATIVE_API_VERSION,
            "registeredNativeCapabilities": capabilities::native_catalog().descriptors(),
            "instances": instances
        }))
    }

    #[tool(
        description = "Read Nemo capabilities, current document snapshot, a registered property capability, or bounded diagnostics. Always select the instance from nemo_discover. The payload schema carries one copyable template per operation; property.get needs {\"layerId\",\"property\"} (property names a registered capability id) and the other reads take {}."
    )]
    async fn nemo_query(
        &self,
        Parameters(query): Parameters<Query>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        if !query.operation.is_query() {
            return failure("invalid_request", "Use nemo_command for mutations");
        }
        let request = ApplicationRequest {
            api_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            instance_id: Some(query.instance_id),
            document_id: None,
            expected_revision: None,
            operation: query.operation,
            payload: query.payload,
        };
        // Report a rejected read with its typed code here; reaching the transport
        // would report every fault alike, as an unavailable instance.
        if let Err(error) = request.validate() {
            return failure(error.code(), error.message());
        }
        self.call_legacy(request, context).await
    }

    #[tool(
        description = "Call the same application command/history service as Nemo UI. Use the latest snapshot instanceId, documentId, expectedRevision and a unique requestId. Identical retries reuse their result; changed-body retries fail. Supports editing/keying a registered property capability, undo/redo and diagnostic replay. The payload schema carries one copyable template per operation: send it as a JSON object, not as a JSON-encoded string."
    )]
    async fn nemo_command(
        &self,
        Parameters(request): Parameters<CommandRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        match request {
            CommandRequest::Legacy(request) => {
                if request.operation.is_query() {
                    return failure("invalid_request", "Use nemo_query for reads");
                }
                if let Err(error) = request.validate() {
                    return failure(error.code(), error.message());
                }
                self.call_legacy(request, context).await
            }
            CommandRequest::Native(request) => {
                if let Err(error) = request.validate() {
                    return failure(error.code(), error.message());
                }
                self.call_native(request, context).await
            }
        }
    }
}

#[tool_handler(router = self.tool_router, name = "nemo", version = "0.1.0", instructions = "Discover Nemo, select an instance, read snapshot, then use its identity and revision for commands. Nemo owns document state. Every payload is a JSON object shaped by the template its operation advertises in the tool schema. After a cancelled or disconnected write query state before retrying; reuse the exact requestId and body for idempotent retries.")]
impl ServerHandler for NemoServer {}
