//! Same-query whole-row view for the xsheet and Roll readers (U31, #2374): the single-cell exposure, run-encoded
//! and paged. A run is a maximal stretch whose frames answer the single-cell query identically, an interpolated
//! frame being its own source; a run that would overflow the page budget starts the next page at `nextFrame`.
use crate::object_codec::{object_deserialize, FrameScopeKind};
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_snapshot::MAX_MESSAGE_BYTES;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

/// Run bytes per page; the correlated envelope and its headers stay inside the 4096-byte message bound.
const RUN_BUDGET: usize = 2560;

#[derive(Debug)]
struct CellsSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    view: String,
}
object_deserialize!(CellsSelector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    view: String
});
impl ObjectSnapshot {
    pub(crate) fn query_layer_cells_json(&self, bytes: &[u8]) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision, Unavailable};
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(InvalidRequest);
        }
        let selector: CellsSelector = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        // The scene root or a declared component (#2256); an undeclared context is unavailable, never guessed.
        let context = ContextSource::of_document(self.document()).context(&selector.context_id);
        let total = context.map_or(self.document().total_frames, |c| c.total_frames());
        if selector.view != "cells"
            || selector.at_revision > 9_007_199_254_740_991
            || selector.document_snapshot_id.is_empty()
            || selector.context_id.is_empty()
            || selector.layer_uid.is_empty()
            || selector.frame >= total
        {
            return Err(InvalidRequest);
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        let context = context.map_err(|_| Unavailable)?;
        let layer = context
            .layers()
            .iter()
            .find(|layer| layer.layer_uid == selector.layer_uid)
            .ok_or(NotFound)?;
        // Same rule as the single-cell read: a missing timing model is not a blank row.
        let timing = layer.timing.value().ok_or(Unavailable)?;
        let authored: HashSet<u32> = context
            .objects()
            .iter()
            .filter(|object| {
                object.target.context_id == selector.context_id
                    && object.target.layer_uid == selector.layer_uid
                    && object.target.frame_scope.kind == FrameScopeKind::Authored
            })
            .map(|object| object.target.frame_scope.frame)
            .collect();
        let cell = |frame: u32| {
            let (kind, source, manual) = timing.exposure(frame);
            (
                kind,
                source,
                source.is_some_and(|s| authored.contains(&s)),
                manual,
            )
        };
        let (mut runs, mut used, mut next, mut frame) = (Vec::new(), 0, None, selector.frame);
        while frame < total {
            let (kind, source, content, manual) = cell(frame);
            let mut end = frame + 1;
            while kind != "key" && end < total {
                let (k, s, c, m) = cell(end);
                let source_matches = if kind == "interpolated" {
                    s == Some(end)
                } else {
                    s == source
                };
                if k != kind || !source_matches || c != content || m != manual {
                    break;
                }
                end += 1;
            }
            let run = json!([frame, end - frame, kind, source, content, manual]);
            let size = run.to_string().len() + 1;
            if used + size > RUN_BUDGET {
                next = Some(frame);
                break;
            }
            used += size;
            runs.push(run);
            frame = end;
        }
        Ok(
            json!({"kind":"layerCells","instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),"contextId":selector.context_id,
            "layerUid":selector.layer_uid,"frame":selector.frame,"cells":{"runs":runs,"nextFrame":next}}),
        )
    }
}
#[cfg(test)]
#[path = "../tests/object_layer_cells_snapshot.rs"]
mod tests;
