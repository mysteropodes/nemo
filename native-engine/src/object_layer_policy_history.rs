//! Persisted layer policy uses the existing history and receipt owner.
use crate::commands::PreparedLayerPolicy;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;

impl NativeObjectHistory {
    pub(crate) fn prepare_layer_policy_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerPolicy, DispatchErrorCode> {
        self.owner.prepare_layer_policy_json(bytes)
    }
    pub(crate) fn commit_layer_policy(
        &mut self,
        candidate: PreparedLayerPolicy,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let disclosure = candidate.disclosure();
        let after_revision = self.owner.commit_layer_policy(candidate)?;
        if after_revision != before_revision && !disclosure {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        } else if after_revision != before_revision {
            // #2520: no step of its own; the state rides with the last step, as the folder fold does (redo stays).
            if let Some(last) = self.undo.last_mut().filter(|e| e.after_revision == before_revision) {
                last.after_revision = after_revision;
            }
        }
        Ok(after_revision)
    }
}

#[cfg(test)]
#[path = "../tests/object_layer_policy_edit.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_duplicator_lock_policy.rs"]
mod duplicator_tests;

#[cfg(test)]
#[path = "../tests/object_key_lock_edit.rs"]
mod key_lock_tests;

// #2524: shift, delete, flip and distribute of selected keys as one atomic native history entry.
#[cfg(test)]
#[path = "../tests/object_key_batch_edit.rs"]
mod key_batch_tests;

// #2377: the key selection box warps and the key tints as one atomic native history entry.
#[cfg(test)]
#[path = "../tests/object_key_warp_color_edit.rs"]
mod key_warp_color_tests;

// #2520/#2525: the 3D layer switch through layer policy, its row fact, its property list and its gizmo bounds.
#[cfg(test)]
#[path = "../tests/object_layer_three_d.rs"]
mod three_d_tests;

// #2520/#2525: the 3D switch refuses where the frame path cannot place it and adds no undo entry.
#[cfg(test)]
#[path = "../tests/object_layer_three_d_guard.rs"]
mod three_d_guard_tests;
