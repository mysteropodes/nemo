//! F11 cut is one transaction in the existing native history and staged dispatcher.
use crate::commands::PreparedLayerPlayheadCut;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
pub(crate) const OP_LAYER_PLAYHEAD_CUT: &str = "command.document.object.layer.cut";
impl NativeObjectHistory {
    pub(crate) fn prepare_layer_playhead_cut_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerPlayheadCut, DispatchErrorCode> {
        self.owner.prepare_layer_playhead_cut_json(bytes)
    }
    pub(crate) fn commit_layer_playhead_cut(
        &mut self,
        candidate: PreparedLayerPlayheadCut,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_layer_playhead_cut(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    pub(crate) fn dispatch_layer_playhead_cut(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        if operation != OP_LAYER_PLAYHEAD_CUT {
            return Err((DispatchErrorCode::InvalidRequest, "Unknown cut operation."));
        }
        let bytes = serde_json::to_vec(payload)
            .map_err(|_| (DispatchErrorCode::InvalidRequest, "Invalid cut payload."))?;
        let candidate = self
            .prepare_layer_playhead_cut_json(&bytes)
            .map_err(|code| (code, "Cannot prepare layer cut."))?;
        let created = candidate.created().map(str::to_owned);
        let before = self.content_revision();
        let after = self
            .commit_layer_playhead_cut(candidate)
            .map_err(|code| (code, "Cannot commit layer cut."))?;
        Ok(match created {
            Some(layer_uid) if after != before => CompactResult::LayerCreated { layer_uid },
            _ => CompactResult::Command {
                applied: after != before,
                history_entries_added: u64::from(after != before),
            },
        })
    }
}
#[cfg(test)]
#[path = "../tests/object_layer_playhead_cut_history.rs"]
mod tests;
