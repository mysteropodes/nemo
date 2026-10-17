//! Immutable text-item reads for the ordinary Text tool. Native records remain the
//! sole identity/content authority; no glyph approximation or writable UI mirror.
use crate::object_codec::{object_deserialize, FrameScope, ObjectTarget};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::revision::ObjectSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug)]
struct Selector {
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    view: View,
}
object_deserialize!(Selector {
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    view: View
});
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Scope {
    context_id: String,
    layer_uid: String,
    frame_scope: FrameScope,
}
object_deserialize!(Scope {
    context_id: String,
    layer_uid: String,
    frame_scope: FrameScope
});
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Fields {
    Targets {
        scope: Scope,
        offset: u32,
        limit: u8,
    },
    Inspect {
        target: ObjectTarget,
    },
    Text {
        target: ObjectTarget,
        offset: u32,
        limit: u16,
    },
}
// Tagged derives otherwise accept positional sequences. Original bytes retain
// duplicate/unknown-field rejection at every typed map boundary.
#[derive(Debug)]
struct View(Fields);
impl<'de> Deserialize<'de> for View {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct MapOnly;
        impl<'de> serde::de::Visitor<'de> for MapOnly {
            type Value = View;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a text read view map")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<View, A::Error> {
                Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(View)
            }
        }
        d.deserialize_map(MapOnly)
    }
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn unavailable() -> DispatchErrorCode {
    DispatchErrorCode::Unavailable
}
impl ObjectSnapshot {
    fn text_target_valid(&self, target: &ObjectTarget) -> Result<(), DispatchErrorCode> {
        if target.context_id != "scene-root" {
            return Err(unavailable());
        }
        if !self.document().valid_target(target) || target.frame() >= self.document().total_frames()
        {
            return Err(invalid());
        }
        if self
            .document()
            .layers()
            .iter()
            .filter(|l| l.layer_uid() == target.layer_uid())
            .count()
            != 1
        {
            return Err(DispatchErrorCode::NotFound);
        }
        if self.document().reference_unavailable(target) {
            return Err(unavailable());
        }
        Ok(())
    }
    /// Reads original selector bytes. Shared envelope admission additionally
    /// checks cancellation, expectedRevision absence and the complete byte cap.
    pub(crate) fn query_text_json(&self, bytes: &[u8]) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{NotFound, StaleRevision, WrongDocument, WrongInstance};
        if bytes.len() > 4096 {
            return Err(invalid());
        }
        let s: Selector = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if s.at_revision > 9_007_199_254_740_991 || s.document_snapshot_id.is_empty() {
            return Err(invalid());
        }
        if s.instance_id != self.instance_id() {
            return Err(WrongInstance);
        }
        if s.document_id != self.document_id() {
            return Err(WrongDocument);
        }
        if s.at_revision != self.content_revision() || s.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        let mut out = json!({"instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id()});
        let entries = self
            .document()
            .item_records()
            .map_or(&[][..], |v| v.text_entries());
        match s.view.0 {
            Fields::Targets {
                scope,
                offset,
                limit,
            } => {
                if !(1..=8).contains(&limit) {
                    return Err(invalid());
                }
                self.text_target_valid(&ObjectTarget {
                    context_id: scope.context_id.clone(),
                    layer_uid: scope.layer_uid.clone(),
                    frame_scope: scope.frame_scope.clone(),
                    stroke_id: "text-scope".into(),
                })?;
                let matching: Vec<_> = entries
                    .iter()
                    .filter(|e| {
                        e.target.context_id == scope.context_id
                            && e.target.layer_uid == scope.layer_uid
                            && e.target.frame_scope == scope.frame_scope
                    })
                    .collect();
                let start = offset as usize;
                if start > matching.len() {
                    return Err(invalid());
                }
                let mut page = Vec::new();
                for entry in matching.iter().skip(start).take(limit as usize) {
                    page.push(json!({"target":entry.target}));
                    out["kind"] = json!("targets");
                    out["scope"] = json!(scope);
                    out["offset"] = json!(offset);
                    out["total"] = json!(matching.len());
                    out["items"] = json!(page);
                    out["nextOffset"] = if start + page.len() < matching.len() {
                        json!(start + page.len())
                    } else {
                        Value::Null
                    };
                    if serde_json::to_vec(&out).map_err(|_| unavailable())?.len() > 3072 {
                        page.pop();
                        break; // Whole rows only, never truncate an identity.
                    }
                }
                if page.is_empty() && start < matching.len() {
                    return Err(unavailable());
                }
                out["kind"] = json!("targets");
                out["scope"] = json!(scope);
                out["offset"] = json!(offset);
                out["total"] = json!(matching.len());
                out["items"] = json!(page);
                out["nextOffset"] = if start + page.len() < matching.len() {
                    json!(start + page.len())
                } else {
                    Value::Null
                };
            }
            view => {
                let target = match &view {
                    Fields::Inspect { target } | Fields::Text { target, .. } => target,
                    _ => unreachable!(),
                };
                self.text_target_valid(target)?;
                let entry = entries
                    .iter()
                    .find(|e| &e.target == target)
                    .ok_or(NotFound)?;
                let mut block = serde_json::to_value(&entry.block).map_err(|_| unavailable())?;
                let text = block
                    .as_object_mut()
                    .and_then(|b| b.remove("text"))
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .ok_or_else(unavailable)?;
                out["target"] = json!(target);
                out["textBytes"] = json!(text.len());
                match view {
                    Fields::Inspect { .. } => {
                        out["kind"] = json!("inspect");
                        out["block"] = block;
                    }
                    Fields::Text { offset, limit, .. } => {
                        let start = offset as usize;
                        if !(1..=1024).contains(&limit)
                            || start > text.len()
                            || !text.is_char_boundary(start)
                        {
                            return Err(invalid());
                        }
                        let mut end = (start + limit as usize).min(text.len());
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        if end == start && start < text.len() {
                            return Err(invalid());
                        }
                        out["kind"] = json!("text");
                        out["offset"] = json!(offset);
                        loop {
                            out["text"] = json!(&text[start..end]);
                            out["nextOffset"] = if end < text.len() {
                                json!(end)
                            } else {
                                Value::Null
                            };
                            if serde_json::to_vec(&out).map_err(|_| unavailable())?.len() <= 3072 {
                                break;
                            }
                            if end == start {
                                return Err(unavailable());
                            }
                            end -= 1;
                            while !text.is_char_boundary(end) {
                                end -= 1;
                            }
                            if end == start && start < text.len() {
                                return Err(unavailable());
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        if serde_json::to_vec(&out).map_err(|_| unavailable())?.len() > 3072 {
            return Err(unavailable());
        }
        Ok(out)
    }
    pub(super) fn dispatch_text_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let fields = ["atRevision", "documentSnapshotId", "view"];
        if request
            .payload
            .as_object()
            .is_none_or(|p| p.len() != 3 || fields.iter().any(|k| !p.contains_key(*k)))
        {
            return Ok(self.read_failure(
                request,
                invalid(),
                "Text read requires only revision, snapshot and view.",
            ));
        }
        let selector = json!({"instanceId":request.instance_id,"documentId":request.document_id,
            "atRevision":request.payload["atRevision"],"documentSnapshotId":request.payload["documentSnapshotId"],"view":request.payload["view"]});
        let result =
            match self.query_text_json(&serde_json::to_vec(&selector).map_err(|_| invalid())?) {
                Ok(value) => value,
                Err(code) => {
                    return Ok(self.read_failure(
                        request,
                        code,
                        "Pinned text read is invalid or unavailable.",
                    ))
                }
            };
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
        if serde_json::to_vec(&response)
            .map_err(|_| unavailable())?
            .len()
            > 4096
        {
            return Ok(self.read_failure(
                request,
                unavailable(),
                "Complete text read response exceeds 4096 bytes.",
            ));
        }
        Ok(response)
    }
}
