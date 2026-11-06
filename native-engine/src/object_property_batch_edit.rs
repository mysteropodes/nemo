//! #2520 / #2525 (C-motion-canvas-4): several Motion value edits as ONE candidate, hence one revision and one history entry.
//!
//! Baseline `motion.js` `multiLayerMove` / `multiLayerScale` / `multiLayerRotate` and a multi-element `elementMove` (@1065e9d8):
//! one gesture on a box over several layers (or shapes) writes each one's Position / Scale / Rotation. Here every entry is an
//! ordinary `object.property.edit` (`setValue` or `setKey` only) chained over one working copy of the document by the single
//! property-edit owner, so applicability, locks, dimensions and the static-versus-keyed decision are the same as for a lone edit
//! and a refusal of any entry publishes nothing. Two entries for one holder property are refused: the gesture owns one value each.
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::object_deserialize;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Value};

type R<T> = Result<T, DispatchErrorCode>;
const MAX_BYTES: usize = 65_536;
const MAX_EDITS: usize = 64;

#[derive(Debug)]
struct Payload {
    command: String,
    edits: Vec<Value>,
}
object_deserialize!(Payload {
    command: String,
    edits: Vec<Value>
});

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
/// An entry is exactly `{target, property, action}` with a `setValue` / `setKey` action; the single-edit owner validates the rest.
fn entry(value: &Value) -> R<Vec<u8>> {
    let object = value.as_object().ok_or_else(invalid)?;
    let action = object.get("action").and_then(Value::as_object).ok_or_else(invalid)?;
    if object.len() != 3
        || !["target", "property", "action"].iter().all(|k| object.contains_key(*k))
        || !matches!(action.get("kind").and_then(Value::as_str), Some("setValue" | "setKey"))
        || action.len() != 3
    {
        return Err(invalid());
    }
    serde_json::to_vec(&json!({
        "command": "object.property.edit",
        "target": object["target"],
        "property": object["property"],
        "action": object["action"],
    }))
    .map_err(|_| invalid())
}

impl ObjectFillOwner {
    pub(crate) fn prepare_property_batch_edit_json(&self, bytes: &[u8]) -> R<PreparedPropertyEdit> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid());
        }
        let payload: Payload = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if payload.command != "object.property.batch"
            || payload.edits.is_empty()
            || payload.edits.len() > MAX_EDITS
        {
            return Err(invalid());
        }
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let mut seen: Vec<(Value, Value)> = Vec::new();
        for edit in &payload.edits {
            let single = entry(edit)?;
            let key = (edit["target"].clone(), edit["property"].clone());
            if seen.contains(&key) {
                return Err(invalid());
            }
            seen.push(key);
            document =
                Self::apply_property_edit_json(origin.clone(), document, &single)?.into_document();
        }
        Ok(PreparedPropertyEdit::sealed(origin, document))
    }
}
