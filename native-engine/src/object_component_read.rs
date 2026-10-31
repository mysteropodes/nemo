//! #2256: read-only native component model (`query.document.object.component.read`).
//!
//! A Component is a CONTEXT of the one document (`object_context_resolver`), not a second document. This query lists the
//! contexts a pinned snapshot declares (`scene-root`, then every component in table order) and, for one context id, its
//! summary, its layers and the scene layers that instance it. It is the model that enter/exit component context reads
//! first; it opens no context, writes nothing, mints no id and keeps no copy: the answer is built from the immutable
//! snapshot the request is pinned to, one bounded page at a time.
//!
//! The pin is `(atRevision, documentSnapshotId)` of this snapshot; any other pin is `stale_revision`. An undeclared context
//! id is `not_found`. `instances` answers for a component only: the scene is not instanced by anything. A component record
//! natively carries no camera keys (they belong to an owner that does not exist yet), so its `hasCameraKeys` is false;
//! the scene reports whether the project camera has keys. A page is cut by `limit` AND by the 4096-byte response bound; an
//! item that cannot fit alone is a typed `unavailable`, never a truncated name.
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};

#[cfg(test)]
#[path = "../tests/object_component_read.rs"]
mod tests;

const MAX_SAFE: u64 = 9_007_199_254_740_991;
/// Items per page; the byte bound cuts a page earlier when names are long.
const MAX_PAGE: u64 = 16;
const MAX_ID_BYTES: usize = 128;
pub(crate) const OPERATION: &str = "query.document.object.component.read";

enum Selector<'a> {
    Contexts {
        cursor: u64,
        limit: u64,
    },
    Context {
        id: &'a str,
    },
    Layers {
        id: &'a str,
        cursor: u64,
        limit: u64,
    },
    Instances {
        id: &'a str,
        cursor: u64,
        limit: u64,
    },
}

fn fields<'a>(
    object: &'a serde_json::Map<String, Value>,
    required: &[&str],
) -> Option<&'a serde_json::Map<String, Value>> {
    (object.len() == required.len() && required.iter().all(|key| object.contains_key(*key)))
        .then_some(object)
}
fn context_id(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES && !id.contains('\0'))
}
fn page_args(object: &serde_json::Map<String, Value>) -> Option<(u64, u64)> {
    let cursor = object.get("cursor")?.as_u64().filter(|c| *c <= MAX_SAFE)?;
    let limit = object
        .get("limit")?
        .as_u64()
        .filter(|l| (1..=MAX_PAGE).contains(l))?;
    Some((cursor, limit))
}
fn selector(payload: &Value) -> Option<Selector<'_>> {
    let object = fields(
        payload.as_object()?,
        &["atRevision", "documentSnapshotId", "selector"],
    )?;
    let selector = object.get("selector")?.as_object()?;
    match selector.get("kind")?.as_str()? {
        "contexts" => {
            let (cursor, limit) = page_args(fields(selector, &["kind", "cursor", "limit"])?)?;
            Some(Selector::Contexts { cursor, limit })
        }
        "context" => {
            let object = fields(selector, &["kind", "contextId"])?;
            Some(Selector::Context {
                id: context_id(&object["contextId"])?,
            })
        }
        kind @ ("layers" | "instances") => {
            let object = fields(selector, &["kind", "contextId", "cursor", "limit"])?;
            let (cursor, limit) = page_args(object)?;
            let id = context_id(&object["contextId"])?;
            Some(if kind == "layers" {
                Selector::Layers { id, cursor, limit }
            } else {
                Selector::Instances { id, cursor, limit }
            })
        }
        _ => None,
    }
}

impl ObjectSnapshot {
    pub(crate) fn dispatch_component_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let ReadAdmission::Refused(response) = self.read_admission(request)? {
            return Ok(response);
        }
        let refused = |code, message| Ok(self.read_failure(request, code, message));
        let Some(selector) = selector(&request.payload) else {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Invalid component read payload.",
            );
        };
        let pinned = request.payload["atRevision"] == self.content_revision()
            && request.payload["documentSnapshotId"] == self.snapshot_id();
        if !pinned {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Component read pin is not this snapshot.",
            );
        }
        match self.component_answer(request, selector) {
            Ok(response) => Ok(response),
            Err(code) => refused(
                code,
                "Component read is unavailable or names no such context.",
            ),
        }
    }

    #[cfg(feature = "evaluation")]
    fn component_envelope(&self, request: &OpacityRequest, result: Value) -> ResponseEnvelope {
        ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    #[cfg(feature = "evaluation")]
    fn component_head(&self, kind: &str) -> Value {
        json!({"atRevision":self.content_revision(),
            "documentSnapshotId":self.snapshot_id(),"kind":kind})
    }

    #[cfg(not(feature = "evaluation"))]
    fn component_answer(
        &self,
        _request: &OpacityRequest,
        _selector: Selector<'_>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        Err(DispatchErrorCode::Unavailable)
    }

    #[cfg(feature = "evaluation")]
    fn component_answer(
        &self,
        request: &OpacityRequest,
        selector: Selector<'_>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use crate::evaluation::object_component_kernels::object_context_resolver::{
            ContextKind, ContextSource,
        };
        let source = ContextSource::of_document(self.document());
        let summary = |id: &str| -> Result<Value, DispatchErrorCode> {
            let context = source
                .context(id)
                .map_err(|_| DispatchErrorCode::NotFound)?;
            let component = context.component();
            Ok(json!({"contextId":context.id(),
                "contextKind":if context.kind() == ContextKind::SceneRoot { "scene" } else { "component" },
                "name":component.and_then(|c| c.name.value().cloned()),
                "totalFrames":context.total_frames(),
                "layerCount":context.layers().len()}))
        };
        let ids = source.declared_contexts();
        let instancing = |id: &str| -> Vec<&str> {
            self.document()
                .layers()
                .iter()
                .map(|layer| layer.layer_uid())
                .filter(|uid| source.instance(uid).is_some_and(|i| i.symbol_id == id))
                .collect()
        };
        let (mut page, count, cursor, limit, items): (Value, usize, u64, u64, Vec<Value>) =
            match selector {
                Selector::Context { id } => {
                    let mut result = self.component_head("context");
                    let context = source
                        .context(id)
                        .map_err(|_| DispatchErrorCode::NotFound)?;
                    let component = context.component();
                    let scene = component.is_none();
                    let scene_camera = self
                        .project()
                        .and_then(|p| p.project().camera_keys())
                        .is_some_and(|keys| !keys.is_empty());
                    let mut body = summary(id)?;
                    body["fps"] = json!(component.and_then(|c| c.fps.value().cloned()));
                    body["instanceCount"] = json!(if scene {
                        self.document()
                            .layers()
                            .iter()
                            .filter(|layer| source.instance(layer.layer_uid()).is_some())
                            .count()
                    } else {
                        instancing(id).len()
                    });
                    body["exposedPropertyCount"] =
                        json!(component.map_or(0, |c| c.exposed_props().len()));
                    body["hasCameraKeys"] = json!(scene && scene_camera);
                    for (key, value) in body.as_object().into_iter().flatten() {
                        result[key] = value.clone();
                    }
                    let response = self.component_envelope(request, result);
                    return self.component_bounded(response);
                }
                Selector::Contexts { cursor, limit } => (
                    self.component_head("contexts"),
                    ids.len(),
                    cursor,
                    limit,
                    ids.iter().filter_map(|id| summary(id).ok()).collect(),
                ),
                Selector::Layers { id, cursor, limit } => {
                    let context = source
                        .context(id)
                        .map_err(|_| DispatchErrorCode::NotFound)?;
                    let scene = context.component().is_none();
                    let items = context
                        .layers()
                        .iter()
                        .map(|layer| {
                            let kind = if layer.widget().is_some() {
                                "widget"
                            } else if layer.is_null_layer() {
                                "null"
                            } else if layer.is_folder_layer.value() == Some(&true) {
                                "folder"
                            } else if layer.is_effect_layer.value() == Some(&true) {
                                "effect"
                            } else if scene && source.instance(layer.layer_uid()).is_some() {
                                "instance"
                            } else {
                                "drawing"
                            };
                            json!({"layerUid":layer.layer_uid(),"name":layer.name(),"layerKind":kind})
                        })
                        .collect();
                    let mut head = self.component_head("layers");
                    head["contextId"] = json!(context.id());
                    (head, context.layers().len(), cursor, limit, items)
                }
                Selector::Instances { id, cursor, limit } => {
                    let context = source
                        .context(id)
                        .map_err(|_| DispatchErrorCode::NotFound)?;
                    if context.kind() != ContextKind::Component {
                        return Err(DispatchErrorCode::InvalidRequest);
                    }
                    let uids = instancing(id);
                    let mut head = self.component_head("instances");
                    head["contextId"] = json!(context.id());
                    let items = uids.iter().map(|uid| json!({"layerUid":uid})).collect();
                    (head, uids.len(), cursor, limit, items)
                }
            };
        let cursor = usize::try_from(cursor).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if cursor > count {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let count_key = match page["kind"].as_str() {
            Some("contexts") => "contextCount",
            Some("layers") => "layerCount",
            _ => "instanceCount",
        };
        page[count_key] = json!(count);
        page["cursor"] = json!(cursor);
        let mut taken: Vec<Value> = Vec::new();
        for (at, item) in items
            .into_iter()
            .enumerate()
            .skip(cursor)
            .take(limit as usize)
        {
            taken.push(item);
            page["items"] = json!(taken);
            page["nextCursor"] = if at + 1 < count {
                json!(at + 1)
            } else {
                Value::Null
            };
            if serde_json::to_vec(&self.component_envelope(request, page.clone()))
                .map_err(|_| DispatchErrorCode::Internal)?
                .len()
                > MAX_MESSAGE_BYTES
            {
                taken.pop();
                if taken.is_empty() {
                    return Err(DispatchErrorCode::Unavailable);
                }
                page["items"] = json!(taken);
                page["nextCursor"] = json!(at);
                break;
            }
        }
        if taken.is_empty() {
            page["items"] = json!([]);
            page["nextCursor"] = Value::Null;
        }
        self.component_bounded(self.component_envelope(request, page))
    }

    #[cfg(feature = "evaluation")]
    fn component_bounded(
        &self,
        response: ResponseEnvelope,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        match serde_json::to_vec(&response).map(|bytes| bytes.len()) {
            Ok(0..=MAX_MESSAGE_BYTES) => Ok(response),
            Ok(_) => Err(DispatchErrorCode::Unavailable),
            Err(_) => Err(DispatchErrorCode::Internal),
        }
    }
}
