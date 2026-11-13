//! One operation's full contract, assembled from its capability declaration and the
//! generated transport schema; nothing here is a second source of truth.
use crate::{
    contract::Operation,
    discovery_catalog::{self as catalog, Applicability, Selected},
    native_contract::request_schema,
    project_contract,
};
use serde_json::{json, Value};

const V1_REFUSALS: [&str; 6] = [
    "invalid_request",
    "stale_revision",
    "unavailable",
    "unsaved_changes",
    "indeterminate",
    "export_failed",
];

/// Declared codes, or the transport's common set when a declaration names none.
fn refusal_codes(d: &Value) -> Vec<Value> {
    let declared: Vec<Value> = ["commonErrorCodes", "domainErrorCodes"]
        .iter()
        .flat_map(|key| d[*key].as_array().into_iter().flatten().cloned())
        .collect();
    if declared.is_empty() {
        COMMON_REFUSALS.iter().map(|code| json!(code)).collect()
    } else {
        declared
    }
}

const COMMON_REFUSALS: [&str; 9] = [
    "invalid_request",
    "wrong_instance",
    "wrong_document",
    "stale_revision",
    "busy_conflict",
    "not_found",
    "unavailable",
    "cancelled_before_dispatch",
    "internal",
];

fn live_request(mut request: Value, selected: &Selected) -> Value {
    if let Some((instance, document, revision)) = selected.identity {
        request["instanceId"] = json!(instance);
        if request.get("documentId").is_some() {
            request["documentId"] = json!(document);
        }
        if request.get("expectedRevision").is_some() {
            request["expectedRevision"] = json!(revision);
        }
        if let Some(payload) = request.get_mut("payload") {
            if payload.get("atRevision").is_some() {
                payload["atRevision"] = json!(revision);
            }
        }
    }
    request
}

fn availability(operation: &str, api: u32, selected: &Selected) -> Value {
    let state = if api == 1 {
        catalog::v1_applicability(selected.kind, selected.v1_operations, operation)
    } else {
        match selected.kind {
            Some(kind) => catalog::native_applicability(kind, operation),
            None => catalog::declared_applicability(operation),
        }
    };
    let kinds = catalog::descriptor(operation).map(catalog::document_kinds);
    let mut out = match state {
        Applicability::Available => json!({"state": "available"}),
        Applicability::Unavailable(reason) => json!({"state": "unavailable", "reason": reason}),
        Applicability::OtherKind { use_instead } => json!({"state": "not-for-open-document",
            "reason": format!("does not apply to the open {} document", selected.kind.unwrap_or("")),
            "useInstead": use_instead}),
        Applicability::Undeclared => json!({"state": "undeclared"}),
    };
    out["documentKinds"] = json!(kinds);
    out["openDocumentKind"] = json!(selected.kind);
    out
}

fn native_contract(operation: &str, selected: &Selected) -> Option<Value> {
    let d = catalog::descriptor(operation)?;
    let policy = request_schema::expected_revision_policy(operation);
    let example = d["examples"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["request"]["operation"] == operation);
    let mut out = json!({
        "operation": operation, "api": 2, "tool": "nemo_command",
        "family": catalog::family_of(operation), "capability": d["id"],
        "purpose": d["purposes"][operation],
        "availability": availability(operation, 2, selected),
        "expectedRevision": match policy {
            Some(true) => "required: the current revision",
            Some(false) => "forbidden: reads carry atRevision in the payload instead",
            None => "not declared",
        },
        "requestSchema": request_schema::standalone_payload_schema(operation),
        "refusalCodes": refusal_codes(d),
    });
    for key in [
        "semantics",
        "readPolicy",
        "input",
        "output",
        "resourceBoundary",
    ] {
        if !d[key].is_null() {
            out[key] = d[key].clone();
        }
    }
    match example {
        Some(example) => {
            out["example"] = live_request(example["request"].clone(), selected);
            if !example["response"].is_null() {
                out["exampleResponse"] = example["response"].clone();
            }
        }
        None => out["example"] = json!("none declared; build the request from requestSchema"),
    }
    Some(out)
}

fn v1_contract(operation: &str, selected: &Selected) -> Option<Value> {
    let entry = catalog::guide()["legacyOperations"].get(operation)?;
    let op: Operation = serde_json::from_value(json!(operation)).ok()?;
    let query = catalog::is_v1_query(operation);
    let schema = if project_contract::is_project_operation(&op) {
        project_contract::payload_schemas(&[op]).pop()
    } else {
        Some(
            json!({"type": "object", "description": "property operations name a registered capability in payload.property; see example"}),
        )
    };
    let steps = catalog::guide()["kinds"]
        .as_object()?
        .values()
        .flat_map(|k| k["steps"].as_array().into_iter().flatten());
    let payload = steps
        .filter(|step| step["operation"] == operation)
        .map(|step| step["payload"].clone())
        .next()
        .unwrap_or_else(|| op.payload_example());
    let example = if query {
        json!({"instanceId": selected.identity.map(|i| i.0), "operation": operation, "payload": payload})
    } else {
        let request = json!({"apiVersion": 1, "requestId": "unique-request-id", "instanceId": null,
            "documentId": null, "expectedRevision": 0, "operation": operation, "payload": payload});
        let mut request = live_request(request, selected);
        request["documentId"] = json!(selected.identity.map(|i| i.1));
        request
    };
    Some(json!({
        "operation": operation, "api": 1, "tool": if query { "nemo_query" } else { "nemo_command" },
        "family": entry["family"], "purpose": entry["purpose"],
        "availability": availability(operation, 1, selected),
        "expectedRevision": if query { "not used" } else { "required with the documentId of the latest snapshot (project.open and project.new need only instanceId)" },
        "requestSchema": schema, "example": example,
        "refusalCodes": V1_REFUSALS,
        "result": "every response carries ok, the document identity and result or error",
    }))
}

/// The full contract of one operation, or `None` when no registered declaration names it.
/// `api` picks a side when a name exists on both routes (history.undo, history.redo);
/// the native route answers when it is not given.
pub fn contract(operation: &str, api: Option<u32>, selected: &Selected) -> Option<Value> {
    match api {
        Some(1) => v1_contract(operation, selected),
        Some(_) => native_contract(operation, selected),
        None => native_contract(operation, selected).or_else(|| v1_contract(operation, selected)),
    }
}
