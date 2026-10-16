//! Private bridge from codec admission and sealed fill origins to revision storage.
use crate::commands::{prepare_object_fill_json, PreparedObjectFill};
use crate::object_document::{
    object_deserialize, AuthoredOrder, ObjectDocument, ObjectLayer, OrderLayer, Presence,
    ProjectEnvelope, ProjectReferenceState,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectRevisionOwner;
use serde::Deserialize;
use std::collections::HashSet;
#[path = "object_property_edit.rs"]
mod property_edit;
pub(crate) use property_edit::PreparedPropertyEdit;
#[path = "object_project_settings.rs"]
mod project_settings;
pub(crate) use project_settings::PreparedProjectSettings;

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

#[derive(Debug)]
struct LayerVisibilityPayload {
    command: String,
    layer_uid: String,
    visible: bool,
}
object_deserialize!(LayerVisibilityPayload {
    command: String,
    layer_uid: String,
    visible: bool
});

/// A detached value sealed to this owner's original immutable snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PreparedLayerVisibility {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

#[derive(Debug)]
struct LayerCreatePayload {
    command: String,
    name: String,
}
object_deserialize!(LayerCreatePayload {
    command: String,
    name: String
});

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
    pub(crate) fn prepare_layer_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: LayerCreatePayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.create" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let next = origin
            .content_revision()
            .checked_add(1)
            .filter(|revision| *revision <= 9_007_199_254_740_991)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let mut document = origin.document().clone();
        // Revision identity is monotonic even across undo/redo. Check imported
        // IDs as well; neither preparation nor a refused commit consumes an ID.
        let layer_uid = {
            let existing: HashSet<_> = document.layers.iter().map(ObjectLayer::layer_uid).collect();
            (0..=existing.len())
                .map(|suffix| format!("native-layer:{next}:{suffix}"))
                .find(|id| !existing.contains(id.as_str()))
                .ok_or(DispatchErrorCode::Unavailable)?
        };
        let natural_position =
            u32::try_from(document.layers.len()).map_err(|_| DispatchErrorCode::Unavailable)?;
        if let Presence::Present(plan) = &mut document.composition_order {
            for context in &mut plan.contexts {
                context.layers.push(OrderLayer {
                    layer_uid: layer_uid.clone(),
                    natural_position,
                    order: AuthoredOrder {
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    },
                });
            }
        }
        document.layers.push(ObjectLayer::blank_drawing(
            layer_uid.clone(),
            payload.name,
            document.layers.len(),
        ));
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedLayerCreate {
            origin,
            document,
            layer_uid,
        })
    }
    pub(crate) fn commit_layer_create(
        &mut self,
        candidate: PreparedLayerCreate,
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
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    pub(crate) fn prepare_layer_visibility_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerVisibility, DispatchErrorCode> {
        let payload: LayerVisibilityPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.visibility.set" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let index = origin
            .document()
            .layers
            .iter()
            .position(|layer| layer.layer_uid() == payload.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let changed = origin.document().layers[index].visible() != payload.visible;
        let mut document = origin.document().clone();
        if changed {
            document.layers[index].visible = Presence::Present(payload.visible);
        }
        Ok(PreparedLayerVisibility {
            origin,
            document,
            changed,
        })
    }
    pub(crate) fn commit_layer_visibility(
        &mut self,
        candidate: PreparedLayerVisibility,
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
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
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
