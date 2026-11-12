//! Pinned raw Draw symmetry metadata and the native-computed effective settings.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Selector {
    Settings {},
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
fn axis((x1, y1, x2, y2): (f64, f64, f64, f64)) -> Value {
    json!({"x1": x1, "y1": y1, "x2": x2, "y2": y2})
}
fn point((x, y): (f64, f64)) -> Value {
    json!({"x": x, "y": y})
}
impl ObjectSnapshot {
    /// Original symmetry-bridge defaultAxis from admitted native canvas dimensions;
    /// any mode other than 'x' or 'free' takes the vertical 'y' axis.
    pub(crate) fn symmetry_default_axis(
        (width, height): (u32, u32),
        mode: Option<&str>,
    ) -> (f64, f64, f64, f64) {
        let (w, h) = (f64::from(width), f64::from(height));
        let (cx, cy) = (w / 2., h / 2.);
        match mode {
            Some("x") => (0., cy, w, cy),
            Some("free") => {
                let r = w.min(h) * 0.4;
                (cx - r, cy - r, cx + r, cy + r)
            }
            _ => (cx, 0., cx, h),
        }
    }
    pub(crate) fn symmetry_default_center((width, height): (u32, u32)) -> (f64, f64) {
        (f64::from(width) / 2., f64::from(height) / 2.)
    }
    pub(crate) fn query_symmetry_metadata_json(&self, bytes: &[u8]) -> Result<Value, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if !shape.get("selector").is_some_and(Value::is_object) {
            return Err(Error::InvalidRequest);
        }
        let query: Query = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        let Selector::Settings {} = query.selector;
        if query.command != "object.project.symmetry.read" {
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
        let dimensions = metadata.canvas_dimensions();
        if dimensions.0 == 0 || dimensions.1 == 0 {
            return Err(Error::Unavailable);
        }
        let stored = metadata.symmetry_metadata();
        // The original importJSON: enabled||false, mode||'y', axis/centre||null (then
        // ensured from the canvas), sectors||6 and extend defaulting to true.
        let mode = stored.mode.as_deref().filter(|m| !m.is_empty()).unwrap_or("y");
        let sectors = stored
            .radial_sectors
            .filter(|n| *n != 0. && !n.is_nan())
            .unwrap_or(6.);
        let mut result = json!({
            "effective": {
                "enabled": stored.enabled.unwrap_or(false),
                "mode": mode,
                "axis": axis(stored.axis.flatten().unwrap_or_else(|| {
                    Self::symmetry_default_axis(dimensions, Some(mode))
                })),
                "radialCenter": point(stored.radial_center.flatten().unwrap_or_else(|| {
                    Self::symmetry_default_center(dimensions)
                })),
                "radialSectors": sectors,
                "extend": stored.extend.unwrap_or(true)
            }
        });
        if let Some(value) = stored.enabled {
            result["symmetryEnabled"] = json!(value);
        }
        if let Some(value) = &stored.mode {
            result["symmetryMode"] = json!(value);
        }
        if let Some(value) = stored.axis {
            result["symmetryAxis"] = value.map_or(Value::Null, axis);
        }
        if let Some(value) = stored.radial_center {
            result["symmetryRadialCenter"] = value.map_or(Value::Null, point);
        }
        if let Some(value) = stored.radial_sectors {
            result["symmetryRadialSectors"] = json!(value);
        }
        if let Some(value) = stored.extend {
            result["symmetryExtend"] = json!(value);
        }
        let response = json!({
            "instanceId": self.instance_id(),
            "documentId": self.document_id(),
            "atRevision": self.content_revision(),
            "documentSnapshotId": self.snapshot_id(),
            "contextId": "scene-root",
            "result": result
        });
        if serde_json::to_vec(&response).map_err(|_| Error::Internal)?.len() > 4096 {
            return Err(Error::Unavailable);
        }
        Ok(response)
    }
    /// Common dispatcher admits the complete envelope before selecting this wrapper.
    pub(crate) fn dispatch_symmetry_read(
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
                "Invalid symmetry read payload.",
            ));
        }
        let query = json!({
            "command": "object.project.symmetry.read",
            "instanceId": request.instance_id,
            "documentId": request.document_id,
            "atRevision": request.payload["atRevision"],
            "documentSnapshotId": request.payload["documentSnapshotId"],
            "contextId": request.payload["contextId"],
            "selector": request.payload["selector"]
        });
        let bytes = serde_json::to_vec(&query).map_err(|_| Error::InvalidRequest)?;
        match self.query_symmetry_metadata_json(&bytes) {
            Ok(result) => {
                let response = ResponseEnvelope {
                    api_version: request.api_version,
                    request_id: request.request_id.clone(),
                    instance_id: self.instance_id().into(),
                    document_id: self.document_id().into(),
                    content_revision: self.content_revision(),
                    ok: true,
                    result: Some(result),
                    error: None,
                };
                if serde_json::to_vec(&response).map_err(|_| Error::Internal)?.len() > 4096 {
                    return Ok(self.read_failure(request, Error::Unavailable, "Symmetry read exceeds its response budget."));
                }
                Ok(response)
            }
            Err(code) => Ok(self.read_failure(
                request,
                code,
                "Symmetry read is invalid, unavailable or stale.",
            )),
        }
    }
}
