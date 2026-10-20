//! Pinned raw Perspective metadata and native-computed effective VP projection.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Selector {
    Settings {},
    VpPage { cursor: u64, limit: u64 },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Query {
    command: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    selector: Selector,
}
fn encoded(value: &Value) -> Result<usize, Error> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| Error::Internal)
}
impl ObjectSnapshot {
    /// Pure original defaultVPs formula, shared by explicit edits and projections.
    /// Dimensions come only from admitted native metadata, never a caller array.
    pub(crate) fn perspective_default_vps(
        dimensions: (u32, u32),
        mode: Option<&str>,
    ) -> Result<Vec<(f64, f64, Option<bool>)>, Error> {
        let (width, height) = dimensions;
        if width == 0 || height == 0 {
            return Err(Error::Unavailable);
        }
        let (w, h) = (f64::from(width), f64::from(height));
        let (cx, cy) = (w / 2., h / 2.);
        Ok(match mode {
            Some("1pt" | "fisheye") => vec![(cx, cy, Some(false))],
            Some("3pt") => vec![
                (cx - w * 0.6, cy - h * 0.15, Some(false)),
                (cx + w * 0.6, cy - h * 0.15, Some(false)),
                (cx, cy + h * 1.3, Some(false)),
            ],
            _ => vec![
                (cx - w * 0.65, cy, Some(false)),
                (cx + w * 0.65, cy, Some(false)),
            ],
        })
    }
    pub(crate) fn query_perspective_metadata_json(&self, bytes: &[u8]) -> Result<Value, Error> {
        self.perspective_read_with_budget(bytes, 4096)
    }
    fn perspective_read_with_budget(&self, bytes: &[u8], budget: usize) -> Result<Value, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if !shape.get("selector").is_some_and(Value::is_object) {
            return Err(Error::InvalidRequest);
        }
        let query: Query = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if query.command != "object.project.perspective.read" {
            return Err(Error::InvalidRequest);
        }
        if query.instance_id != self.instance_id() {
            return Err(Error::WrongInstance);
        }
        if query.document_id != self.document_id() {
            return Err(Error::WrongDocument);
        }
        if query.at_revision != self.content_revision()
            || query.document_snapshot_id != self.snapshot_id()
        {
            return Err(Error::StaleRevision);
        }
        if query.context_id != "scene-root" {
            return Err(Error::Unavailable);
        }
        let metadata = self.project().ok_or(Error::Unavailable)?.project();
        let (enabled, mode, density, supplied) = metadata.perspective_metadata();
        let stored = supplied.flatten().unwrap_or_default();
        let defaults;
        let points = if stored.is_empty() {
            defaults = Self::perspective_default_vps(metadata.canvas_dimensions(), mode)?;
            defaults.as_slice()
        } else {
            stored
        };
        let mut result = json!({
            "perspectiveVPsPresent": supplied.is_some(),
            "perspectiveVPsNull": matches!(supplied, Some(None)),
            "perspectiveVPCount": stored.len(),
            "effectiveVPCount": points.len(),
            "vpSource": if stored.is_empty() { "default" } else { "stored" }
        });
        match query.selector {
            Selector::Settings {} => {
                if let Some(value) = enabled {
                    result["perspectiveEnabled"] = json!(value);
                }
                if let Some(value) = mode {
                    result["perspectiveMode"] = json!(value);
                }
                if let Some(value) = density {
                    result["perspectiveDensity"] = json!(value);
                }
            }
            Selector::VpPage { cursor, limit } => {
                if cursor > 9_007_199_254_740_991 || !(1..=16).contains(&limit) {
                    return Err(Error::InvalidRequest);
                }
                let start = usize::try_from(cursor).map_err(|_| Error::InvalidRequest)?;
                if start > points.len() {
                    return Err(Error::InvalidRequest);
                }
                result["kind"] = json!("perspectiveVPPage");
                result["cursor"] = json!(cursor);
                result["items"] = json!([]);
                result["nextCursor"] = Value::Null;
                let mut items = Vec::new();
                for at in (start..points.len()).take(limit as usize) {
                    let (x, y, locked) = points[at];
                    let mut item = json!({"ordinal": at + 1, "x": x, "y": y});
                    if let Some(value) = locked {
                        item["locked"] = json!(value);
                    }
                    items.push(item);
                    result["items"] = json!(items);
                    result["nextCursor"] = if at + 1 < points.len() {
                        json!(at + 1)
                    } else {
                        Value::Null
                    };
                    if encoded(&self.perspective_response(result.clone()))? > budget {
                        items.pop();
                        if items.is_empty() {
                            return Err(Error::Unavailable);
                        }
                        result["items"] = json!(items);
                        result["nextCursor"] = json!(at);
                        break;
                    }
                }
            }
        }
        let response = self.perspective_response(result);
        if encoded(&response)? > budget {
            return Err(Error::Unavailable);
        }
        Ok(response)
    }
    fn perspective_response(&self, result: Value) -> Value {
        json!({
            "instanceId": self.instance_id(),
            "documentId": self.document_id(),
            "atRevision": self.content_revision(),
            "documentSnapshotId": self.snapshot_id(),
            "contextId": "scene-root",
            "result": result
        })
    }
    /// Common dispatcher admits the complete envelope before selecting this wrapper.
    pub(crate) fn dispatch_perspective_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, Error> {
        let fields = ["atRevision", "documentSnapshotId", "contextId", "selector"];
        if request.payload.as_object().is_none_or(|value| {
            value.len() != fields.len() || fields.iter().any(|key| !value.contains_key(*key))
        }) {
            return Ok(self.read_failure(
                request,
                Error::InvalidRequest,
                "Invalid Perspective read payload.",
            ));
        }
        let query = json!({
            "command": "object.project.perspective.read",
            "instanceId": request.instance_id,
            "documentId": request.document_id,
            "atRevision": request.payload["atRevision"],
            "documentSnapshotId": request.payload["documentSnapshotId"],
            "contextId": request.payload["contextId"],
            "selector": request.payload["selector"]
        });
        let mut response = ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(Value::Null),
            error: None,
        };
        let overhead = serde_json::to_vec(&response)
            .map_err(|_| Error::Internal)?
            .len()
            - 4;
        let budget = 4096usize.checked_sub(overhead).ok_or(Error::Unavailable)?;
        match self.perspective_read_with_budget(
            &serde_json::to_vec(&query).map_err(|_| Error::InvalidRequest)?,
            budget,
        ) {
            Ok(result) => {
                response.result = Some(result);
                Ok(response)
            }
            Err(code) => Ok(self.read_failure(
                request,
                code,
                "Perspective read is invalid, unavailable, stale or not found.",
            )),
        }
    }
}
