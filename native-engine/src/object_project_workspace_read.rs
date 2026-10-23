//! Pinned raw project workspace record for the original restore consumer.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Selector {
    Record {},
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
impl ObjectSnapshot {
    pub(crate) fn query_workspace_metadata_json(&self, bytes: &[u8]) -> Result<Value, Error> {
        self.workspace_read_with_budget(bytes, 4096)
    }
    fn workspace_read_with_budget(&self, bytes: &[u8], budget: usize) -> Result<Value, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if !shape.get("selector").is_some_and(Value::is_object) {
            return Err(Error::InvalidRequest);
        }
        let query: Query = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if query.command != "object.project.workspace.read" {
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
        let Selector::Record {} = query.selector;
        let workspace = self
            .project()
            .ok_or(Error::Unavailable)?
            .project()
            .annotations()
            .0;
        let mut result = json!({"workspacePresent":workspace.is_some()});
        if let Some(workspace) = workspace {
            result["workspace"] = serde_json::to_value(workspace).map_err(|_| Error::Internal)?;
        }
        let response = json!({
            "instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "contextId":"scene-root","result":result
        });
        if serde_json::to_vec(&response)
            .map_err(|_| Error::Internal)?
            .len()
            > budget
        {
            return Err(Error::Unavailable);
        }
        Ok(response)
    }
    /// Common object admission has already fenced the complete request envelope.
    pub(crate) fn dispatch_workspace_read(
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
                "Invalid workspace read payload.",
            ));
        }
        let query = json!({
            "command":"object.project.workspace.read","instanceId":request.instance_id,
            "documentId":request.document_id,"atRevision":request.payload["atRevision"],
            "documentSnapshotId":request.payload["documentSnapshotId"],
            "contextId":request.payload["contextId"],"selector":request.payload["selector"]
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
        match self.workspace_read_with_budget(
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
                "Workspace read is invalid, unavailable or stale.",
            )),
        }
    }
}
