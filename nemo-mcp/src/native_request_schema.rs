//! Schema-derived request shapes, compiled once per used operation. No document
//! admission, execution availability, or authoritative native semantics live here.
use crate::contract::NativeApplicationRequest;
use jsonschema::{Registry, Resource, Validator};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::OnceLock};

include!(concat!(env!("OUT_DIR"), "/native_request_resources.rs"));

struct Operation {
    branch: usize,
    payload: OnceLock<Result<Validator, String>>,
    envelope: OnceLock<Result<Validator, String>>,
}
struct Catalog {
    schema: Value,
    resources: Registry<'static>,
    operations: HashMap<String, Operation>,
}

fn build_catalog() -> Result<Catalog, String> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .map_err(|e| e.to_string())?;
    build_catalog_from(schema, RESOURCES)
}

fn build_catalog_from(schema: Value, sources: &[(&str, &str)]) -> Result<Catalog, String> {
    let id = schema["$id"].as_str().ok_or("missing transport id")?;
    let base = id.rsplit_once('/').ok_or("invalid transport id")?.0;
    let mut resources = Registry::new();
    for (file, source) in sources {
        let document: Value = serde_json::from_str(source).map_err(|e| format!("{file}: {e}"))?;
        resources = resources
            .add(format!("{base}/{file}"), Resource::from_contents(document))
            .map_err(|e| format!("{file}: {e}"))?;
    }
    resources = resources
        .add(id.to_owned(), Resource::from_contents(schema.clone()))
        .map_err(|e| e.to_string())?;
    let mut operations = HashMap::new();
    for (index, branch) in schema["$defs"]["Request"]["oneOf"]
        .as_array()
        .ok_or("missing request union")?
        .iter()
        .enumerate()
    {
        // The assembler checks this representation before publication. Runtime
        // still fails closed on bad embedded input instead of panicking on traffic.
        let parts = branch["allOf"].as_array().ok_or("missing request allOf")?;
        if parts.len() != 2 {
            return Err("unsupported request branch".into());
        }
        let shape = parts[1].as_object().ok_or("missing request shape")?;
        let properties = shape
            .get("properties")
            .and_then(Value::as_object)
            .ok_or("missing request properties")?;
        if shape.len() != 1
            || properties.len() != 2
            || !properties.contains_key("payload")
            || !properties.contains_key("operation")
        {
            return Err("unsupported request property projection".into());
        }
        let base = parts[0]["$ref"]
            .as_str()
            .ok_or("missing request envelope ref")?;
        if !base.starts_with("#/$defs/") {
            return Err("nonlocal request envelope".into());
        }
        let op = &properties["operation"];
        let names: Vec<_> = match op.get("const") {
            Some(name) => vec![name],
            None => op["enum"]
                .as_array()
                .ok_or("missing operation const/enum")?
                .iter()
                .collect(),
        };
        for name in names {
            let name = name.as_str().ok_or("non-string operation")?.to_owned();
            if operations
                .insert(
                    name,
                    Operation {
                        branch: index,
                        payload: OnceLock::new(),
                        envelope: OnceLock::new(),
                    },
                )
                .is_some()
            {
                return Err("duplicate operation".into());
            }
        }
    }
    Ok(Catalog {
        schema,
        resources: resources.prepare().map_err(|e| e.to_string())?,
        operations,
    })
}

fn catalog() -> Result<&'static Catalog, &'static String> {
    static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            build_catalog()
                .inspect_err(|error| tracing::error!(%error, "native request schema unavailable"))
        })
        .as_ref()
}

fn validator(operation: &str, envelope: bool) -> Result<Option<&'static Validator>, &'static str> {
    let catalog = catalog().map_err(String::as_str)?;
    let Some(entry) = catalog.operations.get(operation) else {
        return Ok(None);
    };
    let cell = if envelope {
        &entry.envelope
    } else {
        &entry.payload
    };
    cell.get_or_init(|| {
        compile(catalog, entry, envelope).inspect_err(
            |error| tracing::error!(%operation, %error, "native request validator unavailable"),
        )
    })
    .as_ref()
    .map(Some)
    .map_err(String::as_str)
}

fn compile(catalog: &Catalog, entry: &Operation, envelope: bool) -> Result<Validator, String> {
    let id = catalog.schema["$id"]
        .as_str()
        .ok_or("missing transport id")?;
    let branch = &catalog.schema["$defs"]["Request"]["oneOf"][entry.branch];
    let schema = if envelope {
        let base = branch["allOf"][0]["$ref"]
            .as_str()
            .ok_or("missing envelope ref")?;
        json!({"allOf":[{"$ref":format!("{id}{base}")},
                {"properties":{"operation":branch["allOf"][1]["properties"]["operation"]}}]})
    } else {
        json!({"$ref":format!("{id}#/$defs/Request/oneOf/{}/allOf/1/properties/payload", entry.branch)})
    };
    // All entries resolve into one shared registry, not cloned full trees.
    jsonschema::draft202012::options()
        .with_registry(&catalog.resources)
        .build(&schema)
        .map_err(|e| e.to_string())
}

pub(super) fn payload(operation: &str, payload: &Value) -> Option<bool> {
    match validator(operation, false) {
        Ok(validator) => validator.map(|validator| validator.is_valid(payload)),
        Err(_) => Some(false),
    }
}

pub(crate) fn envelope(request: &NativeApplicationRequest) -> Option<bool> {
    let validator = match validator(&request.operation, true) {
        Ok(Some(validator)) => validator,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    // Payload is checked separately at its existing diagnostic boundary. The
    // assembler forbids cross-payload conditions in projected envelope branches.
    // Avoid copying/revalidating large media payloads to check metadata.
    let mut value = json!({"apiVersion":request.api_version,"requestId":request.request_id,
        "instanceId":request.instance_id,"documentId":request.document_id,
        "operation":request.operation,"payload":{},"cancelledBeforeDispatch":request.cancelled_before_dispatch});
    if let Some(revision) = request.expected_revision {
        value["expectedRevision"] = json!(revision);
    }
    Some(validator.is_valid(&value))
}

#[cfg(test)]
pub(super) fn declares(operation: &str) -> bool {
    catalog().is_ok_and(|c| c.operations.contains_key(operation))
}

#[cfg(test)]
#[path = "native_request_schema_tests.rs"]
mod tests;
