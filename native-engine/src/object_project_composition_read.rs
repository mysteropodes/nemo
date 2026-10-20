//! Pinned root composition reads; private selectors retain their original semantics.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

// D26-P1: detached root-composition reads share the original immutable snapshot.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum CompositionSelector {
    Settings {},
    MarkerAt { index: usize },
    MarkerPage { cursor: usize, limit: usize },
    MarkerNamed { name: String },
    MarkerNearest { frame: f64 },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CompositionRead {
    command: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    selector: CompositionSelector,
}
impl ObjectSnapshot {
    /// Original-byte, bounded query only. No dispatcher/public capability activation.
    pub(crate) fn query_composition_metadata_json(
        &self,
        bytes: &[u8],
    ) -> Result<serde_json::Value, DispatchErrorCode> {
        self.composition_read_with_budget(bytes, 4096)
    }
    fn composition_read_with_budget(
        &self,
        bytes: &[u8],
        budget: usize,
    ) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision, Unavailable};
        if bytes.len() > 4096 {
            return Err(InvalidRequest);
        }
        let shape: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if !shape
            .get("selector")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(InvalidRequest);
        }
        let query: CompositionRead = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if query.command != "object.project.composition.read" {
            return Err(InvalidRequest);
        }
        if query.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if query.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if query.at_revision != self.content_revision()
            || query.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        if query.context_id != "scene-root" {
            return Err(Unavailable);
        }
        match &query.selector {
            CompositionSelector::MarkerAt { index: 0 } => return Err(InvalidRequest),
            CompositionSelector::MarkerPage { cursor, limit }
                if *cursor > 4096 || !(1..=16).contains(limit) => return Err(InvalidRequest),
            CompositionSelector::MarkerNamed { name }
                if name.len() > 1024 || name.contains('\0') =>
            {
                return Err(InvalidRequest)
            }
            CompositionSelector::MarkerNearest { frame } if !frame.is_finite() => {
                return Err(InvalidRequest);
            }
            _ => {}
        }
        let metadata = self.project().ok_or(Unavailable)?.project();
        let markers = metadata.composition_markers();
        let result = match query.selector {
            CompositionSelector::Settings {} => {
                let (bpm, offset, show) = metadata.composition_bpm();
                let mut value = serde_json::json!({
                    "markersPresent": markers.is_some(),
                    "markerCount": markers.map_or(0, |values| values.len()),
                    "effectiveBpm": bpm.unwrap_or(120.0),
                    "effectiveBpmOffset": offset.unwrap_or(0.0),
                    "effectiveBpmShow": show.unwrap_or(false)
                });
                if let Some(v) = bpm {
                    value["bpm"] = v.into();
                }
                if let Some(v) = offset {
                    value["bpmOffset"] = v.into();
                }
                if let Some(v) = show {
                    value["bpmShow"] = v.into();
                }
                value
            }
            CompositionSelector::MarkerPage { cursor, limit } => {
                let values = markers.unwrap_or_default();
                if cursor > values.len() { return Err(InvalidRequest); }
                let mut page = json!({"kind":"markerPage", "markersPresent":markers.is_some(),
                    "markerCount":values.len(), "cursor":cursor, "items":[], "nextCursor":null});
                let mut items = Vec::new();
                for (at, (frame, name, color)) in values.iter().enumerate().skip(cursor).take(limit) {
                    let mut item = json!({"ordinal":at + 1, "frame":frame});
                    if let Some(name) = name { item["name"] = name.clone().into(); }
                    if let Some(color) = color { item["color"] = color.clone().into(); }
                    items.push(item);
                    page["items"] = json!(items);
                    page["nextCursor"] = if at + 1 < values.len() { json!(at + 1) } else { Value::Null };
                    if encoded_len(&self.composition_response(page.clone()))? > budget {
                        items.pop();
                        if items.is_empty() { return Err(Unavailable); }
                        page["items"] = json!(items);
                        page["nextCursor"] = json!(at);
                        break;
                    }
                }
                page
            }
            selector => {
                let values = markers.ok_or(NotFound)?;
                let index = match selector {
                    CompositionSelector::MarkerAt { index } => index
                        .checked_sub(1)
                        .filter(|i| *i < values.len())
                        .ok_or(NotFound)?,
                    CompositionSelector::MarkerNamed { name } => values
                        .iter()
                        .position(|m| m.1.as_deref().unwrap_or("") == name)
                        .ok_or(NotFound)?,
                    CompositionSelector::MarkerNearest { frame } => {
                        if values.is_empty() {
                            return Err(NotFound);
                        }
                        // Match first-stored JS ties, including all-overflow distances.
                        let mut best = 0;
                        let mut distance = f64::INFINITY;
                        for (i, marker) in values.iter().enumerate() {
                            let candidate = (marker.0 - frame).abs();
                            if candidate < distance {
                                best = i;
                                distance = candidate;
                            }
                        }
                        best
                    }
                    CompositionSelector::Settings {} | CompositionSelector::MarkerPage { .. } => unreachable!(),
                };
                let (frame, name, color) = &values[index];
                let time = frame / metadata.fps();
                if !time.is_finite() {
                    return Err(Unavailable);
                }
                serde_json::json!({"index": index + 1, "frame": frame, "time": time,
                    "name": name.as_deref().unwrap_or(""),
                    "color": color.as_deref().filter(|v| !v.is_empty())})
            }
        };
        let response = self.composition_response(result);
        if encoded_len(&response)? > budget { return Err(Unavailable); }
        Ok(response)
    }
    fn composition_response(&self, result: Value) -> Value {
        json!({"instanceId":self.instance_id(), "documentId":self.document_id(),
            "atRevision":self.content_revision(), "documentSnapshotId":self.snapshot_id(),
            "contextId":"scene-root", "result":result})
    }

    /// Called only after the shared snapshot dispatcher has admitted its envelope.
    pub(crate) fn dispatch_composition_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use DispatchErrorCode::{Internal, InvalidRequest, Unavailable};
        let fields = ["atRevision", "documentSnapshotId", "contextId", "selector"];
        if request.payload.as_object().is_none_or(|p|
            p.len() != fields.len() || fields.iter().any(|k| !p.contains_key(*k))) {
            return Ok(self.read_failure(request, InvalidRequest, "Invalid composition read payload."));
        }
        let query = json!({"command":"object.project.composition.read",
            "instanceId":request.instance_id, "documentId":request.document_id,
            "atRevision":request.payload["atRevision"],
            "documentSnapshotId":request.payload["documentSnapshotId"],
            "contextId":request.payload["contextId"], "selector":request.payload["selector"]});
        let mut response = ResponseEnvelope {
            api_version:request.api_version, request_id:request.request_id.clone(),
            instance_id:self.instance_id().into(), document_id:self.document_id().into(),
            content_revision:self.content_revision(), ok:true, result:Some(Value::Null), error:None,
        };
        // Charge the actual complete envelope, including escaped correlation IDs.
        let overhead = serde_json::to_vec(&response).map_err(|_| Internal)?.len() - 4;
        let budget = 4096usize.checked_sub(overhead).ok_or(Unavailable)?;
        match self.composition_read_with_budget(&serde_json::to_vec(&query).map_err(|_| InvalidRequest)?, budget) {
            Ok(value) => response.result = Some(value),
            Err(code) => return Ok(self.read_failure(request, code, "Pinned composition read is invalid or unavailable.")),
        }
        if serde_json::to_vec(&response).map_err(|_| Internal)?.len() > 4096 {
            return Ok(self.read_failure(request, Unavailable, "Complete composition read exceeds 4096 bytes."));
        }
        Ok(response)
    }
}
fn encoded_len(value: &Value) -> Result<usize, DispatchErrorCode> {
    serde_json::to_vec(value).map(|bytes| bytes.len()).map_err(|_| DispatchErrorCode::Unavailable)
}
#[cfg(test)]
#[path = "../tests/object_composition_read.rs"]
mod tests;
