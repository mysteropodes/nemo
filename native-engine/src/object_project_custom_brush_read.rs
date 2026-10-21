//! Pinned ordered project custom catalog; no built-in lookup or brush execution.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Selector {
    Settings {},
    CatalogPage { cursor: u64, limit: u64 },
    Record { id: String },
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
    pub(crate) fn query_custom_brush_metadata_json(&self, bytes: &[u8]) -> Result<Value, Error> {
        self.custom_brush_read_with_budget(bytes, 4096)
    }
    fn custom_brush_read_with_budget(&self, bytes: &[u8], budget: usize) -> Result<Value, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if !shape.get("selector").is_some_and(Value::is_object) {
            return Err(Error::InvalidRequest);
        }
        let query: Query = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if query.command != "object.project.customBrushPreset.read" {
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
        let catalog = self
            .project()
            .ok_or(Error::Unavailable)?
            .project()
            .custom_brush_presets();
        let entries = catalog.map_or(&[][..], |catalog| catalog.entries());
        let result = match query.selector {
            Selector::Settings {} => {
                json!({"customBrushPresetsPresent":catalog.is_some(),"presetCount":entries.len()})
            }
            Selector::Record { id } => {
                let (_, preset) = entries.iter().find(|v| v.0 == id).ok_or(Error::NotFound)?;
                json!({"kind":"customBrushPresetRecord","id":id,"preset":preset})
            }
            Selector::CatalogPage { cursor, limit } => {
                if cursor > 9_007_199_254_740_991 || !(1..=16).contains(&limit) {
                    return Err(Error::InvalidRequest);
                }
                let start = usize::try_from(cursor).map_err(|_| Error::InvalidRequest)?;
                if start > entries.len() {
                    return Err(Error::InvalidRequest);
                }
                let mut page = json!({"kind":"customBrushPresetPage","customBrushPresetsPresent":catalog.is_some(),
                    "presetCount":entries.len(),"cursor":cursor,"items":[],"nextCursor":null});
                let mut items = Vec::new();
                for at in (start..entries.len()).take(limit as usize) {
                    let (id, preset) = &entries[at];
                    let mut item = json!({"ordinal":at+1,"id":id});
                    if let Some(label) = preset.label() {
                        item["label"] = json!(label);
                    }
                    items.push(item);
                    page["items"] = json!(items);
                    page["nextCursor"] = if at + 1 < entries.len() {
                        json!(at + 1)
                    } else {
                        Value::Null
                    };
                    if encoded(&self.custom_brush_response(page.clone()))? > budget {
                        items.pop();
                        if items.is_empty() {
                            return Err(Error::Unavailable);
                        }
                        page["items"] = json!(items);
                        page["nextCursor"] = json!(at);
                        break;
                    }
                }
                page
            }
        };
        let response = self.custom_brush_response(result);
        if encoded(&response)? > budget {
            return Err(Error::Unavailable);
        }
        Ok(response)
    }
    fn custom_brush_response(&self, result: Value) -> Value {
        json!({"instanceId":self.instance_id(),"documentId":self.document_id(),"atRevision":self.content_revision(),
            "documentSnapshotId":self.snapshot_id(),"contextId":"scene-root","result":result})
    }
    pub(crate) fn dispatch_custom_brush_read(
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
                "Invalid custom preset read payload.",
            ));
        }
        let query = json!({"command":"object.project.customBrushPreset.read","instanceId":request.instance_id,
            "documentId":request.document_id,"atRevision":request.payload["atRevision"],
            "documentSnapshotId":request.payload["documentSnapshotId"],"contextId":request.payload["contextId"],
            "selector":request.payload["selector"]});
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
        match self.custom_brush_read_with_budget(
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
                "Custom preset read is invalid, unavailable, stale or not found.",
            )),
        }
    }
}
