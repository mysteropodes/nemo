//! Pinned guide axes on the existing immutable project snapshot.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
enum Axis {
    #[serde(rename = "h")]
    Horizontal,
    #[serde(rename = "v")]
    Vertical,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Selector {
    Settings {},
    AxisPage { axis: Axis, cursor: u64, limit: u64 },
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
    pub(crate) fn query_guide_metadata_json(&self, bytes: &[u8]) -> Result<Value, Error> {
        self.guide_read_with_budget(bytes, 4096)
    }
    fn guide_read_with_budget(&self, bytes: &[u8], budget: usize) -> Result<Value, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if !shape.get("selector").is_some_and(Value::is_object) {
            return Err(Error::InvalidRequest);
        }
        let query: Query = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if query.command != "object.project.guide.read" {
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
        let supplied = metadata.guide_metadata();
        let (h, v) = supplied.unwrap_or_default();
        let result = match query.selector {
            Selector::Settings {} => {
                json!({"guidesPresent":supplied.is_some(),"hPresent":h.is_some(),
                "vPresent":v.is_some(),"hCount":h.map_or(0, |values| values.len()),"vCount":v.map_or(0, |values| values.len())})
            }
            Selector::AxisPage {
                axis,
                cursor,
                limit,
            } => {
                if cursor > 9_007_199_254_740_991 || !(1..=16).contains(&limit) {
                    return Err(Error::InvalidRequest);
                }
                let (name, values) = match axis {
                    Axis::Horizontal => ("h", h),
                    Axis::Vertical => ("v", v),
                };
                let count = values.map_or(0, |values| values.len());
                self.guide_page(json!({"kind":"guideAxisPage","axis":name,"guidesPresent":supplied.is_some(),
                    "axisPresent":values.is_some(),"guideCount":count,"cursor":cursor,"items":[],"nextCursor":null}),
                    count,cursor,limit,budget,|index|json!({"ordinal":index+1,"position":values.unwrap()[index]}))?
            }
        };
        let response = self.guide_response(result);
        if encoded(&response)? > budget {
            return Err(Error::Unavailable);
        }
        Ok(response)
    }
    fn guide_page<F: FnMut(usize) -> Value>(
        &self,
        mut page: Value,
        count: usize,
        cursor: u64,
        limit: u64,
        budget: usize,
        mut item: F,
    ) -> Result<Value, Error> {
        let cursor = usize::try_from(cursor).map_err(|_| Error::InvalidRequest)?;
        if cursor > count {
            return Err(Error::InvalidRequest);
        }
        let mut items = Vec::new();
        for at in (cursor..count).take(limit as usize) {
            items.push(item(at));
            page["items"] = json!(items);
            page["nextCursor"] = if at + 1 < count {
                json!(at + 1)
            } else {
                Value::Null
            };
            if encoded(&self.guide_response(page.clone()))? > budget {
                items.pop();
                if items.is_empty() {
                    return Err(Error::Unavailable);
                }
                page["items"] = json!(items);
                page["nextCursor"] = json!(at);
                break;
            }
        }
        Ok(page)
    }
    fn guide_response(&self, result: Value) -> Value {
        json!({"instanceId":self.instance_id(),"documentId":self.document_id(),"atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),"contextId":"scene-root","result":result})
    }
    /// The common dispatcher must admit the request before selecting this wrapper.
    /// It is not currently registered as an available public operation.
    pub(crate) fn dispatch_guide_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, Error> {
        let fields = ["atRevision", "documentSnapshotId", "contextId", "selector"];
        if request
            .payload
            .as_object()
            .is_none_or(|v| v.len() != fields.len() || fields.iter().any(|k| !v.contains_key(*k)))
        {
            return Ok(self.read_failure(
                request,
                Error::InvalidRequest,
                "Invalid guide read payload.",
            ));
        }
        let query = json!({"command":"object.project.guide.read","instanceId":request.instance_id,"documentId":request.document_id,
            "atRevision":request.payload["atRevision"],"documentSnapshotId":request.payload["documentSnapshotId"],"contextId":request.payload["contextId"],"selector":request.payload["selector"]});
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
        match self.guide_read_with_budget(
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
                "Guide read is invalid, unavailable, stale or not found.",
            )),
        }
    }
}
