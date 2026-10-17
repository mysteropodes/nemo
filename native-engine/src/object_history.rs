//! Staged object edits/history authority; no transport, retries or replacement port.
use crate::commands::{
    ObjectFillOwner, PreparedLayerCreate, PreparedLayerVisibility, PreparedObjectFill,
    PreparedPropertyEdit, PreparedReferenceEdit, PreparedVectorEdit, PreparedWidgetEdit,
    ReferenceEdit,
};
use crate::object_document::{ObjectDocument, ProjectEnvelope};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use crate::transaction::StageReceipts;

#[cfg(feature = "font_asset_wire")]
#[path = "object_font_asset_history.rs"]
mod font_assets;
#[path = "object_layer_policy_history.rs"]
mod layer_policy;
#[path = "object_command_dispatch.rs"]
mod object_command_dispatch;
#[path = "object_project_settings_history.rs"]
mod project_settings;
// L02-F (#2108): layer delete/duplicate/rename/reorder as owner transactions.
#[path = "object_layer_lifecycle_history.rs"]
mod layer_lifecycle;
// L02-G (#2108): group / ungroup / combine / mode / rename as owner transactions.
#[path = "object_group_history.rs"]
mod group_history;
// L05-C (lane #2111): frame-structure transactions (insert / extend / duplicate / remove frames).
#[path = "object_frame_edit_structure_history.rs"]
mod object_frame_edit_structure;
// L05-D (lane #2111): F6 / Clear Keyframe / Delete Frames / Remove Tween transactions.
#[path = "object_exposure_edit_key_history.rs"]
mod object_exposure_edit_key;
// L05-E (lane #2111): key retime (moveKeyframe) transaction.
#[path = "object_exposure_edit_retime_history.rs"]
mod object_exposure_edit_retime;
// L09-H (#2115): placement edits as one history transaction (G42, I16b).
#[path = "object_media_link_history.rs"]
#[allow(dead_code)]
mod object_media_link_history;
// L10-B (#2124): lane-10 shot/montage/import family aggregator.
#[path = "object_shot_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod object_shot_kernels;
#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_project_settings.rs"]
mod project_settings_tests;

#[derive(Debug, Clone)]
struct ObjectHistoryEntry {
    before_revision: u64,
    after_revision: u64,
}

#[cfg(test)]
#[path = "../tests/object_project_history.rs"]
mod project_tests;

#[cfg(test)]
#[path = "../tests/object_layer_create.rs"]
mod layer_create_tests;
#[cfg(test)]
#[path = "../tests/object_group_ops_chain.rs"]
mod object_group_ops_chain;
#[cfg(test)]
#[path = "../tests/object_group_ops_controls.rs"]
mod object_group_ops_controls;
#[cfg(test)]
#[path = "../tests/object_group_ops_join.rs"]
mod object_group_ops_join;
#[cfg(test)]
#[path = "../tests/object_group_ops_payloads.rs"]
mod object_group_ops_payloads;
#[cfg(test)]
#[path = "../tests/object_group_ops.rs"]
mod object_group_ops_tests;
#[cfg(test)]
#[path = "../tests/object_group_ops_topology.rs"]
mod object_group_ops_topology;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_chain.rs"]
mod object_layer_lifecycle_chain;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_controls.rs"]
mod object_layer_lifecycle_controls;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_payloads.rs"]
mod object_layer_lifecycle_payloads;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_scenario.rs"]
mod object_layer_lifecycle_scenario;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle.rs"]
mod object_layer_lifecycle_tests;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_topology.rs"]
mod object_layer_lifecycle_topology;
#[cfg(test)]
#[path = "../tests/object_property_edit.rs"]
mod property_edit_tests;
// L03-M (#2109): widget transaction tests (the write tests need the evaluation kernel).
#[cfg(test)]
#[path = "../tests/object_widget_edit_controls.rs"]
mod object_widget_edit_controls_tests;
#[cfg(test)]
#[path = "../tests/object_widget_edit_guards.rs"]
mod object_widget_edit_guards_tests;
#[cfg(test)]
#[path = "../tests/object_widget_edit_rows.rs"]
mod object_widget_edit_rows;
#[cfg(test)]
#[path = "../tests/object_widget_edit.rs"]
mod object_widget_edit_tests;
#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_widget_edit_writes.rs"]
mod object_widget_edit_writes_tests;
#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_property_sampling.rs"]
mod property_sampling_tests;
#[cfg(test)]
#[path = "../tests/object_property_track_edit.rs"]
mod property_track_edit_tests;

#[cfg(feature = "project_media_codec")]
#[path = "object_project_history.rs"]
mod project_workflow;
#[cfg(feature = "project_media_codec")]
pub use project_workflow::{InactiveProject, PreparedProjectReplacement};

/// One non-Clone owner. Construction always admits a fresh document incarnation.
#[derive(Debug)]
pub struct NativeObjectHistory {
    owner: ObjectFillOwner,
    undo: Vec<ObjectHistoryEntry>,
    redo: Vec<ObjectHistoryEntry>,
    receipts: StageReceipts,
}

impl NativeObjectHistory {
    pub(crate) fn prepare_property_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        self.owner.prepare_property_edit_json(bytes)
    }
    pub(crate) fn prepare_property_track_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        self.owner.prepare_property_track_edit_json(bytes)
    }
    pub(crate) fn commit_property_edit(
        &mut self,
        candidate: PreparedPropertyEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before = self.content_revision();
        let after = self.owner.commit_property_edit(candidate)?;
        if before != after {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
    pub(crate) fn prepare_vector_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedVectorEdit, DispatchErrorCode> {
        self.owner.prepare_vector_edit_json(bytes)
    }
    /// History entry only when the revision changed: an exact no-op keeps both stacks.
    pub(crate) fn commit_vector_edit(
        &mut self,
        candidate: PreparedVectorEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before = self.content_revision();
        let after = self.owner.commit_vector_edit(candidate)?;
        if before != after {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
    /// Widget creation is the layer-create candidate with the widget, controls and holder added.
    pub(crate) fn prepare_widget_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        self.owner.prepare_widget_create_json(bytes)
    }
    pub(crate) fn prepare_widget_write_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedWidgetEdit, DispatchErrorCode> {
        self.owner.prepare_widget_write_json(bytes)
    }
    pub(crate) fn prepare_widget_settings_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedWidgetEdit, DispatchErrorCode> {
        self.owner.prepare_widget_settings_json(bytes)
    }
    pub(crate) fn commit_widget_edit(
        &mut self,
        candidate: PreparedWidgetEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before = self.content_revision();
        let after = self.owner.commit_widget_edit(candidate)?;
        if before != after {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
    pub(crate) fn prepare_layer_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        self.owner.prepare_layer_create_json(bytes)
    }
    pub(crate) fn commit_layer_create(
        &mut self,
        candidate: PreparedLayerCreate,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_layer_create(candidate)?;
        self.undo.push(ObjectHistoryEntry {
            before_revision,
            after_revision,
        });
        self.redo.clear();
        Ok(after_revision)
    }
    pub(crate) fn prepare_reference_edit(
        &self,
        edit: ReferenceEdit,
    ) -> Result<PreparedReferenceEdit, DispatchErrorCode> {
        self.owner.prepare_reference_edit(edit)
    }
    pub(crate) fn commit_reference_edit(
        &mut self,
        candidate: PreparedReferenceEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before = self.content_revision();
        let after = self.owner.commit_reference_edit(candidate)?;
        if after != before {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
    /// Crate-private only; aggregate/resource admission and host routing are
    /// unavailable. Uses the SAME checked owner and fresh incarnation as bare.
    pub(crate) fn from_admitted_project(
        instance_id: impl Into<String>,
        project: ProjectEnvelope,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            owner: ObjectFillOwner::from_admitted_project(instance_id, project)?,
            undo: vec![],
            redo: vec![],
            receipts: StageReceipts::default(),
        })
    }
    pub fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            owner: ObjectFillOwner::new(instance_id, document)?,
            undo: vec![],
            redo: vec![],
            receipts: StageReceipts::default(),
        })
    }
    pub fn instance_id(&self) -> &str {
        self.owner.instance_id()
    }
    pub fn document_id(&self) -> &str {
        self.owner.document_id()
    }
    pub fn content_revision(&self) -> u64 {
        self.owner.content_revision()
    }
    pub fn acquire_snapshot(&self, revision: u64) -> Option<ObjectSnapshot> {
        self.owner.acquire_snapshot(revision)
    }
    pub fn history_depths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    pub(crate) fn prepare_layer_visibility_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerVisibility, DispatchErrorCode> {
        self.owner.prepare_layer_visibility_json(bytes)
    }
    pub(crate) fn commit_layer_visibility(
        &mut self,
        candidate: PreparedLayerVisibility,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_layer_visibility(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Strict D1 payload preparation from this owner's current immutable snapshot.
    pub fn prepare_fill_json(&self, bytes: &[u8]) -> Result<PreparedObjectFill, DispatchErrorCode> {
        self.owner.prepare_fill_json(bytes)
    }
    /// Returns the selected revision; semantic no-op returns the unchanged head.
    pub fn commit_fill(&mut self, candidate: PreparedObjectFill) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_fill(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Restores full retained before-content into a new monotonic revision.
    pub fn undo(&mut self, expected_revision: u64) -> Result<u64, DispatchErrorCode> {
        self.move_history(expected_revision, true)
    }
    /// Restores full retained after-content into a new monotonic revision.
    pub fn redo(&mut self, expected_revision: u64) -> Result<u64, DispatchErrorCode> {
        self.move_history(expected_revision, false)
    }
    fn move_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<u64, DispatchErrorCode> {
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let source = if undo { &self.undo } else { &self.redo };
        let entry = source.last().ok_or(DispatchErrorCode::Unavailable)?.clone();
        let revision = if undo {
            entry.before_revision
        } else {
            entry.after_revision
        };
        let restored = self.owner.restore(revision)?;
        if undo {
            self.undo.pop();
            self.redo.push(entry);
        } else {
            self.redo.pop();
            self.undo.push(entry);
        }
        Ok(restored)
    }
}
