//! Sealed layer compositing transactions (G11, L08-J) through the existing revision owner: the blend,
//! first-matte, additional-matte, effect-stack, borrowed-stack and folder-collapse writes of the baseline
//! panels, plus creation of an effect or folder layer. Declared as a private child of `object_fill_owner.rs`
//! next to the layer policy / visibility / group preparations it mirrors.
//!
//! Preparation parses a bounded, strict payload (`scene_effect_layer_payload.rs`), reads the owner's current
//! immutable snapshot, edits ONE detached layer and runs the storage admission a reopened file would run
//! before sealing the candidate to that snapshot. Commit re-checks instance, document and revision exactly
//! like `commit_layer_policy`; an edit that leaves the layer equal publishes nothing and records nothing.
//! No history, receipt or id state lives here.
//!
//! Baseline rules kept (each pinned by an oracle row): the blend write clears the member for Normal; picking
//! a matte source on a layer with no real mode sets `alpha`; removing the first matte removes its additional
//! mattes with it; removing the last additional matte removes the member; a layer cannot matte itself;
//! "+" needs a matte that is on. Creation reuses the one layer-create path (unique monotonic id, order-plan
//! insertion) and replaces the blank layer of the SAME candidate with the effect or folder layer, so there is
//! one commit and one history entry.
//!
//! Not provided: the `matteOn` track removal that the matte menu also performs (property tracks belong to
//! the property owners), locked-layer policy (the baseline does not guard these panels), the picker UI.
use crate::commands::{ObjectFillOwner, PreparedLayerCreate};
use crate::object_document::{blend_write, MatteRef, ObjectDocument, ObjectLayer, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "scene_effect_layer_payload.rs"]
mod wire;
use wire::{edit_of, id_ok, name_ok, parse, CreatePayload, Edit, EditPayload};

/// Detached, non-Clone candidate: only this child can construct its sealed origin.
#[derive(Debug)]
pub(crate) struct PreparedLayerCompositing {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

#[allow(dead_code)]
impl PreparedLayerCompositing {
    /// `false`: nothing will be published or recorded.
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.document
    }
}

/// The layers an edit points at must exist and must not be the edited layer itself.
fn check_references(
    document: &ObjectDocument,
    layer_uid: &str,
    edit: &Edit,
) -> Result<(), DispatchErrorCode> {
    let source = match edit {
        Edit::MatteSource(uid)
        | Edit::EffectsFromSet(uid)
        | Edit::MatteMoreAdd { source: uid, .. } => Some(uid),
        Edit::MatteMoreSet {
            source: Some(uid), ..
        } => Some(uid),
        _ => None,
    };
    let uid = match source {
        Some(uid) => uid,
        None => return Ok(()),
    };
    if uid == layer_uid {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if document.layers.iter().any(|layer| layer.layer_uid() == uid) {
        Ok(())
    } else {
        Err(DispatchErrorCode::NotFound)
    }
}

fn apply(layer: &mut ObjectLayer, edit: &Edit) -> Result<(), DispatchErrorCode> {
    match edit {
        Edit::Blend(mode) => {
            layer.blend_mode = match blend_write(mode) {
                Some(stored) => Presence::Present(stored.to_string()),
                None => Presence::Missing,
            };
        }
        Edit::MatteSource(uid) => {
            layer.matte_source_layer_uid = Presence::Present(uid.clone());
            // `if (!ld.matteMode || ld.matteMode === 'none') ld.matteMode = 'alpha'`
            if layer
                .matte_mode()
                .map_or(true, |mode| mode.is_empty() || mode == "none")
            {
                layer.matte_mode = Presence::Present("alpha".to_string());
            }
        }
        Edit::MatteMode(mode) => layer.matte_mode = Presence::Present(mode.clone()),
        Edit::MatteClear => {
            layer.matte_mode = Presence::Missing;
            layer.matte_source_layer_uid = Presence::Missing;
            layer.mattes_more = Presence::Missing;
        }
        Edit::MatteMoreAdd { source, mode } => {
            if !layer.matte_on() {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let mut extras = layer.mattes_more().to_vec();
            extras.push(MatteRef {
                uid: source.clone(),
                mode: mode.clone(),
            });
            layer.mattes_more = Presence::Present(extras);
        }
        Edit::MatteMoreSet {
            index,
            source,
            mode,
        } => {
            let mut extras = layer.mattes_more().to_vec();
            let entry = extras
                .get_mut(*index)
                .ok_or(DispatchErrorCode::InvalidRequest)?;
            if let Some(uid) = source {
                entry.uid = uid.clone();
            }
            if let Some(mode) = mode {
                entry.mode = mode.clone();
            }
            layer.mattes_more = Presence::Present(extras);
        }
        Edit::MatteMoreRemove(index) => {
            let mut extras = layer.mattes_more().to_vec();
            if *index >= extras.len() {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            extras.remove(*index);
            layer.mattes_more = if extras.is_empty() {
                Presence::Missing
            } else {
                Presence::Present(extras)
            };
        }
        Edit::Effects(stack) => layer.effects = Presence::Present(stack.clone()),
        Edit::EffectsFromSet(uid) => layer.effects_from = Presence::Present(uid.clone()),
        Edit::EffectsFromClear => layer.effects_from = Presence::Missing,
        Edit::FolderCollapsed(value) => {
            if !layer.is_folder_layer() {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            layer.folder_collapsed = Presence::Present(*value);
        }
    }
    Ok(())
}

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_compositing_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCompositing, DispatchErrorCode> {
        let payload: EditPayload = parse(bytes)?;
        if payload.command != "object.layer.compositing.edit" || !id_ok(&payload.layer_uid) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let edit = edit_of(&payload)?;
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let index = document
            .layers
            .iter()
            .position(|layer| layer.layer_uid() == payload.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        check_references(&document, &payload.layer_uid, &edit)?;
        apply(&mut document.layers[index], &edit)?;
        let changed = document.layers[index] != origin.document().layers[index];
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedLayerCompositing {
            origin,
            document,
            changed,
        })
    }

    pub(crate) fn commit_layer_compositing(
        &mut self,
        candidate: PreparedLayerCompositing,
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

    /// Creation of an `effect` or `folder` layer: the layer-create candidate whose appended blank layer is
    /// replaced by the kind layer. Committed through the ordinary `commit_layer_create`.
    pub(crate) fn prepare_layer_compositing_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        let payload: CreatePayload = parse(bytes)?;
        if payload.command != "object.layer.compositing.create" || !name_ok(&payload.name) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let folder = match payload.kind.as_str() {
            "effect" => false,
            "folder" => true,
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        let base = serde_json::to_vec(
            &serde_json::json!({"command": "object.layer.create", "name": payload.name.clone()}),
        )
        .map_err(|_| DispatchErrorCode::Internal)?;
        let mut candidate = self.prepare_layer_create_json(&base)?;
        let existing = candidate.document.layers.len().saturating_sub(1);
        let uid = candidate.layer_uid.clone();
        let slot = candidate
            .document
            .layers
            .last_mut()
            .ok_or(DispatchErrorCode::Internal)?;
        *slot = if folder {
            ObjectLayer::blank_folder_layer(uid, payload.name, existing)
        } else {
            ObjectLayer::blank_effect_layer(uid, payload.name, existing)
        };
        ObjectSnapshot::validate_drawing(&candidate.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(candidate)
    }
}
