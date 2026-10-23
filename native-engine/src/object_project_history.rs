//! Project-file and still-reference consumers of the one object history owner.
use crate::history::object_history::{
    NativeObjectHistory, ObjectFillOwner, ObjectHistoryEntry, StageReceipts,
};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;

/// Immutable admitted source, bound to the original owner. No writable history
/// or replacement authority exists until this token is consumed by that owner.
#[derive(Debug)]
pub struct PreparedProjectReplacement {
    origin: ObjectSnapshot,
    next: ObjectSnapshot,
}

/// An admitted immutable project, never a second history or an activation token.
/// Clones share the snapshot's existing charged allocations. Origin pins record
/// admission provenance only; a later host operation must check its live owner.
#[derive(Clone)]
pub struct InactiveProject {
    snapshot: ObjectSnapshot,
    origin_document: String,
    origin_snapshot: String,
    origin_revision: u64,
}

impl InactiveProject {
    /// Detached, codec-bounded metadata; no document bytes or writable handle.
    pub fn metadata(&self) -> Result<serde_json::Value, DispatchErrorCode> {
        let mut value = project_snapshot_info(&self.snapshot)?;
        value["instanceId"] = self.snapshot.instance_id().into();
        value["documentId"] = self.snapshot.document_id().into();
        value["origin"] = serde_json::json!({
            "documentId":self.origin_document, "documentSnapshotId":self.origin_snapshot,
            "contentRevision":self.origin_revision});
        Ok(value)
    }
}

#[path = "object_reference_dispatch.rs"]
mod reference_dispatch;
// L06-H (lane #2112): asset attach / remove commands.
#[path = "object_raster_asset_dispatch.rs"]
#[allow(dead_code)]
mod asset_dispatch;

// Both initial New and replacement admit this same fully specified blank value.
// It has no external resources and cannot carry an unchecked drawing from a caller.
fn blank_project(
    canvas_width: u32,
    canvas_height: u32,
    fps: f64,
) -> Result<crate::object_document::ProjectEnvelope, DispatchErrorCode> {
    use crate::object_document::{
        AuthoredOrder, FrameScope, FrameScopeKind, ObjectDocument, ObjectLayer, OrderContext,
        OrderLayer, OrderPlan, OrderScope, Presence, ProjectEnvelope, ProjectMetadata,
        OBJECT_DOCUMENT_FORMAT,
    };
    let metadata = ProjectMetadata::new(canvas_width, canvas_height, fps)
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
    let frames = (fps * 5.0).round().max(1.0);
    if !frames.is_finite() || frames > u32::MAX as f64 {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let metadata = metadata
        .with_canvas_background(Some("#ffffff"))
        .and_then(|value| value.with_work_area(Some(0.0), Some(frames - 1.0)))
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
    let drawing = ObjectDocument {
        format: OBJECT_DOCUMENT_FORMAT.into(),
        format_version: 2,
        total_frames: frames as u32,
        layers: vec![ObjectLayer::blank_drawing(
            "layer-1".into(),
            "Layer 1".into(),
            0,
        )],
        objects: vec![],
        property_holders: Presence::Missing,
        layer_folders: Presence::Missing,
        // Native painting consumes the explicit v2 order plan even when empty.
        composition_order: Presence::Present(OrderPlan {
            format: "nemo.native-object-order-plan".into(),
            format_version: 1,
            document_format_version: 2,
            contexts: vec![OrderContext {
                context_id: "scene-root".into(),
                layers: vec![OrderLayer {
                    layer_uid: "layer-1".into(),
                    natural_position: 0,
                    order: AuthoredOrder {
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    },
                }],
                scopes: vec![OrderScope {
                    frame_scope: FrameScope {
                        kind: FrameScopeKind::Authored,
                        frame: 0,
                    },
                    elements: vec![],
                }],
            }],
            migration: "explicit-plan-required".into(),
            reference_policy: "unresolved".into(),
        }),
        image_meshes: Presence::Missing,
        items: Presence::Missing,
        component_tables: Presence::Missing,
        motion_arcs: Presence::Missing,
        audio_tracks: Presence::Missing,
    };
    Ok(ProjectEnvelope::no_media(metadata, drawing))
}

impl NativeObjectHistory {
    /// Create the native blank drawing from explicit New Project intent. The
    /// five-second timeline matches the existing desktop New form. This only
    /// constructs an owner; host replacement and UI publication remain separate.
    pub fn new_blank_project(
        instance: impl Into<String>,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<Self, DispatchErrorCode> {
        let project = blank_project(canvas_width, canvas_height, fps)?;
        let instance = instance.into();
        if instance.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Self::from_admitted_project(instance, project).map_err(|_| DispatchErrorCode::Unavailable)
    }

    /// Prepare New without a second writable history. Existing replacement
    /// validation and cleanup/commit rules also govern this immutable token.
    pub fn prepare_blank_project_replacement(
        &self,
        snapshot_id: &str,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<PreparedProjectReplacement, DispatchErrorCode> {
        let origin = self
            .acquire_snapshot(self.content_revision())
            .filter(|snapshot| snapshot.snapshot_id() == snapshot_id)
            .ok_or(DispatchErrorCode::StaleRevision)?;
        if origin.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let project = blank_project(canvas_width, canvas_height, fps)?;
        let next = ObjectSnapshot::from_admitted_project(self.instance_id().into(), project)
            .map_err(|_| DispatchErrorCode::Unavailable)?;
        Ok(PreparedProjectReplacement { origin, next })
    }

    /// Admit B without replacing A or constructing a second writable owner.
    /// Check A's current snapshot before parsing or reserving B's resources.
    pub fn prepare_inactive_project(
        &self,
        expected_snapshot_id: &str,
        bytes: &[u8],
    ) -> Result<InactiveProject, DispatchErrorCode> {
        let origin = self
            .acquire_snapshot(self.content_revision())
            .filter(|snapshot| snapshot.snapshot_id() == expected_snapshot_id)
            .ok_or(DispatchErrorCode::StaleRevision)?;
        if origin.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let snapshot = ObjectSnapshot::admit_project_file(self.instance_id().into(), bytes)?;
        Ok(InactiveProject {
            snapshot,
            origin_document: origin.document_id().into(),
            origin_snapshot: origin.snapshot_id().into(),
            origin_revision: origin.content_revision(),
        })
    }

    pub fn prepare_project_replacement(
        &self,
        snapshot_id: &str,
        bytes: &[u8],
    ) -> Result<PreparedProjectReplacement, DispatchErrorCode> {
        let origin = self
            .acquire_snapshot(self.content_revision())
            .filter(|snapshot| snapshot.snapshot_id() == snapshot_id)
            .ok_or(DispatchErrorCode::StaleRevision)?;
        if origin.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let next = ObjectSnapshot::admit_project_file(self.instance_id().into(), bytes)?;
        Ok(PreparedProjectReplacement { origin, next })
    }

    pub fn validate_project_replacement(
        &self,
        prepared: &PreparedProjectReplacement,
    ) -> Result<(), DispatchErrorCode> {
        self.validate_project_origin(&prepared.origin)
    }

    fn validate_project_origin(&self, origin: &ObjectSnapshot) -> Result<(), DispatchErrorCode> {
        let current = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
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
        Ok(())
    }

    /// Host reservation/quiescence must precede activation. The caller privately
    /// parks the returned non-Clone A history; only this value stays routed.
    /// Refusal never consumes B. Admission provenance permits later edits in A,
    /// while the supplied live origin and exact target pin control this switch.
    pub fn activate_inactive_project(
        &mut self,
        origin: &ObjectSnapshot,
        target_snapshot_id: &str,
        incoming: &mut Option<InactiveProject>,
    ) -> Result<Self, DispatchErrorCode> {
        self.validate_project_origin(origin)?;
        if origin.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let project = incoming.as_ref().ok_or(DispatchErrorCode::Unavailable)?;
        if project.snapshot.instance_id() != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if project.origin_document != self.document_id()
            || project.snapshot.document_id() == self.document_id()
        {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if project.snapshot.snapshot_id() != target_snapshot_id {
            return Err(DispatchErrorCode::StaleRevision);
        }
        // Clone only the already immutable admitted value, retaining its charge.
        // Every fallible step finishes before taking B or moving either history.
        let owner = ObjectFillOwner::from_admitted_snapshot(project.snapshot.clone())?;
        let next = Self {
            owner,
            undo: Vec::new(),
            redo: Vec::new(),
            receipts: StageReceipts::default(),
        };
        drop(incoming.take());
        Ok(std::mem::replace(self, next))
    }

    /// Swap two complete existing histories under the same host authority.
    /// No snapshot store, redo stack or request receipt is reset or duplicated.
    pub fn swap_project_history(
        &mut self,
        origin: &ObjectSnapshot,
        target_snapshot_id: &str,
        parked: &mut Self,
    ) -> Result<(), DispatchErrorCode> {
        self.validate_project_origin(origin)?;
        if origin.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        if parked.instance_id() != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if parked.document_id() == self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        let target = parked
            .acquire_snapshot(parked.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        if target.snapshot_id() != target_snapshot_id {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if target.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        std::mem::swap(self, parked);
        Ok(())
    }

    /// Host cleanup/reservation precedes this consuming commit. The same owner
    /// receives a fresh document incarnation; A's immutable readers stay valid.
    pub fn commit_project_replacement(
        &mut self,
        prepared: PreparedProjectReplacement,
    ) -> Result<(), DispatchErrorCode> {
        self.validate_project_replacement(&prepared)?;
        self.owner.replace_admitted_snapshot(prepared.next)?;
        self.undo.clear();
        self.redo.clear();
        self.receipts = StageReceipts::default();
        Ok(())
    }

    pub fn from_project_file(
        instance: impl Into<String>,
        bytes: &[u8],
    ) -> Result<Self, DispatchErrorCode> {
        Ok(Self {
            owner: ObjectFillOwner::from_project_file(instance.into(), bytes)?,
            undo: Vec::new(),
            redo: Vec::new(),
            receipts: StageReceipts::default(),
        })
    }

    /// A retained revision may be saved; the exact snapshot pin is mandatory.
    pub fn serialize_project_file(
        &self,
        at_revision: u64,
        snapshot_id: &str,
    ) -> Result<Vec<u8>, DispatchErrorCode> {
        let snapshot = self
            .acquire_snapshot(at_revision)
            .filter(|s| s.snapshot_id() == snapshot_id)
            .ok_or(DispatchErrorCode::StaleRevision)?;
        snapshot.serialize_project_file()
    }

    /// Detached presentation metadata and a digest of the native file bytes.
    /// No drawing authority, media bytes or resource handles cross this port.
    pub fn project_file_info(&self) -> Result<serde_json::Value, DispatchErrorCode> {
        let snapshot = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        let mut value = project_snapshot_info(&snapshot)?;
        value["undoDepth"] = self.undo.len().into();
        value["redoDepth"] = self.redo.len().into();
        Ok(value)
    }

    /// Current immutable project extent without serializing media or file bytes.
    /// Bare drawing snapshots intentionally have no project dimensions.
    pub fn project_canvas_dimensions(&self) -> Result<Option<(u32, u32)>, DispatchErrorCode> {
        let snapshot = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        Ok(snapshot
            .project()
            .map(|project| project.project().canvas_dimensions()))
    }

    pub fn is_project_file(&self) -> bool {
        self.acquire_snapshot(self.content_revision())
            .is_some_and(|s| s.project().is_some())
    }

    /// Admission and publication stay under the same owner borrow. The caller's
    /// expected revision is checked before PNG decode or persistent allocation.
    pub(crate) fn attach_reference_png(
        &mut self,
        expected_revision: u64,
        id: String,
        name: String,
        png: &[u8],
    ) -> Result<u64, DispatchErrorCode> {
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let snapshot: ObjectSnapshot = self
            .acquire_snapshot(expected_revision)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let candidate = snapshot.prepare_reference_attachment(id, name, png)?;
        let after = self.owner.commit_reference_attachment(candidate)?;
        if after != expected_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision: expected_revision,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
}

// Active and inactive reads use the same canonical serialization and metadata.
fn project_snapshot_info(
    snapshot: &ObjectSnapshot,
) -> Result<serde_json::Value, DispatchErrorCode> {
    use crate::object_document::ProjectReferenceState;
    let project = snapshot.project().ok_or(DispatchErrorCode::Unavailable)?;
    let bytes = snapshot.serialize_project_file()?;
    let digest: String = crate::request_receipts::sha256(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let layers: Vec<_> = project
        .drawing()
        .layers
        .iter()
        .map(|layer| {
            let mut info = serde_json::json!({"layerUid": layer.layer_uid()});
            if let Some(name) = layer.name() {
                info["name"] = serde_json::json!(name);
            }
            if let Some(visible) = layer.visible.value() {
                info["visible"] = serde_json::json!(visible);
            }
            if let Some(shy) = layer.shy.value() {
                info["shy"] = serde_json::json!(shy);
            }
            let locked = project.drawing().layer_is_locked(layer.layer_uid());
            if locked || layer.locked.value().is_some() {
                info["locked"] = serde_json::json!(locked);
            }
            if let Some(color) = layer.color.value() {
                info["color"] = serde_json::json!(color);
            }
            if let Some(parent) = layer.parent_layer_uid.value() {
                info["parentLayerUid"] = serde_json::json!(parent);
            }
            info
        })
        .collect();
    let reference = match project.reference_state() {
        ProjectReferenceState::NoMedia => serde_json::Value::Null,
        ProjectReferenceState::StillImage(image) => serde_json::json!({
            "id":image.id(), "name":image.name(), "visible":image.visible(),
            "opacity":image.opacity(), "offsetFrames":image.offset_frames()}),
    };
    let (width, height) = project.project().canvas_dimensions();
    let mut info = serde_json::json!({"documentSnapshotId":snapshot.snapshot_id(),
        "contentRevision":snapshot.content_revision(), "documentDigest":digest,
        "byteCount":bytes.len(), "canvasW":width, "canvasH":height,
        "fps":project.project().fps(), "totalFrames":project.drawing().total_frames,
        "layers":layers, "reference":reference});
    if let Some(background) = project.project().canvas_background() {
        info["canvasBg"] = background.into();
    }
    let (wa_in, wa_out) = project.project().work_area();
    if let Some(value) = wa_in {
        info["waIn"] = value.into();
    }
    if let Some(value) = wa_out {
        info["waOut"] = value.into();
    }
    Ok(info)
}

#[cfg(test)]
#[path = "../tests/project_media_workflow.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_project_replacement.rs"]
mod replacement_tests;
