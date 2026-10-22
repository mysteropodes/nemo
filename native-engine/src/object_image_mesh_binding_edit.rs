//! T11/#2242 detached binding planner, a COMMANDS child of object_image_mesh_edit.rs.
//! The parent routes preparation through the existing sealed owner and history.
//!
//! Required read-queries port in object_image_mesh_edit_read.rs (never a document query):
//! BindingState { store: ImageMeshStore, sequence: MeshIdSequence,
//!                sets: Vec<Vec<BindingLayer>> }; BindingLayer { frames: Vec<Vec<BindingSlot>> };
//! BindingSlot::{Raster(RasterItem), NonRaster}; BindingTarget { set, layer, frame, slot: usize };
//! BindingOperation::{Attach { target, mesh }, Detach { target },
//!   Propagate { target, mesh_id: Option<String> }, ReleaseIfUnused { mesh_id: String },
//!   DuplicateLayer { set: usize, layer: usize }};
//! apply_binding(BindingState, BindingOperation) -> Result<(BindingState, BindingReport),
//!   BindingRefusal::{Unavailable, UnknownTarget, InvalidInput}>.
//! The port converts these plain projections to the EXISTING binding kernel and back; it must
//! not reconstruct the sequence. BindingReport variants are matched below. No kernel is copied here.
//! BindingState::raster_at reads a checked returned projection without cloning the kernel state;
//! BindingTarget is a Copy value of four primitive indices.
//!
//! Shared composition must route binding actions before the old meshId-required payload parse,
//! retain origin_sequence in the sealed candidate, and compare it at commit even for a no-op.
//! The sole owner retains its sequence through undo/redo and replacement (observing replacement
//! store ids can only raise it). Preparation/refusal/cancellation cannot advance that owner value.
//! Publish tags + store once, then record the new sequence and existing history/receipt normally.
//!
//! Native positional identity is naturalPosition, not animated/evaluated paint Order. A sparse
//! absent authored scope is an empty frame; it is counted as skipped without materializing a
//! totalFrames-sized array. Exposure holds are not extra authored frames. Every writeback uses
//! the full retained ObjectTarget, never a filtered raster ordinal or a stroke-id-only lookup.
//!
//! Each declared component uses its own layers, frame bounds and explicit mixed order. The
//! frame reader's component-item availability is a separate outstanding consumer requirement.
//! Video layer records remain unavailable until their actual provider joins.
use crate::object_document::{
    object_deserialize, ImageMesh, MeshIdSequence, ObjectDocument, ObjectTarget, Presence,
};
pub(crate) use crate::object_snapshot::object_image_mesh_edit_read::BindingReport;
use crate::object_snapshot::object_image_mesh_edit_read::{
    apply_binding, BindingOperation, BindingRefusal,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

const ROOT: &str = "scene-root";
const MAX_PAYLOAD_BYTES: usize = 65_536;

#[path = "object_image_mesh_binding_projection.rs"]
mod projection;
use projection::{layer_target, project, slot_target};

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Action {
    Attach {
        target: ObjectTarget,
        mesh: ImageMesh,
    },
    Detach {
        target: ObjectTarget,
    },
    Propagate {
        target: ObjectTarget,
        mesh_id: Presence<Option<String>>,
    },
    ReleaseIfUnused {
        mesh_id: String,
    },
    Duplicate {
        context_id: String,
        layer_uid: String,
    },
    AttachVideo {
        context_id: String,
        layer_uid: String,
        mesh: ImageMesh,
    },
    DetachVideo {
        context_id: String,
        layer_uid: String,
    },
}

#[derive(Debug)]
struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});

pub(crate) fn binding_action_kind(kind: &str) -> bool {
    [
        "attach",
        "detach",
        "propagate",
        "releaseIfUnused",
        "duplicate",
        "attachVideo",
        "detachVideo",
    ]
    .contains(&kind)
}

/// Detached proposal only. The parent's private PreparedImageMeshEdit seals this to its origin.
#[derive(Debug)]
pub(crate) struct BindingEditPlan {
    document: ObjectDocument,
    origin_sequence: MeshIdSequence,
    sequence: MeshIdSequence,
    report: BindingReport,
    changed: bool,
}

impl BindingEditPlan {
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.document
    }
    pub(crate) fn origin_sequence(&self) -> MeshIdSequence {
        self.origin_sequence
    }
    pub(crate) fn sequence(&self) -> MeshIdSequence {
        self.sequence
    }
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
    pub(crate) fn report(&self) -> &BindingReport {
        &self.report
    }
    pub(crate) fn into_parts(self) -> (ObjectDocument, MeshIdSequence, BindingReport) {
        (self.document, self.sequence, self.report)
    }
}

fn error(refusal: BindingRefusal) -> DispatchErrorCode {
    match refusal {
        BindingRefusal::Unavailable => DispatchErrorCode::Unavailable,
        BindingRefusal::UnknownTarget => DispatchErrorCode::NotFound,
        BindingRefusal::InvalidInput => DispatchErrorCode::InvalidRequest,
    }
}

fn remap_id(id: &mut Presence<String>, pairs: &[(String, String)]) {
    if let Some((_, new)) = pairs.iter().find(|(old, _)| id.value() == Some(old)) {
        *id = Presence::Present(new.clone());
    }
}

pub(crate) fn plan_binding_edit_json(
    origin: &ObjectSnapshot,
    sequence: MeshIdSequence,
    bytes: &[u8],
) -> Result<BindingEditPlan, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let payload: Payload =
        serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    if payload.command != "object.image.mesh.edit" {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let document = origin.document();
    // Store release requires a complete reference scan, not a positional ordering provider.
    if matches!(
        &payload.action,
        Action::AttachVideo { .. } | Action::DetachVideo { .. }
    ) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let positional = !matches!(&payload.action, Action::ReleaseIfUnused { .. });
    let projection = project(document, sequence, positional)?;
    let mut missing = 0;
    let mut duplicated_layer = None;
    let operation = match payload.action {
        Action::Attach { target, mesh } => {
            let at = slot_target(document, &projection, &target)?;
            missing = projection.missing[at.set][at.layer];
            BindingOperation::Attach { target: at, mesh }
        }
        Action::Detach { target } => {
            let at = slot_target(document, &projection, &target)?;
            missing = projection.missing[at.set][at.layer];
            BindingOperation::Detach { target: at }
        }
        Action::Propagate { target, mesh_id } => {
            let Presence::Present(mesh_id) = mesh_id else {
                return Err(DispatchErrorCode::InvalidRequest);
            };
            let at = slot_target(document, &projection, &target)?;
            missing = projection.missing[at.set][at.layer];
            BindingOperation::Propagate {
                target: at,
                mesh_id,
            }
        }
        Action::ReleaseIfUnused { mesh_id } => BindingOperation::ReleaseIfUnused { mesh_id },
        Action::Duplicate {
            context_id,
            layer_uid,
        } => {
            let (set, layer) = layer_target(document, &context_id, &layer_uid)?;
            duplicated_layer = Some((context_id, layer));
            BindingOperation::DuplicateLayer { set, layer }
        }
        Action::AttachVideo { .. } | Action::DetachVideo { .. } => {
            return Err(DispatchErrorCode::Unavailable)
        }
    };
    let (state, mut report) = apply_binding(projection.state, operation).map_err(error)?;
    match &mut report {
        BindingReport::Attach { skipped, .. }
        | BindingReport::Propagate { skipped, .. }
        | BindingReport::Detach { skipped, .. } => *skipped += missing,
        BindingReport::Released(_) | BindingReport::Duplicated { .. } => {}
    }
    let mut next = document.clone();
    if let Presence::Present(items) = &mut next.items {
        if let Presence::Present(rasters) = &mut items.raster {
            for location in projection.locations {
                let item = state
                    .raster_at(location.at)
                    .map_err(|_| DispatchErrorCode::Internal)?;
                // Source index is provenance, not identity: check the complete retained target
                // before writing, then avoid a second linear raster-table search.
                let entry = rasters
                    .get_mut(location.source_index)
                    .filter(|entry| entry.target == location.target)
                    .ok_or(DispatchErrorCode::Internal)?;
                entry.item = item.clone();
            }
        }
    }
    // A missing store stays missing for a genuine no-op (including missing-source duplicate).
    if document.image_meshes().is_some() || !state.store.entries().is_empty() {
        next.image_meshes = Presence::Present(state.store);
    }
    if let (Some((context_id, layer)), BindingReport::Duplicated { pairs, .. }) =
        (duplicated_layer, &report)
    {
        // Baseline duplicateLayer rewrites all THREE owners of a mesh id together: raster
        // tags, mesh-keyed elementMotion holders, and rig binds of the copied layer.
        let uid = if context_id == ROOT {
            next.layers[layer].layer_uid()
        } else {
            next.component_set()
                .and_then(|set| set.component(&context_id))
                .and_then(|component| component.layers().get(layer))
                .ok_or(DispatchErrorCode::Internal)?
                .layer_uid()
        }
        .to_owned();
        if let Presence::Present(holders) = &mut next.property_holders {
            for holder in holders.iter_mut().filter(|holder| {
                holder.target.context_id() == context_id && holder.target.layer_uid() == uid
            }) {
                remap_id(&mut holder.target.stroke_id, pairs);
            }
        }
        let destination = if context_id == ROOT {
            next.layers.get_mut(layer)
        } else if let Presence::Present(tables) = &mut next.component_tables {
            if let Presence::Present(set) = &mut tables.components {
                set.components
                    .iter_mut()
                    .find(|component| component.symbol_id() == context_id)
                    .and_then(|component| component.layers.get_mut(layer))
            } else {
                None
            }
        } else {
            None
        }
        .ok_or(DispatchErrorCode::Internal)?;
        if let Presence::Present(rig) = &mut destination.rig {
            for bind in &mut rig.binds {
                remap_id(&mut bind.mesh_id, pairs);
            }
        }
    }
    ObjectSnapshot::validate_drawing(&next).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    let changed = &next != document;
    Ok(BindingEditPlan {
        document: next,
        origin_sequence: sequence,
        sequence: state.sequence,
        report,
        changed,
    })
}

#[cfg(all(
    test,
    feature = "history",
    feature = "project_media_codec",
    feature = "evaluation"
))]
#[path = "../tests/object_image_mesh_binding_history.rs"]
mod history_tests;
