//! Private bridge from codec admission and sealed fill origins to revision storage.
use crate::commands::{prepare_object_fill_json, PreparedObjectFill};
use crate::object_document::{ObjectDocument, ProjectEnvelope, ProjectReferenceState};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectRevisionOwner;

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
pub(crate) struct ObjectFillOwner {
    revisions: ObjectRevisionOwner,
}

impl ObjectFillOwner {
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn replace_admitted_snapshot(&mut self, snapshot: ObjectSnapshot) {
        self.revisions = ObjectRevisionOwner::from_admitted(snapshot);
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
