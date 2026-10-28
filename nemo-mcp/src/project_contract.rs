//! Structural admission and JSON Schema for the four v1 project transport
//! primitives (`project.open`, `project.new`, `project.saveAs`, `project.export`).
//!
//! Only key presence, key spelling, JSON types and numeric/length bounds are
//! checked here. Filesystem scope and document existence stay with the app.
use crate::contract::{Operation, RequestError};
use serde_json::{json, Map, Value};

const MAX_STRING_BYTES: usize = 4096;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const FORMATS: [&str; 3] = ["png", "svg", "lottie"];

pub(crate) const fn is_project_operation(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::ProjectOpen
            | Operation::ProjectNew
            | Operation::ProjectSaveAs
            | Operation::ProjectExport
    )
}

pub(crate) fn validate(
    operation: &Operation,
    payload: &Map<String, Value>,
    expected: &str,
) -> Result<(), RequestError> {
    let (required, optional): (&[&str], &[&str]) = match operation {
        Operation::ProjectOpen => (&["path"], &[]),
        Operation::ProjectNew => (&["canvasW", "canvasH", "fps"], &[]),
        Operation::ProjectSaveAs => (&["path"], &[]),
        _ => (&["format", "destination"], &["start", "end"]),
    };
    let fail = |message: String| {
        Err(RequestError::MalformedPayload(format!(
            "{message}; {expected}"
        )))
    };
    let label = operation.label();
    for key in payload.keys() {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return fail(format!("{label} payload contains unsupported field {key}"));
        }
    }
    for key in required {
        if !payload.contains_key(*key) {
            return fail(format!("{label} payload is missing {key}"));
        }
    }
    for (key, value) in payload {
        let problem = match key.as_str() {
            "path" | "destination" => absolute_path_problem(value),
            "format" => match value.as_str() {
                Some(format) if FORMATS.contains(&format) => None,
                _ => Some("must be one of png, svg, lottie".to_owned()),
            },
            "canvasW" | "canvasH" => integer_problem(value, 1, 16_384),
            "fps" => integer_problem(value, 1, 240),
            _ => integer_problem(value, 0, MAX_SAFE_INTEGER),
        };
        if let Some(problem) = problem {
            return fail(format!("payload.{key} {problem}"));
        }
    }
    if let (Some(start), Some(end)) = (
        payload.get("start").and_then(Value::as_u64),
        payload.get("end").and_then(Value::as_u64),
    ) {
        if start > end {
            return fail("payload.start must not exceed payload.end".to_owned());
        }
    }
    Ok(())
}

fn absolute_path_problem(value: &Value) -> Option<String> {
    let Some(text) = value.as_str() else {
        return Some("must be a string".to_owned());
    };
    if text.is_empty() || text.len() > MAX_STRING_BYTES {
        return Some(format!("must contain 1..{MAX_STRING_BYTES} bytes"));
    }
    (!text.starts_with('/')).then(|| "must be an absolute path starting with /".to_owned())
}

fn integer_problem(value: &Value, min: u64, max: u64) -> Option<String> {
    match value.as_u64() {
        Some(number) if (min..=max).contains(&number) => None,
        _ => Some(format!("must be an integer in {min}..={max}")),
    }
}

pub(crate) fn payload_example(operation: &Operation) -> Value {
    match operation {
        Operation::ProjectOpen => json!({"path": "/Users/example/Documents/project.json"}),
        Operation::ProjectNew => json!({"canvasW": 1920, "canvasH": 1080, "fps": 24}),
        Operation::ProjectSaveAs => json!({"path": "/Users/example/Documents/project.json"}),
        _ => {
            json!({"format": "png", "destination": "/Users/example/Documents/export", "start": 0, "end": 0})
        }
    }
}

pub(crate) const fn required_keys(operation: &Operation) -> &'static [&'static str] {
    match operation {
        Operation::ProjectOpen | Operation::ProjectSaveAs => &["path"],
        Operation::ProjectNew => &["canvasW", "canvasH", "fps"],
        _ => &["format", "destination"],
    }
}

fn path_schema() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": MAX_STRING_BYTES, "pattern": "^/"})
}

fn int_schema(min: u64, max: u64) -> Value {
    json!({"type": "integer", "minimum": min, "maximum": max})
}

/// One strict branch per project operation, for the payload `anyOf`.
pub(crate) fn payload_schemas(operations: &[Operation]) -> Vec<Value> {
    operations
        .iter()
        .filter(|operation| is_project_operation(operation))
        .map(|operation| {
            let (properties, required) = match operation {
                Operation::ProjectOpen => (json!({"path": path_schema()}), json!(["path"])),
                Operation::ProjectNew => (
                    json!({"canvasW": int_schema(1, 16_384), "canvasH": int_schema(1, 16_384),
                           "fps": int_schema(1, 240)}),
                    json!(["canvasW", "canvasH", "fps"]),
                ),
                Operation::ProjectSaveAs => (json!({"path": path_schema()}), json!(["path"])),
                _ => (
                    json!({"format": {"enum": FORMATS}, "destination": path_schema(),
                           "start": int_schema(0, MAX_SAFE_INTEGER),
                           "end": int_schema(0, MAX_SAFE_INTEGER)}),
                    json!(["format", "destination"]),
                ),
            };
            json!({"type": "object", "additionalProperties": false,
                   "properties": properties, "required": required})
        })
        .collect()
}

#[cfg(test)]
#[path = "project_contract_tests.rs"]
mod tests;
