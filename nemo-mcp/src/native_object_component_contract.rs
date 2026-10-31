//! L04-N closed public contract of the native component and duplicator setup operations (component convert,
//! expose and place; duplicator set, object pool and path). Pure shape and receipt validation: execution, storage,
//! history and replay stay in the native owner. The table is mirrored by
//! `src/js/adapters/native-object-component-contract.js`.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::{exact, object};
use serde_json::Value;

/// Owner bounds of the wire payloads: identities are non-empty, at most 128 UTF-8 bytes and carry no NUL.
const MAX_ID: usize = 128;
/// A layer list holds one to 256 identities.
const MAX_LAYERS: usize = 256;
/// The record admission refuses an exposed-property key that is not this prefix plus at least one more byte.
const EXPOSED_KEY_PREFIX: &str = "ep_";

#[derive(Clone, Copy)]
enum K {
    /// Any string (a name or label the owner stores verbatim).
    Text,
    Id,
    Ids,
    ExposedKey,
    Number,
    Boolean,
    /// A native u32 (the owner additionally refuses above 999999 when it enables a duplicator).
    Seed,
    /// `null` clears the path layer, a string names one (at most 128 bytes).
    PathUid,
}

/// What a successful receipt of an operation looks like.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receipt {
    /// `{applied, historyEntriesAdded}`.
    Command,
    /// A layer was minted: `{applied: true, historyEntriesAdded: 1, layerUid}`.
    Created,
    /// A merge convert reports its new instance layer, a single-layer convert the compact result.
    Either,
}

/// (operation suffix, command, receipt, "member:kind" in wire order; a trailing `?` marks an optional member).
static SPECS: [(&str, &str, Receipt, &str); 6] = [
    (
        "component.convert",
        "object.component.convert",
        Receipt::Either,
        "contextId:text layerUids:ids",
    ),
    (
        "component.expose",
        "object.component.expose",
        Receipt::Command,
        "symbolId:id key:exposedKey targetStrokeId:id targetField:id label:text default:number?",
    ),
    (
        "component.place",
        "object.component.place",
        Receipt::Created,
        "symbolId:id name:text",
    ),
    (
        "duplicator.set",
        "object.duplicator.set",
        Receipt::Command,
        "layerUid:id enabled:boolean seed:seed?",
    ),
    (
        "duplicator.objects",
        "object.duplicator.objects",
        Receipt::Created,
        "layerUids:ids seed:seed",
    ),
    (
        "duplicator.path",
        "object.duplicator.path",
        Receipt::Command,
        "layerUid:id pathLayerUid:pathUid",
    ),
];
const PREFIX: &str = "command.document.object.";

fn spec(operation: &str) -> Option<(&'static str, Receipt, &'static str)> {
    let suffix = operation.strip_prefix(PREFIX)?;
    SPECS
        .iter()
        .find(|row| row.0 == suffix)
        .map(|row| (row.1, row.2, row.3))
}

pub(crate) fn admitted(operation: &str) -> bool {
    spec(operation).is_some()
}

/// #2256: the one read of the component model, kept beside its commands but a pinned query (no expectedRevision).
#[path = "native_component_read_contract.rs"]
pub(crate) mod read;

/// #2256: the pure enter/exit context transition of the workspace (view state: no revision, no history entry).
#[path = "native_workspace_context_contract.rs"]
pub(crate) mod workspace;

/// A command, the component read or the workspace transition: the operations whose request and result this module validates.
pub(crate) fn owns(operation: &str) -> bool {
    admitted(operation) || operation == read::OP || operation == workspace::OP
}

fn id_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_ID && !text.contains('\0')
}

fn id(value: &Value) -> bool {
    value.as_str().is_some_and(id_text)
}

fn kind(token: &str) -> Option<K> {
    Some(match token {
        "text" => K::Text,
        "id" => K::Id,
        "ids" => K::Ids,
        "exposedKey" => K::ExposedKey,
        "number" => K::Number,
        "boolean" => K::Boolean,
        "seed" => K::Seed,
        "pathUid" => K::PathUid,
        _ => return None,
    })
}

fn check(kind: K, value: &Value) -> bool {
    match kind {
        K::Text => value.is_string(),
        K::Id => id(value),
        K::Ids => value
            .as_array()
            .is_some_and(|items| (1..=MAX_LAYERS).contains(&items.len()) && items.iter().all(id)),
        K::ExposedKey => value.as_str().is_some_and(|key| {
            id_text(key)
                && key.len() > EXPOSED_KEY_PREFIX.len()
                && key.starts_with(EXPOSED_KEY_PREFIX)
        }),
        K::Number => value.is_number() && value.as_f64().is_some_and(f64::is_finite),
        K::Boolean => value.is_boolean(),
        K::Seed => value
            .as_u64()
            .is_some_and(|seed| seed <= u64::from(u32::MAX)),
        K::PathUid => value.is_null() || value.as_str().is_some_and(|path| path.len() <= MAX_ID),
    }
}

/// One member of a spec row: (wire name, kind token, optional).
fn members(row: &'static str) -> Vec<(&'static str, &'static str, bool)> {
    row.split(' ')
        .filter_map(|member| {
            let (name, token) = member.split_once(':')?;
            Some(match token.strip_suffix('?') {
                Some(bare) => (name, bare, true),
                None => (name, token, false),
            })
        })
        .collect()
}

pub(super) fn request(operation: &str, payload: &Value) -> bool {
    if operation == read::OP {
        return read::request(payload);
    }
    if operation == workspace::OP {
        return workspace::request(payload);
    }
    let Some((command, _, row)) = spec(operation) else {
        return false;
    };
    let parsed = members(row);
    let mut required = vec!["command"];
    let mut optional = Vec::new();
    for (name, _, is_optional) in &parsed {
        if *is_optional {
            optional.push(*name);
        } else {
            required.push(*name);
        }
    }
    exact(payload, &required, &optional).is_some_and(|object| {
        object.get("command").and_then(Value::as_str) == Some(command)
            && parsed.iter().all(|(name, token, is_optional)| {
                match (object.get(*name), kind(token)) {
                    (Some(value), Some(k)) => check(k, value),
                    (None, Some(_)) => *is_optional,
                    _ => false,
                }
            })
    })
}

/// The result of a command (receipt only) or of the component read (judged against its own request pins).
pub(super) fn result_for(operation: &str, payload: &Value, value: &Value) -> bool {
    if operation == read::OP {
        read::result(payload, value)
    } else if operation == workspace::OP {
        workspace::result(payload, value)
    } else {
        result(operation, value)
    }
}

pub(super) fn result(operation: &str, value: &Value) -> bool {
    let Some((_, receipt, _)) = spec(operation) else {
        return false;
    };
    match receipt {
        Receipt::Command => object::visibility_result(value),
        Receipt::Created => object::create_result(value),
        Receipt::Either => object::visibility_result(value) || object::create_result(value),
    }
}

/// The receipt advances the revision exactly when it applied one edit.
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
        == Some(revision)
}

/// Success-envelope correlation: same owner and document, and the exact revision advance.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if request.operation == read::OP {
        return read::response_matches(request, response);
    }
    if request.operation == workspace::OP {
        return workspace::response_matches(request, response);
    }
    if !admitted(&request.operation) {
        return true;
    }
    response.instance_id == request.instance_id
        && response.document_id == request.document_id
        && response.result.as_ref().is_some_and(|value| {
            let convert = request.operation == "command.document.object.component.convert";
            let shape_matches = !convert
                || (self::request(&request.operation, &request.payload)
                    && request.payload["layerUids"]
                        .as_array()
                        .is_some_and(|layers| {
                            if layers
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<std::collections::HashSet<_>>()
                                .len()
                                == 1
                            {
                                object::visibility_result(value) && value["applied"] == true
                            } else {
                                object::create_result(value)
                            }
                        }));
            shape_matches
                && revision_matches(request.expected_revision, response.content_revision, value)
        })
}

#[cfg(test)]
#[path = "native_object_component_contract_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_component_receipt_tests.rs"]
mod receipt_tests;
