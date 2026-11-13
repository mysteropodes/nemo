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

const PIN_PREFIX: &str = "native-object:";

/// Fixture identities inside a declared example, swapped for the open document's or,
/// where there is none to give, for a placeholder the client fills from an earlier step.
struct Localize<'a> {
    fixture_document: Option<&'a str>,
    live: Option<(&'a str, u64)>,
    object_kind: bool,
}

impl Localize<'_> {
    fn pin(&self) -> String {
        match self.live {
            Some((document, revision)) if self.object_kind => {
                format!("{PIN_PREFIX}{document}:{revision}")
            }
            _ => "<documentSnapshotId>".to_owned(),
        }
    }

    fn walk(&self, value: &mut Value) {
        match value {
            Value::String(text) if text.starts_with(PIN_PREFIX) => *text = self.pin(),
            Value::String(text) => {
                if let Some(fixture) = self.fixture_document.filter(|f| text.contains(f)) {
                    *text = text.replace(fixture, self.live.map_or("<documentId>", |l| l.0));
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.walk(item)),
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    match (key.as_str(), &*item) {
                        ("documentSnapshotId", Value::String(_)) => *item = json!(self.pin()),
                        ("layerUid", Value::String(_)) => *item = json!("<layerUid>"),
                        ("contextId", Value::String(id)) if id != "scene-root" => {
                            *item = json!("<contextId>")
                        }
                        ("atRevision", Value::Number(_)) if self.live.is_some() => {
                            *item = json!(self.live.map(|l| l.1))
                        }
                        _ => self.walk(item),
                    }
                }
            }
            _ => {}
        }
    }
}

fn live_request(mut request: Value, selected: &Selected) -> Value {
    let fixture_document = request["documentId"].as_str().map(str::to_owned);
    let live = selected
        .identity
        .map(|(_, document, revision)| (document, revision));
    if let Some(payload) = request.get_mut("payload") {
        Localize {
            fixture_document: fixture_document.as_deref(),
            live,
            object_kind: selected.kind == Some("object"),
        }
        .walk(payload);
    }
    match selected.identity {
        Some((instance, document, revision)) => {
            request["instanceId"] = json!(instance);
            if request.get("documentId").is_some() {
                request["documentId"] = json!(document);
            }
            if request.get("expectedRevision").is_some() {
                request["expectedRevision"] = json!(revision);
            }
        }
        None => {
            request["instanceId"] = json!("<instanceId>");
            if request.get("documentId").is_some() {
                request["documentId"] = json!("<documentId>");
            }
        }
    }
    request
}

fn availability(operation: &str, api: u32, selected: &Selected) -> Value {
    let state = if api == 1 {
        catalog::v1_applicability(selected.kind, selected.v1_operations, operation)
    } else {
        catalog::applicability_for(selected.kind, operation)
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
    let examples: Vec<&Value> = d["examples"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["request"]["operation"] == operation)
        .collect();
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
    match examples.split_first() {
        Some((first, rest)) => {
            out["example"] = live_request(first["request"].clone(), selected);
            if !first["response"].is_null() {
                out["exampleResponse"] = first["response"].clone();
            }
            if !rest.is_empty() {
                out["otherExamples"] = json!(rest
                    .iter()
                    .map(|e| live_request(e["request"].clone(), selected))
                    .collect::<Vec<_>>());
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
        request["instanceId"] = json!(selected.identity.map(|i| i.0));
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

/// Root examples advertised in the nemo_command input schema: object-document requests
/// taken from the start-here guide, so the schema cannot show a request the open
/// document kind refuses. Each is validated against the schema by the transport tests.
pub fn command_schema_examples() -> Vec<Value> {
    let steps = catalog::guide()["kinds"]["object"]["steps"]
        .as_array()
        .into_iter()
        .flatten();
    let wanted = ["edit-property", "undo", "redo"];
    steps
        .filter(|step| wanted.contains(&step["id"].as_str().unwrap_or_default()))
        .map(|step| {
            json!({
                "apiVersion": 2, "requestId": "unique-request-id", "instanceId": "discovered-instance-id",
                "documentId": "snapshot-document-id", "expectedRevision": 0,
                "operation": step["operation"], "payload": step["payload"]
            })
        })
        .collect()
}
