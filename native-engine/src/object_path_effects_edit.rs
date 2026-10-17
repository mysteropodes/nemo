//! L01-G (G13): sealed replacement of a layer's ordered path-effect stack through the one revision owner.
//! Strict original-byte payload, one detached candidate document sealed to the immutable origin snapshot, full
//! storage admission before anything can publish, and publication only through the existing
//! `ObjectRevisionOwner`. The edit touches `ObjectLayer::path_fx` and nothing else: no record, no anchor and no
//! other layer changes, so every source geometry stays exactly as stored (non-destructive by construction).
//!
//! Wire (operation `command.document.object.path.effects.edit`):
//! `{"command":"object.path.effects.set","layerUid":"L1","stack":[{"type":"zigzag","enabled":true,"p1":12,...}]}`
//! The stack is the COMPLETE new ordered list (the baseline panel's add, toggle, edit and delete are all
//! `ld.pathFx` array writes). Storage order is evaluation order, so a reorder is a real change; `enabled` is
//! absent, true or false and `p1..p3` are absent, null or numbers, each kept exactly as sent. An unknown effect
//! type or a non-finite parameter refuses the WHOLE request at storage admission. An empty stack is an authored
//! empty stack, distinct from a layer that never had one (the baseline `ld.pathFx = []` after the last delete).
//! A stack that equals the stored one (numbers compared by value, `10` and `10.0` alike) preserves bytes,
//! history and redo.
//!
//! The baseline panel (effects-panel.js `pathFxHost`) accepts every layer except a Null layer and never checks
//! the lock, so this edit does not either: path effects are not Drawing content. The stack is stored only; the
//! deformed geometry is evaluated by the L01-H join at a pinned snapshot and frame.
use crate::commands::object_fill_owner::vector_edit::same_value;
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, ObjectDocument, ObjectLayer, PathFxRecord, Presence, PATH_FX_TYPE_IDS,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug)]
struct PathEffectsPayload {
    command: String,
    layer_uid: String,
    stack: Vec<PathFxRecord>,
}
object_deserialize!(PathEffectsPayload {
    command: String,
    layer_uid: String,
    stack: Vec<PathFxRecord>
});

/// Detached candidate sealed to the origin snapshot; its fields are private to this module, so a caller
/// cannot forge an origin or a document. It is not an authoritative snapshot.
#[derive(Debug)]
pub(crate) struct PreparedPathEffectsEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

/// `SMPathFx.hasAny(ld)`: at least one entry that `activeStack` keeps (`enabled !== false` and a known type).
/// The baseline refuses to draw on such a layer (tools.js `editRefusalReason`, line 1817): what is on screen is
/// the deformed geometry, so a stroke added or replaced there would be written over the original drawing.
pub(crate) fn layer_has_active_path_fx(layer: &ObjectLayer) -> bool {
    layer.path_fx().is_some_and(|stack| {
        stack.iter().any(|entry| {
            entry.enabled_flag() != Some(false)
                && PATH_FX_TYPE_IDS.contains(&entry.fx_type.as_str())
        })
    })
}

fn same_stack(before: &Presence<Vec<PathFxRecord>>, after: &Presence<Vec<PathFxRecord>>) -> bool {
    match (before, after) {
        (Presence::Missing, Presence::Missing) => true,
        (Presence::Present(_), Presence::Present(_)) => {
            match (serde_json::to_value(before), serde_json::to_value(after)) {
                (Ok(before), Ok(after)) => same_value(&before, &after),
                _ => false,
            }
        }
        _ => false,
    }
}

impl ObjectFillOwner {
    /// Strict payload preparation directly from the original bytes (a `Value` would erase duplicate members).
    /// The returned candidate is sealed to this owner's current immutable snapshot and already passed the
    /// complete storage admission of the edited document; nothing is published.
    pub(crate) fn prepare_path_effects_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedPathEffectsEdit, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: PathEffectsPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.path.effects.set" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let layer = document
            .layers
            .iter_mut()
            .find(|layer| layer.layer_uid() == payload.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let next = Presence::Present(payload.stack);
        let changed = !same_stack(&layer.path_fx, &next);
        if changed {
            layer.path_fx = next;
        }
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedPathEffectsEdit {
            origin,
            document,
            changed,
        })
    }

    /// Publish a sealed candidate if and only if the origin pins (instance, document, snapshot, content
    /// revision) still match the current head. An exact no-op returns the unchanged head revision.
    pub(crate) fn commit_path_effects_edit(
        &mut self,
        candidate: PreparedPathEffectsEdit,
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
}

#[cfg(test)]
#[path = "../tests/object_path_effects_edit_parity.rs"]
mod parity_tests;
