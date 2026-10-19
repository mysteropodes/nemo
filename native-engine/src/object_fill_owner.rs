//! Private bridge from codec admission and sealed fill origins to revision storage.
use crate::commands::{prepare_object_fill_json, PreparedObjectFill};
use crate::object_document::{ObjectDocument, ProjectEnvelope, ProjectReferenceState};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectRevisionOwner;
#[path = "object_property_edit.rs"]
mod property_edit;
pub(crate) use property_edit::PreparedPropertyEdit;
#[path = "object_project_settings.rs"]
mod project_settings;
pub(crate) use project_settings::PreparedProjectSettings;
// L10-H (#2116): atomic import batch (sealed candidate + the single publish).
#[path = "native_import_transaction_owner.rs"]
mod import_batch;
pub(crate) use import_batch::{ImportBatchResult, ImportSkipReport, PreparedImportBatch};
// L05-H (#2134), re-homed from the lane-5 evaluation aggregator: detached frame-clipboard packet kernel (copyFrames).
#[path = "object_frame_clipboard_packet.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_packet;
pub(crate) use object_frame_clipboard_packet::FrameClipboardPacket;
// L05-I (lane #2111): sealed paste / cut / duplicate-selection transactions (pasteFrames, cutFrames, duplicateSelectedFrames).
#[path = "object_frame_clipboard_paste.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_paste;
#[path = "object_frame_clipboard_paste_plan.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_paste_plan;
#[path = "object_frame_clipboard_paste_wire.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_paste_wire;
pub(crate) use object_frame_clipboard_paste::{
    CellRef, FrameClipboardOperation, PasteCell, PasteRequest,
};
pub(crate) use object_frame_clipboard_paste_plan::{
    packet_paste_cells, FrameClipboardOutcome, PreparedFrameClipboard,
};
pub(crate) use object_frame_clipboard_paste_wire::{cut_clipboard_payload, paste_payload};
// L04-I (#2110): sealed replacement of the lane-4 component/StoryBoard/duplicator tables.
#[path = "object_component_commit.rs"]
#[allow(dead_code)]
mod component_tables;
#[allow(unused_imports)]
pub(crate) use component_tables::{ComponentTablesEdit, PreparedComponentTables};
// F11 cut-at-playhead through the existing sealed revision owner.
#[path = "object_layer_playhead_cut_owner.rs"]
mod layer_playhead_cut;
pub(crate) use layer_playhead_cut::PreparedLayerPlayheadCut;
#[path = "object_layer_lifecycle_ops.rs"]
mod layer_lifecycle;
pub(crate) use layer_lifecycle::PreparedLayerLifecycle;
#[path = "object_group_ops.rs"]
mod group_edit;
pub(crate) use group_edit::{PreparedBooleanEdit, PreparedGroupEdit};
// L05-B (#2160), re-homed from the evaluation aggregator: pure document-level frame remap kernel.
#[path = "object_frame_edit_document.rs"]
#[allow(dead_code)]
mod object_frame_edit_document;
// L05-C (lane #2111): sealed insert / extend / duplicate / remove frame transactions.
#[path = "object_frame_edit_structure.rs"]
mod object_frame_edit_structure;
pub(crate) use object_frame_edit_structure::PreparedFrameStructure;
// L05-D (lane #2111): sealed F6 / Clear Keyframe / Delete Frames / Remove Tween transactions.
#[path = "object_exposure_edit_key.rs"]
mod object_exposure_edit_key;
pub(crate) use object_exposure_edit_key::PreparedExposureKey;
// F7 uses the same revision owner with an always-checkpoint blank-key candidate.
#[path = "object_blank_key_edit.rs"]
mod object_blank_key_edit;
pub(crate) use object_blank_key_edit::PreparedBlankKey;
// L05-E (lane #2111): sealed key-retime (moveKeyframe) transaction.
#[path = "object_exposure_edit_retime.rs"]
mod object_exposure_edit_retime;
pub(crate) use object_exposure_edit_retime::PreparedExposureRetime;
// L05-J2 (lane #2111): Motion-track half of Repeat Selection and the cross-context frame clipboard port (F13).
#[path = "object_frame_clipboard_tracks.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_tracks;
#[path = "object_frame_clipboard_tracks_context.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_tracks_context;
#[allow(unused_imports)]
pub(crate) use object_frame_clipboard_tracks_context::ClipboardContexts;
// L05-J1 (lane #2111): sealed Repeat Selection transaction (repeatSelection). Imports L05-I's helpers and L05-B's kernel.
#[path = "object_frame_clipboard_repeat.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_repeat;
#[path = "object_frame_clipboard_repeat_wire.rs"]
#[allow(dead_code)]
mod object_frame_clipboard_repeat_wire;
#[allow(unused_imports)]
pub(crate) use object_frame_clipboard_repeat::{
    clamp_times, grown_total, PreparedFrameRepeat, RepeatRequest,
};
#[allow(unused_imports)]
pub(crate) use object_frame_clipboard_repeat_wire::repeat_payload;
// L03-M (#2109): widget (rig control) transactions; the settings edit is a child of this module.
#[path = "object_widget_edit.rs"]
mod widget_edit;
pub(crate) use widget_edit::{
    PreparedImageMeshEdit, PreparedRigEdit, PreparedRigPose, PreparedWidgetEdit,
};
#[cfg(feature = "project_media_codec")]
#[path = "object_raster_image_import_owner.rs"]
pub(crate) mod image_import;
// L06-H (lane #2112): sealed commit of a prepared asset attach / remove.
#[cfg(feature = "project_media_codec")]
#[path = "object_raster_asset_owner.rs"]
#[allow(dead_code)]
mod object_raster_asset_owner;
// L09-H (#2115): sealed placement edits (G42, I16b).
#[path = "object_media_link_owner.rs"]
#[allow(dead_code)]
mod object_media_link_owner;
pub(crate) use object_media_link_owner::PreparedPlacementEdit;
// L01-F (#2107): sealed vector create/replace/delete over the same revision owner.
#[path = "object_vector_edit.rs"]
mod vector_edit;
pub(crate) use vector_edit::{PreparedVectorEdit, VectorEditResult, MAX_VECTOR_REQUEST_BYTES};
// The single composition-order element helper, for every lane that adds or removes a record.
#[allow(unused_imports)]
pub(crate) use vector_edit::order::{
    insert_element as insert_order_element, remove_element as remove_order_element, OrderEditError,
};
#[path = "object_layer_policy_edit.rs"]
mod layer_policy_edit;
pub(crate) use layer_policy_edit::PreparedLayerPolicy;
#[path = "object_component_owner_kernels.rs"]
pub(crate) mod component_owner;
// L01-G (#2107): sealed parameter edit of a dynamic shape (rect/ellipse/star set, translate, scale) and sealed
// replacement of a layer's ordered path-effect stack. Both read the vector edit's guards (`vector_edit::check_*`).
#[path = "object_parametric_edit.rs"]
mod parametric_edit;
#[path = "object_path_effects_edit.rs"]
mod path_effects_edit;
#[path = "scene_effect_layer_edit.rs"]
pub(crate) mod scene_effect_layer_edit;
#[path = "object_selection_edit_ops.rs"]
mod selection_edit;
pub(crate) use selection_edit::{PasteDestination, PreparedSelectionEdit};
// L06-L (#2112): sealed raster-item / text-block create, edit and delete over the same revision owner.
#[path = "object_raster_asset_commands.rs"]
mod item_commands;
pub(crate) use item_commands::{ItemEditResult, PreparedItemEdit};
#[path = "object_text_layout_commands.rs"]
mod text_commands;

// #2106 size split: layer create / layer visibility transactions of this owner, moved verbatim.
#[path = "object_layer_create_visibility.rs"]
mod layer_create_visibility;

#[derive(Debug, Clone)]
pub(crate) enum ReferenceEdit {
    Settings {
        name: String,
        visible: bool,
        opacity: f64,
        offset_frames: i64,
    },
    Remove,
}
/// Sealed inside this owner module: callers cannot forge origin or after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedReferenceEdit {
    origin: ObjectSnapshot,
    reference: ProjectReferenceState,
    changed: bool,
}

/// A detached value sealed to this owner's original immutable snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PreparedLayerVisibility {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

/// Structural edit sealed to an existing snapshot, not an authoritative owner.
#[derive(Debug, Clone)]
pub(crate) struct PreparedLayerCreate {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    layer_uid: String,
}

impl PreparedLayerCreate {
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
}

#[derive(Debug)]
pub(crate) struct ObjectFillOwner {
    revisions: ObjectRevisionOwner,
}

impl ObjectFillOwner {
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn replace_admitted_snapshot(&mut self, snapshot: ObjectSnapshot) {
        self.revisions = ObjectRevisionOwner::from_admitted(snapshot);
    }

    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn commit_font_asset(
        &mut self,
        origin: &ObjectSnapshot,
        record: std::sync::Arc<crate::object_document::AssetRecord>,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if origin.snapshot_id() != current.snapshot_id()
            || origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.revisions
            .publish_admitted_font(record)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }

    /// Initial admission only: preserve the already minted inactive identity.
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn from_admitted_snapshot(
        snapshot: ObjectSnapshot,
    ) -> Result<Self, DispatchErrorCode> {
        if snapshot.content_revision() != 0 || snapshot.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        Ok(Self {
            revisions: ObjectRevisionOwner::from_admitted(snapshot),
        })
    }

    #[cfg(feature = "project_media_codec")]
    pub(crate) fn from_project_file(
        instance: String,
        bytes: &[u8],
    ) -> Result<Self, DispatchErrorCode> {
        let snapshot = ObjectSnapshot::admit_project_file(instance, bytes)?;
        Ok(Self {
            revisions: ObjectRevisionOwner::from_admitted(snapshot),
        })
    }
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn commit_reference_attachment(
        &mut self,
        candidate: crate::object_snapshot::PreparedReferenceAttachment,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        let origin = candidate.origin();
        if origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if origin.snapshot_id() != current.snapshot_id()
            || origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed() {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish_admitted_reference(candidate.into_reference())
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    pub(crate) fn prepare_reference_edit(
        &self,
        edit: ReferenceEdit,
    ) -> Result<PreparedReferenceEdit, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let before = origin
            .project()
            .ok_or(DispatchErrorCode::Unavailable)?
            .reference_state();
        let reference = match edit {
            ReferenceEdit::Remove => ProjectReferenceState::NoMedia,
            ReferenceEdit::Settings {
                name,
                visible,
                opacity,
                offset_frames,
            } => {
                if before == ProjectReferenceState::NoMedia {
                    return Err(DispatchErrorCode::Unavailable);
                }
                before
                    .with_settings(name, visible, opacity, offset_frames)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?
            }
        };
        let changed = reference != before;
        Ok(PreparedReferenceEdit {
            origin,
            reference,
            changed,
        })
    }
    pub(crate) fn commit_reference_edit(
        &mut self,
        candidate: PreparedReferenceEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish_reference(candidate.reference)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    /// Private typed entry, not an import/host/resource route. PNG was admitted
    /// upstream; even a fabricated ProjectEnvelope drawing is revalidated here.
    pub(crate) fn from_admitted_project(
        instance_id: impl Into<String>,
        project: ProjectEnvelope,
    ) -> Result<Self, &'static str> {
        let instance_id = instance_id.into();
        if instance_id.is_empty() {
            return Err("instanceId must be nonempty");
        }
        ObjectSnapshot::validate_drawing(project.drawing())
            .map_err(|_| "project drawing admission failed")?;
        let snapshot = ObjectSnapshot::from_admitted_project(instance_id, project)?;
        Ok(Self {
            revisions: ObjectRevisionOwner::from_admitted(snapshot),
        })
    }
    pub(crate) fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        let snapshot = ObjectSnapshot::new(instance_id, document)
            .map_err(|_| "object document admission failed")?;
        Ok(Self {
            revisions: ObjectRevisionOwner::from_admitted(snapshot),
        })
    }
    pub(crate) fn instance_id(&self) -> &str {
        self.revisions.instance_id()
    }
    pub(crate) fn document_id(&self) -> &str {
        self.revisions.document_id()
    }
    pub(crate) fn content_revision(&self) -> u64 {
        self.revisions.content_revision()
    }
    pub(crate) fn acquire_snapshot(&self, revision: u64) -> Option<ObjectSnapshot> {
        self.revisions.acquire(revision)
    }
    pub(crate) fn prepare_fill_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedObjectFill, DispatchErrorCode> {
        prepare_object_fill_json(&self.current_snapshot(), bytes)
    }
    pub(crate) fn commit_fill(
        &mut self,
        candidate: PreparedObjectFill,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.base_instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.base_document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.base_snapshot_id() != current.snapshot_id()
            || candidate.base_content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed() {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document().clone())
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    pub(crate) fn restore(&mut self, revision: u64) -> Result<u64, DispatchErrorCode> {
        self.revisions
            .restore(revision)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    fn current_snapshot(&self) -> ObjectSnapshot {
        self.acquire_snapshot(self.content_revision())
            .expect("current object revision has a retained immutable snapshot")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sealed_candidate_pins_existing_bytes_after_owner_drop_without_binary_copy() {
        let document: ObjectDocument = serde_json::from_value(serde_json::json!({
            "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
            "layers":[{"layerUid":"L"}],"objects":[]}))
        .unwrap();
        let project = ProjectEnvelope::no_media(
            crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
            document,
        );
        // Opaque already-admitted buffer for retention only, not PNG validation.
        let encoded = vec![1, 2, 3];
        let pointer = encoded.as_ptr();
        let state = ProjectReferenceState::admitted_still(
            "media-1".into(),
            "before.png".into(),
            true,
            1.,
            0,
            encoded,
            3,
        )
        .unwrap();
        let owner =
            ObjectFillOwner::from_admitted_project("candidate", project.with_reference(state))
                .unwrap();
        let candidate = owner
            .prepare_reference_edit(ReferenceEdit::Settings {
                name: "after.png".into(),
                visible: false,
                opacity: 0.5,
                offset_frames: 2,
            })
            .unwrap();
        drop(owner);
        let ProjectReferenceState::StillImage(before) =
            candidate.origin.project().unwrap().reference_state()
        else {
            panic!()
        };
        let ProjectReferenceState::StillImage(after) = candidate.reference else {
            panic!()
        };
        assert_eq!((before.name(), after.name()), ("before.png", "after.png"));
        assert_eq!(before.encoded_png(), after.encoded_png());
        assert_eq!(before.encoded_png().as_ptr(), pointer);
        assert_eq!(after.encoded_png().as_ptr(), pointer);
    }
}

// Whole-cell Move and atomic bar Shift share this existing native owner.
#[path = "object_frame_bulk_move.rs"]
mod object_frame_bulk_move;
pub(crate) use object_frame_bulk_move::{PreparedBulkFrameEdit, PreparedLayerShift};
