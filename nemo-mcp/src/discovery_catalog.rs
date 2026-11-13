//! What the registered declarations say about each operation for the open document
//! kind. One source: the capability descriptors (`documentKinds`, `purposes`,
//! `useInstead`, availability). The desktop host's own refusal and its test read the
//! same functions, so discovery cannot advertise what the product refuses.
use crate::{capabilities::native_catalog, contract::Operation};
use serde_json::{json, Value};
use std::sync::OnceLock;

pub const PAGE_SIZE: usize = 60;
const GUIDE_SOURCE: &str = include_str!("../../engineering/application/discovery-guide-v1.json");

/// The apiVersion 1 project operations run the app's lifecycle flows on every kind.
pub fn project_operations() -> impl Iterator<Item = &'static str> {
    Operation::labels().iter().copied().filter(|label| label.starts_with("project."))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applicability {
    Available,
    /// The capability declares itself unavailable in this build.
    Unavailable(String),
    /// Declared for other document kinds only.
    OtherKind { use_instead: Option<String> },
    Undeclared,
}

pub fn guide() -> &'static Value {
    static GUIDE: OnceLock<Value> = OnceLock::new();
    GUIDE.get_or_init(|| serde_json::from_str(GUIDE_SOURCE).expect("discovery guide is valid JSON"))
}

pub fn descriptor(operation: &str) -> Option<&'static Value> {
    native_catalog().capability_for_operation(operation)
}

pub fn document_kinds(descriptor: &Value) -> Vec<&str> {
    descriptor["documentKinds"]
        .as_array()
        .map(|kinds| kinds.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// The operation to use on a `kind` document instead of `operation`, where a
/// descriptor declares one (v1 and v2 operation names alike).
pub fn use_instead(kind: &str, operation: &str) -> Option<String> {
    native_catalog()
        .descriptors()
        .iter()
        .find_map(|d| d["useInstead"][kind][operation].as_str())
        .map(str::to_owned)
}

pub fn native_applicability(kind: &str, operation: &str) -> Applicability {
    match declared_applicability(operation) {
        Applicability::Available if !descriptor_kinds_contain(operation, kind) => {
            Applicability::OtherKind { use_instead: use_instead(kind, operation) }
        }
        other => other,
    }
}

/// Availability the declaration alone gives, before any document kind is known.
pub fn declared_applicability(operation: &str) -> Applicability {
    let Some(descriptor) = descriptor(operation) else {
        return Applicability::Undeclared;
    };
    if descriptor["availability"]["state"] == "available" {
        return Applicability::Available;
    }
    let reason = descriptor["availability"]["reason"].as_str();
    Applicability::Unavailable(reason.unwrap_or("declared unavailable").to_owned())
}

fn descriptor_kinds_contain(operation: &str, kind: &str) -> bool {
    descriptor(operation).is_some_and(|d| document_kinds(d).contains(&kind))
}

/// Every public native operation a `kind` document accepts, in declaration order.
pub fn available_native_operations(kind: &str) -> Vec<&'static str> {
    all_native_operations()
        .into_iter()
        .filter(|op| native_applicability(kind, op) == Applicability::Available)
        .collect()
}

/// The refusal text for an operation that does not apply to a `kind` document.
pub fn inapplicable_message(kind: &str, operation: &str) -> Option<String> {
    let Applicability::OtherKind { use_instead } = native_applicability(kind, operation) else {
        return None;
    };
    let hint = use_instead.map_or_else(
        || "No equivalent exists on this document.".to_owned(),
        |other| format!("Use {other} instead."),
    );
    Some(format!("{operation} does not apply to a {kind} document. {hint}"))
}

pub fn v1_applicability(kind: Option<&str>, listed: Option<&[String]>, operation: &str) -> Applicability {
    let Some(listed) = listed else { return Applicability::Available };
    if project_operations().any(|op| op == operation) || listed.iter().any(|op| op == operation) {
        return Applicability::Available;
    }
    Applicability::OtherKind { use_instead: kind.and_then(|k| use_instead(k, operation)) }
}

pub fn all_native_operations() -> Vec<&'static str> {
    native_catalog()
        .descriptors()
        .iter()
        .flat_map(|d| d["operations"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect()
}

pub fn family_of(operation: &str) -> String {
    let mut parts: Vec<&str> = operation.split('.').collect();
    if matches!(parts[0], "query" | "command" | "job") && parts.len() > 1 {
        parts.remove(0);
    }
    if parts[0] == "document" && parts.len() > 1 {
        parts.remove(0);
    }
    if parts[0] == "object" && parts.len() > 1 {
        parts.remove(0);
    }
    let first = parts[0];
    match first {
        "undo" | "redo" => "history",
        "apply" => "opacity",
        "revision" | "snapshot" | "serialize" | "evaluate" | "object" => "document",
        other => other,
    }
    .to_owned()
}

pub struct Selected<'a> {
    pub kind: Option<&'a str>,
    pub v1_operations: Option<&'a [String]>,
    pub identity: Option<(&'a str, &'a str, u64)>,
}

fn v1_entry(operation: &str) -> Option<&'static Value> {
    guide()["legacyOperations"].get(operation)
}

fn applicability_of(operation: &str, api: u32, selected: &Selected) -> Applicability {
    match (api, selected.kind) {
        (1, kind) => v1_applicability(kind, selected.v1_operations, operation),
        (_, Some(kind)) => native_applicability(kind, operation),
        _ => declared_applicability(operation),
    }
}

pub fn summary(operation: &str, api: u32, selected: &Selected) -> Option<Value> {
    let (purpose, family, tool) = if api == 1 {
        let entry = v1_entry(operation)?;
        let query = Operation::labels().contains(&operation) && is_v1_query(operation);
        (entry["purpose"].as_str()?.to_owned(), entry["family"].as_str()?.to_owned(),
         if query { "nemo_query" } else { "nemo_command" })
    } else {
        let d = descriptor(operation)?;
        (d["purposes"][operation].as_str().unwrap_or("").to_owned(), family_of(operation), "nemo_command")
    };
    let mut out = json!({"operation": operation, "purpose": purpose, "family": family, "api": api, "tool": tool});
    match applicability_of(operation, api, selected) {
        Applicability::Available => out["available"] = json!(true),
        Applicability::Unavailable(reason) => {
            out["available"] = json!(false);
            out["why"] = json!(reason);
        }
        Applicability::OtherKind { use_instead } => {
            out["available"] = json!(false);
            out["why"] = json!(format!("does not apply to the open {} document", selected.kind.unwrap_or("")));
            if let Some(other) = use_instead {
                out["useInstead"] = json!(other);
            }
        }
        Applicability::Undeclared => return None,
    }
    Some(out)
}

pub fn is_v1_query(operation: &str) -> bool {
    matches!(operation, "capabilities" | "snapshot" | "property.get" | "diagnostics.trace")
}

pub fn all_summaries(selected: &Selected) -> Vec<Value> {
    let v2 = all_native_operations().into_iter().filter_map(|op| summary(op, 2, selected));
    let v1 = Operation::labels().iter().filter_map(|op| summary(op, 1, selected));
    v2.chain(v1).collect()
}

pub fn families(entries: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = entries.iter().filter_map(|e| e["family"].as_str().map(str::to_owned)).collect();
    out.sort();
    out.dedup();
    out
}

/// One page of the operation list, optionally filtered by family.
pub fn list_page(selected: &Selected, family: Option<&str>, page: usize) -> Result<Value, String> {
    let all = all_summaries(selected);
    let known = families(&all);
    if let Some(family) = family {
        if !known.iter().any(|f| f.eq_ignore_ascii_case(family)) {
            return Err(format!("unknown family {family}; families: {}", known.join(", ")));
        }
    }
    let matching: Vec<Value> = all
        .into_iter()
        .filter(|e| family.is_none_or(|f| e["family"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(f))))
        .collect();
    let pages = matching.len().div_ceil(PAGE_SIZE).max(1);
    if page == 0 || page > pages {
        return Err(format!("page {page} is outside 1..={pages}"));
    }
    let slice = matching[(page - 1) * PAGE_SIZE..(page * PAGE_SIZE).min(matching.len())].to_vec();
    let mut out = json!({"documentKind": selected.kind, "total": matching.len(), "page": page,
        "pages": pages, "operations": slice});
    if page < pages {
        out["next"] = json!({"detail": "operations", "family": family, "page": page + 1});
    }
    if family.is_none() {
        out["families"] = json!(known);
    }
    Ok(out)
}
