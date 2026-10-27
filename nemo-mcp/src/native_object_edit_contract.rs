//! L02-J closed public contract of the native object edit operations (layer lifecycle, groups, selection edit,
//! clipboard copy/cut/paste, boolean). Pure shape and receipt validation: execution, storage, history and replay stay in
//! the native owner. The table is mirrored by `src/js/adapters/native-object-edit-contract.js`.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::{exact, object};
use serde_json::Value;
use std::collections::HashSet;
#[path = "native_object_edit_clipboard.rs"]
mod clipboard;
#[path = "native_fill_trace_wire.rs"]
mod fill;

const MAX_ID: usize = 128;
const MAX_NAME: usize = 256;
const MAX_LABEL: usize = 64;
const MAX_ITEMS: usize = 1024;

#[derive(Clone, Copy)]
enum K {
    /// Authored stable identity: non-empty, at most 128 UTF-8 bytes, no control character.
    Uid,
    /// Opaque layer identity: any non-empty string.
    Opaque,
    /// One to 1024 distinct opaque layer identities.
    Opaques,
    Gap,
    Selection,
    Name,
    GroupName,
    Frame,
    /// Stroke identities: at least this many, at most 1024.
    Strokes(usize),
    Combine,
    Boolean,
    EditMode,
    Labels,
    Clip,
    Footprint,
    Fill(fill::Field),
}

/// (operation suffix, command, "member:kind ..." in wire order). Every operation is a revisioned command; only a
/// duplicate names its minted identity; copy/cut carry a bounded clipboard; other receipts are compact.
static SPECS: [(&str, &str, &str); 21] = [
    ("layer.delete", "object.layer.delete", "selection:selection"),
    (
        "layer.duplicate",
        "object.layer.duplicate",
        "layerUid:opaque",
    ),
    (
        "layer.rename",
        "object.layer.rename",
        "layerUid:opaque name:name",
    ),
    (
        "layer.reorder",
        "object.layer.reorder",
        "layerUids:opaques gap:gap selection:selection",
    ),
    (
        "group.create",
        "object.group.create",
        "layerUid:uid contextId:uid groupId:uid name:groupName strokeIds:strokes1",
    ),
    (
        "group.ungroup",
        "object.group.ungroup",
        "layerUid:uid contextId:uid strokeIds:strokes1",
    ),
    (
        "group.combine",
        "object.group.combine",
        "layerUid:uid contextId:uid groupId:uid mode:combine strokeIds:strokes1",
    ),
    (
        "group.mode.set",
        "object.group.mode.set",
        "layerUid:uid contextId:uid groupId:uid mode:combine",
    ),
    (
        "group.rename",
        "object.group.rename",
        "layerUid:uid contextId:uid groupId:uid name:groupName",
    ),
    (
        "group.member.remove",
        "object.group.member.remove",
        "layerUid:uid contextId:uid strokeId:uid",
    ),
    (
        "selection.delete",
        "object.selection.delete",
        "contextId:uid layerUid:uid frame:frame mode:editMode strokeIds:strokes0",
    ),
    (
        "selection.duplicate",
        "object.selection.duplicate",
        "contextId:uid layerUid:uid frame:frame mode:editMode strokeIds:strokes0",
    ),
    (
        "layer.split",
        "object.layer.split",
        "contextId:uid layerUid:uid frame:frame labels:labels",
    ),
    (
        "clipboard.paste",
        "object.clipboard.paste",
        "contextId:uid layerUid:uid frame:frame mode:editMode clip:clip",
    ),
    (
        "boolean.apply",
        "object.boolean.apply",
        "mode:boolean layerUid:uid contextId:uid frame:frame strokeIds:strokes2",
    ),
    (
        "boolean.erase",
        "object.boolean.erase",
        "layerUid:uid contextId:uid frame:frame strokeId:uid footprint:footprint",
    ),
    (
        "boolean.flatten",
        "object.boolean.flatten",
        "layerUid:uid contextId:uid frame:frame groupId:uid",
    ),
    ("fill.trace","object.fill.trace","layerUid:uid contextId:uid frame:frame point:point color:color opacity:opacity remove:flag"),
    ("fill.brush","object.fill.brush","layerUid:uid contextId:uid frame:frame outline:outline color:color opacity:opacity placement:placement"),
    ("clipboard.copy", "object.clipboard.copy", "contextId:uid layerUid:uid frame:frame mode:editMode strokeIds:strokes0"),
    ("clipboard.cut", "object.clipboard.cut", "contextId:uid layerUid:uid frame:frame mode:editMode strokeIds:strokes0"),
];
const PREFIX: &str = "command.document.object.";

fn spec(operation: &str) -> Option<(&'static str, &'static str)> {
    let suffix = operation.strip_prefix(PREFIX)?;
    SPECS
        .iter()
        .find(|row| row.0 == suffix)
        .map(|row| (row.1, row.2))
}

pub(crate) fn admitted(operation: &str) -> bool {
    operation == object::subselect::EDIT || spec(operation).is_some()
}

/// Rust `char::is_control` is the Unicode Cc class, the same set the owner refuses.
fn text_ok(text: &str, limit: usize) -> bool {
    !text.is_empty() && text.len() <= limit && !text.chars().any(char::is_control)
}

fn uid(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text_ok(text, MAX_ID))
}

fn opaque(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

fn finite(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}

fn frame(value: &Value) -> bool {
    value
        .as_u64()
        .is_some_and(|frame| frame <= u64::from(u32::MAX))
}

fn distinct_opaque(items: &[Value]) -> bool {
    let mut seen = HashSet::new();
    for item in items {
        match item.as_str() {
            Some(text) if !text.is_empty() && seen.insert(text) => {}
            _ => return false,
        }
    }
    true
}

fn selection(value: &Value) -> bool {
    exact(value, &["active", "selected"], &["anchor"]).is_some_and(|object| {
        object.get("active").is_some_and(opaque)
            && object
                .get("selected")
                .and_then(Value::as_array)
                .is_some_and(|items| items.len() <= MAX_ITEMS && items.iter().all(opaque))
            && object
                .get("anchor")
                .is_none_or(|anchor| anchor.is_null() || opaque(anchor))
    })
}

fn labels(value: &Value) -> bool {
    exact(value, &["shape", "stroke"], &[]).is_some_and(|object| {
        ["shape", "stroke"].iter().all(|key| {
            object
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|text| text_ok(text, MAX_LABEL))
        })
    })
}

fn footprint(value: &Value) -> bool {
    value.as_array().is_some_and(|points| {
        (3..=MAX_ITEMS).contains(&points.len())
            && points.iter().all(|point| {
                point
                    .as_array()
                    .is_some_and(|pair| pair.len() == 2 && pair.iter().all(finite))
            })
    })
}

fn kind(token: &str) -> Option<K> {
    Some(match token {
        "uid" => K::Uid,
        "opaque" => K::Opaque,
        "opaques" => K::Opaques,
        "gap" => K::Gap,
        "selection" => K::Selection,
        "name" => K::Name,
        "groupName" => K::GroupName,
        "frame" => K::Frame,
        "strokes0" => K::Strokes(0),
        "strokes1" => K::Strokes(1),
        "strokes2" => K::Strokes(2),
        "combine" => K::Combine,
        "boolean" => K::Boolean,
        "editMode" => K::EditMode,
        "labels" => K::Labels,
        "clip" => K::Clip,
        "footprint" => K::Footprint,
        _ => K::Fill(fill::kind(token)?),
    })
}

fn check(kind: K, value: &Value) -> bool {
    match kind {
        K::Uid => uid(value),
        K::Opaque => opaque(value),
        K::Opaques => value
            .as_array()
            .is_some_and(|items| (1..=MAX_ITEMS).contains(&items.len()) && distinct_opaque(items)),
        K::Gap => finite(value),
        K::Selection => selection(value),
        K::Name => value.is_string(),
        K::GroupName => value.as_str().is_some_and(|text| text_ok(text, MAX_NAME)),
        K::Frame => frame(value),
        K::Strokes(min) => value
            .as_array()
            .is_some_and(|items| (min..=MAX_ITEMS).contains(&items.len()) && items.iter().all(uid)),
        K::Combine => value.as_str().is_some_and(|mode| {
            matches!(
                mode,
                "none" | "unite" | "subtract" | "intersect" | "exclude"
            )
        }),
        K::Boolean => value
            .as_str()
            .is_some_and(|mode| matches!(mode, "unite" | "subtract" | "intersect" | "exclude")),
        K::EditMode => value
            .as_str()
            .is_some_and(|mode| matches!(mode, "animation2d" | "motion")),
        K::Labels => labels(value),
        K::Clip => clipboard::clip(value),
        K::Footprint => footprint(value),
        K::Fill(field) => fill::check(field, value),
    }
}

pub(super) fn request(operation: &str, payload: &Value) -> bool {
    if operation == object::subselect::EDIT {
        return object::subselect::edit_request(payload);
    }
    let Some((command, members)) = spec(operation) else {
        return false;
    };
    let members: Vec<(&str, &str)> = members
        .split(' ')
        .filter_map(|member| member.split_once(':'))
        .collect();
    let mut required = vec!["command"];
    required.extend(members.iter().map(|member| member.0));
    exact(payload, &required, &[]).is_some_and(|object| {
        object.get("command").and_then(Value::as_str) == Some(command)
            && members.iter().all(|(name, token)| {
                matches!(
                    (object.get(*name), kind(token)),
                    (Some(value), Some(k)) if check(k, value)
                )
            })
    })
}

pub(super) fn result(operation: &str, value: &Value) -> bool {
    if !admitted(operation) {
        return false;
    }
    if operation.ends_with(".layer.duplicate") {
        object::create_result(value)
    } else if clipboard::operation(operation) {
        clipboard::result(operation, value)
    } else {
        object::visibility_result(value)
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
    if !admitted(&request.operation) {
        return true;
    }
    response.instance_id == request.instance_id
        && response.document_id == request.document_id
        && response.result.as_ref().is_some_and(|value| {
            revision_matches(request.expected_revision, response.content_revision, value)
                && clipboard::correlated(request, value)
        })
}

#[cfg(test)]
#[path = "native_object_edit_contract_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_edit_receipt_tests.rs"]
mod receipt_tests;
