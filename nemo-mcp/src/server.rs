//! Small MCP tool family over the running application's versioned API.
#[path = "server_command_projection.rs"]
mod command_projection;
use crate::{
    contract::{ApplicationRequest, NativeApplicationRequest, Operation, NATIVE_API_VERSION},
    discovery::{self, DiscoverArgs},
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
use tracing::{Instrument, Span};

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
        let native_metadata: Value = serde_json::from_str(include_str!(
            "../../engineering/application/native-property-metadata-v1.schema.json"
        ))
        .expect("native property metadata schema is valid JSON");
        let native_fonts: Value = serde_json::from_str(include_str!(
            "../../engineering/application/native-font-assets-v1.schema.json"
        ))
        .expect("native font assets schema is valid JSON");
        let native_items: Value = serde_json::from_str(include_str!(
            "../../engineering/application/native-object-item-v1.schema.json"
        ))
        .expect("native item schema is valid JSON");
        let mut native = native_transport.clone();
        let native_object = native
            .as_object_mut()
            .expect("native transport schema is an object");
        native_object
            .remove("oneOf")
            .expect("native transport declares its request/response union");
        native_object.insert("$ref".into(), json!("#/$defs/Request"));

        // Root unions project as unknown arguments in Codex. Keep a direct object
        // and select the exact version branch with JSON Schema conditionals.
        let mut properties = legacy["properties"].clone();
        properties["apiVersion"] = json!({"type": "integer", "enum": [1, 2]});
        properties["operation"] = json!({"type": "string", "description": "Native v2 operation shapes are selected by the versioned native request schema. Legacy v1 operations use their payload templates below."});
        properties["cancelledBeforeDispatch"] = json!({"type": "boolean"});
        properties["payload"] = command_projection::payload(&legacy, &native_transport);
        let examples = crate::discovery_contract::command_schema_examples();

        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": properties,
            "required": ["apiVersion", "requestId", "operation", "payload"],
            "description": "Use apiVersion 2 for native operations. Which operations apply depends on the open document's kind (documentKind in nemo_discover): call nemo_discover first, it lists the steps and a request to copy for that kind, and {operation: name} gives one operation's exact schema, example and availability. A refusal names the operation to use instead where there is one. The examples below are for an object document; replace instanceId, documentId, expectedRevision and layerUid with the discovered values. Reads (query.*) carry atRevision in the payload and never expectedRevision; commands carry the current expectedRevision, which changes after each command. Use a unique requestId for each new command; reuse the identical body only for retries. Legacy apiVersion 1 payload templates remain available below.",
            "examples": examples,
            "$defs": {"legacy": legacy, "native": native, "nativeBitmapBrushResources": serde_json::from_str::<Value>(include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json")).expect("native bitmap brush resources schema is valid JSON"), "nativePropertyMetadata": native_metadata, "nativeFontAssets": native_fonts, "nativeObjectItem": native_items},
            "if": {"required": ["apiVersion"], "properties": {"apiVersion": {"const": 1}}},
            "then": {"$ref": "#/$defs/legacy"},
            "else": {"$ref": "#/$defs/native"},
            "x-nemo-nativeApiVersion": NATIVE_API_VERSION,
            "x-nemo-discovery": "The native transport schema is $defs.native; per-operation declarations, examples and availability come from nemo_discover {operation: name} (or {detail: \"full\"} for every declaration), not from this schema."
        })
        .try_into()
        .expect("conditional command schema is an object")
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
    ) -> (CallToolResult, &'static str) {
        let Some(instance) = request.instance_id.as_deref() else {
            return (
                failure(
                    "invalid_request",
                    "Select an instance with nemo_discover first",
                ),
                "structured_error",
            );
        };
        let endpoints = match registry::read_endpoints(&self.root) {
            Ok(endpoints) => endpoints,
            Err(_) => {
                return (
                    failure("unavailable", "Nemo discovery registry is unavailable"),
                    "structured_error",
                )
            }
        };
        let Some(endpoint) = endpoints
            .into_iter()
            .find(|record| record.instance_id == instance)
        else {
            return (
                failure(
                    "unavailable",
                    "Selected instance is absent; discover again after reconnect",
                ),
                "structured_error",
            );
        };
        match wire::call(&endpoint, request, context.ct.clone()).await {
            Ok(response) => {
                let ok = response.ok;
                let value =
                    serde_json::to_value(response).expect("serializable application response");
                if ok {
                    (CallToolResult::structured(value), "success")
                } else {
                    (CallToolResult::structured_error(value), "structured_error")
                }
            }
            Err(error) => (
                failure("unavailable", &error.to_string()),
                if context.ct.is_cancelled() {
                    "cancelled"
                } else {
                    "transport_error"
                },
            ),
        }
    }

    async fn call_native(
        &self,
        request: NativeApplicationRequest,
        context: RequestContext<RoleServer>,
    ) -> (CallToolResult, &'static str) {
        let endpoints = match registry::read_endpoints(&self.root) {
            Ok(endpoints) => endpoints,
            Err(_) => {
                return (
                    failure("unavailable", "Nemo discovery registry is unavailable"),
                    "structured_error",
                )
            }
        };
        let Some(endpoint) = endpoints
            .into_iter()
            .find(|record| record.instance_id == request.instance_id)
        else {
            return (
                failure(
                    "unavailable",
                    "Selected instance is absent; discover again after reconnect",
                ),
                "structured_error",
            );
        };
        match wire::call_native(&endpoint, request, context.ct.clone()).await {
            Ok(response) => {
                let ok = response.ok;
                let value = serde_json::to_value(response).expect("serializable native response");
                if ok {
                    (CallToolResult::structured(value), "success")
                } else {
                    (CallToolResult::structured_error(value), "structured_error")
                }
            }
            Err(error) => (
                failure("unavailable", &error.to_string()),
                if context.ct.is_cancelled() {
                    "cancelled"
                } else {
                    "transport_error"
                },
            ),
        }
    }
}

struct RequestTrace {
    span: Span,
    finished: bool,
}

impl RequestTrace {
    fn start(context: &RequestContext<RoleServer>, request_id: &str) -> Self {
        let mcp_request_id = match &context.id {
            rmcp::model::RequestId::Number(value) => value.to_string(),
            rmcp::model::RequestId::String(value) => safe_identifier(value),
        };
        let application_request_id = safe_identifier(request_id);
        let span = tracing::info_span!(
            "mcp_request",
            tool = "nemo_command",
            mcp_request_id = %mcp_request_id,
            application_request_id = %application_request_id
        );
        tracing::info!(parent: &span, event = "started");
        Self {
            span,
            finished: false,
        }
    }

    fn span(&self) -> Span {
        self.span.clone()
    }

    fn finish(mut self, status: &'static str) {
        tracing::info!(parent: &self.span, event = "terminal", status);
        self.finished = true;
    }
}

impl Drop for RequestTrace {
    fn drop(&mut self) {
        if !self.finished {
            tracing::info!(parent: &self.span, event = "terminal", status = "cancelled");
        }
    }
}

fn safe_identifier(value: &str) -> String {
    let mut bytes = value.bytes();
    let allowed_bytes = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    let safe_short = value.len() < 36
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && allowed_bytes;
    let safe_uuid = value.len() == 36 && uuid::Uuid::parse_str(value).is_ok() && allowed_bytes;
    if safe_short || safe_uuid {
        value.to_owned()
    } else {
        "redacted".to_owned()
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
        description = "Start here. Lists the running Nemo instances (instanceId, pid, buildId, open documentId, documentKind, revision) and a short start-here guide for the open document's kind: the few operations you need, each with one request to copy. Ask for more with arguments: {\"operation\": name} for one operation's full contract, {\"detail\": \"operations\", \"family\": ..., \"page\": n} for names with purposes and availability, {\"detail\": \"full\"} for every declaration (very large). Launch Nemo if no instance responds. No document is edited."
    )]
    async fn nemo_discover(
        &self,
        Parameters(args): Parameters<DiscoverArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        match discovery::run(&self.root, args, &context.ct).await {
            Ok(answer) => CallToolResult::structured(answer),
            Err((code, message)) => failure(code, &message),
        }
    }

    #[tool(
        description = "Read through apiVersion 1: capabilities, snapshot (documentId, revision, current frame, layer list), property.get and diagnostics.trace. Select the instance from nemo_discover. snapshot is the quick way to orient; reads of an object document's objects and properties are apiVersion 2 queries sent through nemo_command (nemo_discover shows the request to copy). The payload schema carries one template per operation; property.get needs {\"layerId\",\"property\"} and the other reads take {}."
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
        self.call_legacy(request, context).await.0
    }

    #[tool(
        description = "Read and edit the open document, and open, save or export projects. Workflow: call nemo_discover first; its startHere lists the steps for the open document, each with a request to copy (instanceId and documentId are filled in). apiVersion 2 requests read (query.*) or edit (command.*) the document: a read carries atRevision in its payload and never expectedRevision; a command carries expectedRevision (the current revision, +1 after each command) and a new requestId. apiVersion 1 carries project.open, project.new, project.saveAs and project.export, with documentId and expectedRevision from a snapshot. Identical retries reuse their result; a changed body under the same requestId fails. A refusal names the operation to use instead where there is one; nemo_discover {operation} gives any operation's full contract. Send payload as a JSON object, never as a JSON-encoded string."
    )]
    async fn nemo_command(
        &self,
        Parameters(request): Parameters<CommandRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let request_id = match &request {
            CommandRequest::Legacy(request) => request.request_id.as_str(),
            CommandRequest::Native(request) => request.request_id.as_str(),
        };
        let trace = RequestTrace::start(&context, request_id);
        let cancellation = context.ct.clone();
        let (response, status) = async {
            match request {
                CommandRequest::Legacy(request) => {
                    if request.operation.is_query() {
                        return (
                            failure("invalid_request", "Use nemo_query for reads"),
                            "structured_error",
                        );
                    }
                    if let Err(error) = request.validate() {
                        return (failure(error.code(), error.message()), "structured_error");
                    }
                    self.call_legacy(request, context).await
                }
                CommandRequest::Native(request) => {
                    if let Err(error) = request.validate() {
                        return (failure(error.code(), error.message()), "structured_error");
                    }
                    self.call_native(request, context).await
                }
            }
        }
        .instrument(trace.span())
        .await;
        if cancellation.is_cancelled() {
            drop(trace);
        } else {
            trace.finish(status);
        }
        response
    }
}

#[tool_handler(router = self.tool_router, name = "nemo", version = "0.1.0", instructions = "Call nemo_discover first: it lists the running instances and, for the open document, a short guide with a request to copy for each step (pin a snapshot, read, edit a property, undo, redo, history, save, open, export). Send those requests with nemo_command, or nemo_query for the apiVersion 1 snapshot. Ask nemo_discover with {operation: name} for one operation's schema and example, or {detail: operations} for names, purposes and availability. Nemo owns document state. After a cancelled or disconnected write query state before retrying; reuse the exact requestId and body for idempotent retries.")]
impl ServerHandler for NemoServer {}
