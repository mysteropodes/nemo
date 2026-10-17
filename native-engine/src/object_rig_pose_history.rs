//! Pose transactions use the existing history and receipt owner (I13): the typed preparation of
//! `object_rig_pose_commit.rs`, one history entry per real change, and the dispatch body the shared
//! envelope of `object_command_dispatch.rs` routes to. The envelope itself (api version, identifiers,
//! owner pins, cancellation, `expectedRevision`, request fingerprint and retained replay) is NOT repeated
//! here: it ran before this body is reached, so a replay never executes a pose twice.
use crate::commands::PreparedRigPose;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_RIG_POSE: &str = "command.document.object.rig.pose";

/// True for the rig pose operation; the dispatcher routes exactly this here.
pub(crate) fn is_rig_pose_operation(operation: &str) -> bool {
    operation == OP_RIG_POSE
}

impl NativeObjectHistory {
    /// The typed refusal as the dispatcher carries it: the code and the message that starts with the
    /// stable reason (`no_rigged_stroke`, `layer_locked`, `pose_unavailable`, ...).
    pub(crate) fn prepare_rig_pose_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedRigPose, (DispatchErrorCode, &'static str)> {
        self.owner
            .prepare_rig_pose_json(bytes)
            .map_err(|refusal| (refusal.code(), refusal.message()))
    }

    /// A history entry only when the revision changed: an exact no-op keeps both stacks.
    pub(crate) fn commit_rig_pose(
        &mut self,
        candidate: PreparedRigPose,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_rig_pose(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }

    /// `command.document.object.rig.pose`: prepare, then commit through the sealed owner. The reply is
    /// the compact command result the other object commands use.
    pub(crate) fn dispatch_rig_pose(
        &mut self,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "malformed_request: the rig pose payload is not valid JSON.",
            )
        })?;
        let candidate = self.prepare_rig_pose_json(&bytes)?;
        let before = self.content_revision();
        self.commit_rig_pose(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Rig pose could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_rig_pose_commit_oracle.rs"]
mod object_rig_pose_commit_oracle;

#[cfg(test)]
#[path = "../tests/object_rig_pose_commit_rows.rs"]
mod object_rig_pose_commit_rows;

#[cfg(test)]
#[path = "../tests/object_rig_pose_commit.rs"]
mod object_rig_pose_commit_tests;

#[cfg(test)]
#[path = "../tests/object_rig_pose_commit_wire.rs"]
mod object_rig_pose_commit_wire_tests;

#[cfg(test)]
#[path = "../tests/object_rig_pose_commit_meshes.rs"]
mod object_rig_pose_commit_mesh_tests;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_rig_pose_commit_kernels.rs"]
mod object_rig_pose_commit_kernel_tests;
