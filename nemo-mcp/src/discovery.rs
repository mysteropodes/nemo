//! `nemo_discover`: a compact answer by default, detail on request. The full
//! declaration dump a client once always received is `detail: "full"`.
use crate::{
    capabilities,
    contract::{ApplicationRequest, NativeHostStatus, NativeStatusRequest, Operation, NATIVE_API_VERSION},
    discovery_catalog::{self as catalog, Selected},
    discovery_contract, registry, wire,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

/// A refusal: stable code and a message that says what to do next.
pub type Refusal = (&'static str, String);

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    Guide,
    Operation,
    Operations,
    Full,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoverArgs {
    /// guide (default): instances plus a start-here guide for the open document.
    /// operation: one operation's full contract (needs `operation`).
    /// operations: operation names with one-line purposes and availability, 60 per page.
    /// full: every registered declaration verbatim (very large; rarely needed).
    #[serde(default)]
    pub detail: Option<Detail>,
    /// Operation name for detail "operation", e.g. command.document.object.property.edit.
    #[serde(default)]
    pub operation: Option<String>,
    /// Filter detail "operations" by family, e.g. property, layer, project, history.
    #[serde(default)]
    pub family: Option<String>,
    /// Page of detail "operations", starting at 1.
    #[serde(default)]
    pub page: Option<u32>,
    /// 1 or 2: which route to describe when a name exists on both (history.undo, history.redo).
    #[serde(default)]
    pub api_version: Option<u32>,
    /// Instance whose open document the answer describes; the first with a document by default.
    #[serde(default)]
    pub instance_id: Option<String>,
}

pub struct Instance {
    /// The apiVersion 1 shape `detail: "full"` has always returned.
    pub full: Value,
    pub instance_id: String,
    pub status: Option<NativeHostStatus>,
    pub document_id: Option<String>,
    pub revision: Option<u64>,
    pub v1_operations: Option<Vec<String>>,
}

impl Instance {
    pub fn kind(&self) -> Option<&str> {
        self.status.as_ref().filter(|s| s.available)?.document_kind.as_deref()
    }

    fn selected(&self) -> Selected<'_> {
        Selected {
            kind: self.kind(),
            v1_operations: self.v1_operations.as_deref(),
            identity: self.document_id.as_deref().zip(self.revision).map(|(d, r)| (self.instance_id.as_str(), d, r)),
        }
    }
}

/// Ask every registered instance who it is. An instance that answers neither route is omitted.
pub async fn probe(root: &Path, ct: &wire::CancellationToken) -> Result<Vec<Instance>, Refusal> {
    let endpoints = registry::read_endpoints(root)
        .map_err(|_| ("unavailable", "Nemo discovery registry is unavailable".to_owned()))?;
    let wait = std::time::Duration::from_secs(2);
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
        let legacy = tokio::time::timeout(wait, wire::call(&endpoint, request, ct.child_token()))
            .await
            .ok()
            .and_then(Result::ok)
            .filter(|response| response.ok);
        let status_request = NativeStatusRequest {
            api_version: NATIVE_API_VERSION,
            request_id: uuid::Uuid::new_v4().to_string(),
            instance_id: endpoint.instance_id.clone(),
        };
        let probe_native = legacy.as_ref().is_none_or(|response| {
            response.result.as_ref().is_some_and(|result| {
                result.get("nativeApiVersion").and_then(Value::as_u64) == Some(NATIVE_API_VERSION.into())
            })
        });
        let status = if probe_native {
            let probe = wire::native_status(&endpoint, status_request.clone(), ct.child_token());
            match tokio::time::timeout(wait, probe).await {
                Ok(status) => status.map_err(|error| format!("native host status unavailable: {error}")),
                Err(_) => Err("native host status timed out".to_owned()),
            }
        } else {
            Err("endpoint does not advertise nativeApiVersion 2".to_owned())
        };
        let v1_operations = legacy.as_ref().and_then(|r| r.result.as_ref()).and_then(|r| r["operations"].as_array()).map(
            |ops| ops.iter().filter_map(|op| op.as_str().map(str::to_owned)).collect(),
        );
        let instance = if let Some(response) = legacy {
            let native_status = status.unwrap_or_else(|reason| NativeHostStatus::unavailable(&status_request, reason));
            let full = json!({"instanceId": endpoint.instance_id, "buildId": endpoint.build_id,
                "documentId": response.document_id, "revision": response.revision,
                "capabilities": response.result, "nativeHostStatus": native_status});
            let (document_id, revision) = if native_status.available {
                (native_status.document_id.clone(), native_status.content_revision)
            } else {
                (Some(response.document_id.clone()).filter(|d| !d.is_empty()), Some(response.revision))
            };
            Instance { full, instance_id: endpoint.instance_id, status: Some(native_status), document_id, revision, v1_operations }
        } else {
            let Ok(native_status) = status else { continue };
            let active = native_status.available.then_some(&native_status);
            let full = json!({"instanceId": endpoint.instance_id, "buildId": endpoint.build_id,
                "documentId": active.and_then(|s| s.document_id.as_deref()),
                "revision": active.and_then(|s| s.content_revision),
                "capabilities": null, "nativeHostStatus": native_status});
            let (document_id, revision) = (native_status.document_id.clone(), native_status.content_revision);
            Instance { full, instance_id: endpoint.instance_id, status: Some(native_status), document_id, revision, v1_operations: None }
        };
        instances.push(instance);
    }
    Ok(instances)
}

fn full_answer(instances: &[Instance]) -> Value {
    json!({
        "apiVersion": 1,
        "registeredCapabilities": capabilities::catalog().descriptors(),
        "nativeApiVersion": NATIVE_API_VERSION,
        "registeredNativeCapabilities": capabilities::native_catalog().descriptors(),
        "instances": instances.iter().map(|i| i.full.clone()).collect::<Vec<_>>()
    })
}

fn instance_row(instance: &Instance) -> Value {
    let status = instance.status.as_ref();
    let mut row = json!({
        "instanceId": instance.instance_id, "buildId": instance.full["buildId"],
        "pid": status.and_then(|s| s.pid), "documentId": instance.document_id,
        "documentKind": instance.kind(), "revision": instance.revision,
        "lifecycleGeneration": status.and_then(|s| s.lifecycle_generation),
        "nativeHostStatus": status,
    });
    if let Some(ops) = &instance.v1_operations {
        let mut all = ops.clone();
        all.extend(catalog::project_operations().filter(|p| !ops.iter().any(|o| o == p)).map(str::to_owned));
        row["v1Operations"] = json!(all);
    }
    row
}

/// The request one step sends, with the open document's identity filled in.
fn step_request(step: &Value, instance: Option<&Instance>) -> Value {
    let revision = instance.and_then(|i| i.revision);
    let payload = fill_revision(&step["payload"], revision);
    let (api, id) = (step["apiVersion"].as_u64().unwrap_or(2), step["id"].as_str().unwrap_or("step"));
    let instance_id = instance.map(|i| i.instance_id.as_str());
    if step["tool"] == "nemo_query" {
        return json!({"instanceId": instance_id, "operation": step["operation"], "payload": payload});
    }
    let mut request = json!({"apiVersion": api, "requestId": format!("{id}-1"), "instanceId": instance_id});
    if let Some(document) = instance.and_then(|i| i.document_id.as_deref()) {
        request["documentId"] = json!(document);
    }
    if step["expectedRevision"] == true && revision.is_some() {
        request["expectedRevision"] = json!(revision);
    }
    request["operation"] = step["operation"].clone();
    request["payload"] = payload;
    request
}

fn fill_revision(value: &Value, revision: Option<u64>) -> Value {
    match value {
        Value::String(text) if text == "$revision" => json!(revision.unwrap_or(0)),
        Value::Array(items) => Value::Array(items.iter().map(|v| fill_revision(v, revision)).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), fill_revision(v, revision))).collect()),
        other => other.clone(),
    }
}

pub fn start_here(instance: Option<&Instance>) -> Value {
    let guide = catalog::guide();
    let kind = instance.and_then(Instance::kind).filter(|k| guide["kinds"].get(k).is_some()).unwrap_or("none");
    let section = &guide["kinds"][kind];
    let steps: Vec<Value> = section["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|step| {
            json!({"id": step["id"], "tool": step["tool"], "operation": step["operation"],
                "purpose": step["purpose"], "request": step_request(step, instance)})
        })
        .collect();
    let mut notes: Vec<Value> = guide["notes"].as_array().cloned().unwrap_or_default();
    if instance.is_none() {
        notes.insert(0, json!("No running Nemo instance answered. Launch Nemo, then call nemo_discover again."));
    }
    json!({"instanceId": instance.map(|i| i.instance_id.as_str()), "documentKind": instance.and_then(Instance::kind),
        "title": section["title"], "notes": notes, "steps": steps})
}

fn more(instance: Option<&Instance>) -> Value {
    let name = match instance.and_then(Instance::kind) {
        Some("opacity") => "command.document.apply",
        _ => "command.document.object.property.edit",
    };
    json!({
        "operation": {"arguments": {"operation": name}, "gives": "request schema, example, result shape, refusal codes and availability of one operation"},
        "operations": {"arguments": {"detail": "operations", "family": "property", "page": 1}, "gives": "operation names with a one-line purpose and availability for the open document; family and page are optional"},
        "full": {"arguments": {"detail": "full"}, "gives": "every registered declaration verbatim (about 280,000 characters)"},
        "otherInstance": {"arguments": {"instanceId": "<instanceId from instances>"}},
    })
}

fn choose<'a>(instances: &'a [Instance], wanted: Option<&str>) -> Result<Option<&'a Instance>, Refusal> {
    match wanted {
        Some(id) => instances.iter().find(|i| i.instance_id == id).map(Some).ok_or((
            "invalid_request",
            format!("instance {id} is not running; call nemo_discover without arguments to list instances"),
        )),
        None => Ok(instances.iter().find(|i| i.kind().is_some()).or_else(|| instances.first())),
    }
}

pub async fn run(root: &Path, args: DiscoverArgs, ct: &wire::CancellationToken) -> Result<Value, Refusal> {
    let instances = probe(root, ct).await?;
    let detail = args.detail.unwrap_or(if args.operation.is_some() { Detail::Operation } else { Detail::Guide });
    if detail == Detail::Full {
        return Ok(full_answer(&instances));
    }
    let chosen = choose(&instances, args.instance_id.as_deref())?;
    let selected = chosen.map_or(Selected { kind: None, v1_operations: None, identity: None }, Instance::selected);
    match detail {
        Detail::Operation => {
            let name = args.operation.as_deref().ok_or((
                "invalid_request",
                "detail \"operation\" needs the operation argument, e.g. {\"operation\": \"command.document.object.undo\"}".to_owned(),
            ))?;
            catalog_contract(name, args.api_version, &selected)
        }
        Detail::Operations => {
            let page = args.page.unwrap_or(1) as usize;
            catalog::list_page(&selected, args.family.as_deref(), page).map_err(|message| ("invalid_request", message))
        }
        _ => Ok(json!({
            "apiVersion": 1, "nativeApiVersion": NATIVE_API_VERSION, "detail": "guide",
            "instances": instances.iter().map(instance_row).collect::<Vec<_>>(),
            "startHere": start_here(chosen), "more": more(chosen),
        })),
    }
}

fn catalog_contract(name: &str, api: Option<u32>, selected: &Selected) -> Result<Value, Refusal> {
    discovery_contract::contract(name, api, selected).ok_or_else(|| {
        let near: Vec<&str> = catalog::all_native_operations().into_iter().filter(|op| op.contains(name) || name.contains(op)).take(5).collect();
        ("invalid_request", format!("unknown operation {name}; list names with detail \"operations\"; similar: {}", near.join(", ")))
    })
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
