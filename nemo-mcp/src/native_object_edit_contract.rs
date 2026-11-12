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
    /// An Alt-drag duplicate's distance: a finite `[dx, dy]` pair, each component at most 1e6.
    Offset,
    /// A multi-layer duplicate's other layers: one to 32 `{layerUid, strokeIds}` with distinct layers and one or more distinct stroke identities each.
    Others,
    Fill(fill::Field),
}

/// Operation suffix, command and typed fields in wire order (the bool marks optional fields).
/// Every operation is a revisioned command; only a
/// duplicate names its minted identity; copy/cut carry a bounded clipboard; other receipts are compact.
type Member = (&'static str, K, bool);
static SPECS: [(&str, &str, &[Member]); 21] = [
    (
        "layer.delete",
        "object.layer.delete",
        &[
            ("selection", K::Selection, false),
            ("contextId", K::Uid, true),
        ],
    ),
    (
        "layer.duplicate",
        "object.layer.duplicate",
        &[
            ("layerUid", K::Opaque, false),
            ("contextId", K::Uid, true),
        ],
    ),
    (
        "layer.rename",
        "object.layer.rename",
        &[
            ("layerUid", K::Opaque, false),
            ("name", K::Name, false),
        ],
    ),
    (
        "layer.reorder",
        "object.layer.reorder",
        &[
            ("layerUids", K::Opaques, false),
            ("gap", K::Gap, false),
            ("selection", K::Selection, false),
            ("contextId", K::Uid, true),
        ],
    ),
    (
        "group.create",
        "object.group.create",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("name", K::GroupName, false),
            ("strokeIds", K::Strokes(1), false),
        ],
    ),
    (
        "group.ungroup",
        "object.group.ungroup",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("strokeIds", K::Strokes(1), false),
        ],
    ),
    (
        "group.combine",
        "object.group.combine",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("mode", K::Combine, false),
            ("strokeIds", K::Strokes(1), false),
        ],
    ),
    (
        "group.mode.set",
        "object.group.mode.set",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("mode", K::Combine, false),
        ],
    ),
    (
        "group.rename",
        "object.group.rename",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("name", K::GroupName, false),
        ],
    ),
    (
        "group.member.remove",
        "object.group.member.remove",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("strokeId", K::Uid, false),
        ],
    ),
    (
        "selection.delete",
        "object.selection.delete",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
        ],
    ),
    (
        "selection.duplicate",
        "object.selection.duplicate",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
            ("offset", K::Offset, true),
            ("others", K::Others, true),
        ],
    ),
    (
        "layer.split",
        "object.layer.split",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("labels", K::Labels, false),
        ],
    ),
    (
        "clipboard.paste",
        "object.clipboard.paste",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("clip", K::Clip, false),
        ],
    ),
    (
        "boolean.apply",
        "object.boolean.apply",
        &[
            ("mode", K::Boolean, false),
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("strokeIds", K::Strokes(2), false),
        ],
    ),
    (
        "boolean.erase",
        "object.boolean.erase",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("strokeId", K::Uid, false),
            ("footprint", K::Footprint, false),
            ("joinHistory", K::Fill(fill::Field::Flag), true),
        ],
    ),
    (
        "boolean.flatten",
        "object.boolean.flatten",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("groupId", K::Uid, false),
        ],
    ),
    (
        "fill.trace",
        "object.fill.trace",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("point", K::Fill(fill::Field::Point), false),
            ("color", K::Fill(fill::Field::Color), false),
            ("opacity", K::Fill(fill::Field::Opacity), false),
            ("remove", K::Fill(fill::Field::Flag), false),
        ],
    ),
    (
        "fill.brush",
        "object.fill.brush",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("outline", K::Fill(fill::Field::Outline), false),
            ("color", K::Fill(fill::Field::Color), false),
            ("opacity", K::Fill(fill::Field::Opacity), false),
            ("placement", K::Fill(fill::Field::Placement), false),
            ("mask", K::Fill(fill::Field::Mask), false),
            ("copies", K::Fill(fill::Field::Copies), false),
            ("stamps", K::Fill(fill::Field::Stamps), false),
        ],
    ),
    (
        "clipboard.copy",
        "object.clipboard.copy",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
        ],
    ),
    (
        "clipboard.cut",
        "object.clipboard.cut",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
        ],
    ),
];
const PREFIX: &str = "command.document.object.";

fn spec(operation: &str) -> Option<(&'static str, &'static [Member])> {
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

fn offset(value: &Value) -> bool {
    value.as_array().is_some_and(|pair| {
        pair.len() == 2 && pair.iter().all(|v| finite(v) && v.as_f64().is_some_and(|n| n.abs() <= 1.0e6))
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
        K::Offset => offset(value),
        K::Others => clipboard::others(value),
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
    // The field kinds are compiler-checked constants. Missing required keys and
    // unknown keys are rejected before member checks; only declared optional
    // keys may be absent. No request can introduce an unknown field-kind token.
    let mut required = vec!["command"];
    required.extend(members.iter().filter(|member| !member.2).map(|member| member.0));
    let allowed: Vec<_> = members.iter().filter(|member| member.2).map(|member| member.0).collect();
    exact(payload, &required, &allowed).is_some_and(|object| {
        object.get("command").and_then(Value::as_str) == Some(command)
            && members.iter().all(|(name, kind, optional)| match object.get(*name) {
                Some(value) => check(*kind, value),
                None => *optional,
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

/// The receipt of `operation` for the request `payload` it answers: a joined eraser bite (`joinHistory`) continues the
/// drag's undo entry, so it adds none; every other receipt is `result`.
pub(super) fn result_for(operation: &str, payload: &Value, value: &Value) -> bool {
    let joined = operation.ends_with(".boolean.erase") && payload.get("joinHistory").and_then(Value::as_bool) == Some(true);
    !joined && result(operation, value)
        || joined && exact(value, &["applied", "historyEntriesAdded"], &[]).is_some_and(|v| v["applied"].is_boolean() && v["historyEntriesAdded"] == 0)
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
                && result_for(&request.operation, &request.payload, value)
                && clipboard::correlated(request, value)
        })
}

#[cfg(test)]
#[path = "native_object_edit_contract_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_edit_receipt_tests.rs"]
mod receipt_tests;
