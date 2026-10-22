//! T11/#2242 detached binding planner, a COMMANDS child of object_image_mesh_edit.rs.
//! This file is intentionally unregistered until the shared owner/read port is composed.
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
//! BindingState::raster_at(BindingTarget) -> Result<&RasterItem, BindingRefusal> forwards the
//! existing kernel read; BindingTarget is a Copy value of four primitive indices.
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
//! Component item admission still resolves against root layers/frame bounds, and the component
//! frame reader refuses items. This planner retains their references but refuses component writes
//! until those real consumers join. Video layer records are likewise unavailable. These are
//! outstanding acceptance requirements, not removed baseline functionality.
use crate::object_document::{
    object_deserialize, FrameScopeKind, ImageMesh, ImageMeshStore, MeshIdSequence, ObjectDocument,
    ObjectLayer, ObjectRecord, ObjectTarget, Presence,
};
pub(crate) use crate::object_snapshot::object_image_mesh_edit_read::BindingReport;
use crate::object_snapshot::object_image_mesh_edit_read::{
    apply_binding, BindingLayer, BindingOperation, BindingRefusal, BindingSlot, BindingState,
    BindingTarget,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

const ROOT: &str = "scene-root";
const MAX_PAYLOAD_BYTES: usize = 65_536;

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

struct Context<'a> {
    id: &'a str,
    layers: &'a [ObjectLayer],
    objects: &'a [ObjectRecord],
    frames: u32,
}

struct Location {
    target: ObjectTarget,
    at: BindingTarget,
}

struct Projection {
    state: BindingState,
    locations: Vec<Location>,
    missing: Vec<Vec<usize>>,
}

fn contexts(document: &ObjectDocument) -> Vec<Context<'_>> {
    let mut found = vec![Context {
        id: ROOT,
        layers: document.layers(),
        objects: document.objects(),
        frames: document.total_frames(),
    }];
    if let Some(set) = document.component_set() {
        found.extend(set.components.iter().map(|c| Context {
            id: c.symbol_id(),
            layers: c.layers(),
            objects: c.objects(),
            frames: c.total_frames(),
        }));
    }
    found
}

fn error(refusal: BindingRefusal) -> DispatchErrorCode {
    match refusal {
        BindingRefusal::Unavailable => DispatchErrorCode::Unavailable,
        BindingRefusal::UnknownTarget => DispatchErrorCode::NotFound,
        BindingRefusal::InvalidInput => DispatchErrorCode::InvalidRequest,
    }
}

fn empty_projection(
    document: &ObjectDocument,
    mut sequence: MeshIdSequence,
) -> Result<Projection, DispatchErrorCode> {
    let store = match document.image_meshes() {
        Some(store) => store.clone(),
        None => ImageMeshStore::try_new(vec![]).map_err(|_| DispatchErrorCode::InvalidRequest)?,
    };
    sequence
        .observe_store(&store)
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(Projection {
        state: BindingState {
            store,
            sequence,
            sets: vec![],
        },
        locations: vec![],
        missing: vec![],
    })
}

/// No persistent record is rewritten here; the map retains exact native provenance for writeback.
fn project(
    document: &ObjectDocument,
    sequence: MeshIdSequence,
    positional: bool,
) -> Result<Projection, DispatchErrorCode> {
    let items = document.item_records();
    let rasters = items.map_or(&[][..], |items| items.raster_entries());
    let texts = items.map_or(&[][..], |items| items.text_entries());
    let mut projected = HashSet::new();
    let mut result = empty_projection(document, sequence)?;
    let contexts = if positional {
        contexts(document)
    } else {
        vec![]
    };
    for (set_index, context) in contexts.iter().enumerate() {
        let plan = document.composition_order.value().and_then(|plan| {
            plan.contexts
                .iter()
                .find(|entry| entry.context_id == context.id)
        });
        let mut layers = vec![];
        let mut missing = vec![];
        for (layer_index, layer) in context.layers.iter().enumerate() {
            // A target plus optional actual raster; nonrasters occupy their real mixed positions.
            let mut frames: BTreeMap<u32, Vec<(&ObjectTarget, Option<usize>)>> = BTreeMap::new();
            let in_layer = |target: &ObjectTarget| {
                target.context_id() == context.id
                    && target.layer_uid() == layer.layer_uid()
                    && target.scope_kind() == &FrameScopeKind::Authored
                    && target.frame() < context.frames
            };
            let targets = context
                .objects
                .iter()
                .map(|object| (object.target(), None))
                .chain(
                    rasters
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| (&entry.target, Some(index))),
                )
                .chain(texts.iter().map(|entry| (&entry.target, None)));
            for (target, raster) in targets.filter(|(target, _)| in_layer(target)) {
                frames
                    .entry(target.frame())
                    .or_default()
                    .push((target, raster));
            }
            let mut projected_frames = vec![];
            for (frame_number, mut slots) in frames {
                let scope = plan.and_then(|plan| {
                    plan.scopes.iter().find(|scope| {
                        scope.frame_scope.kind == FrameScopeKind::Authored
                            && scope.frame_scope.frame == frame_number
                    })
                });
                if let Some(scope) = scope {
                    let mut positioned = vec![];
                    for (target, raster) in slots {
                        let element = scope
                            .elements
                            .iter()
                            .find(|entry| {
                                entry.layer_uid == target.layer_uid
                                    && entry.stroke_id == target.stroke_id
                            })
                            .ok_or(DispatchErrorCode::Unavailable)?;
                        positioned.push((element.natural_position, target, raster));
                    }
                    positioned.sort_by_key(|entry| entry.0);
                    if positioned
                        .iter()
                        .enumerate()
                        .any(|(index, entry)| entry.0 as usize != index)
                    {
                        return Err(DispatchErrorCode::Unavailable);
                    }
                    slots = positioned
                        .into_iter()
                        .map(|(_, target, raster)| (target, raster))
                        .collect();
                } else if document.format_version() == 2 && context.id == ROOT {
                    // An admitted v2 root must cover every item/object; never guess a missing scope.
                    return Err(DispatchErrorCode::Unavailable);
                } else if slots.iter().any(|(_, r)| r.is_some())
                    && slots.iter().any(|(_, r)| r.is_none())
                {
                    // No v1 cross-table order exists. Component references are retained below;
                    // component writes are unavailable regardless of this projection.
                    if context.id == ROOT {
                        return Err(DispatchErrorCode::Unavailable);
                    }
                    slots.retain(|(_, raster)| raster.is_none());
                }
                let frame_index = projected_frames.len();
                let mut bound = vec![];
                for (slot_index, (target, raster)) in slots.into_iter().enumerate() {
                    match raster {
                        Some(index) => {
                            projected.insert(index);
                            result.locations.push(Location {
                                target: target.clone(),
                                at: BindingTarget {
                                    set: set_index,
                                    layer: layer_index,
                                    frame: frame_index,
                                    slot: slot_index,
                                },
                            });
                            bound.push(BindingSlot::Raster(rasters[index].item.clone()));
                        }
                        None => bound.push(BindingSlot::NonRaster),
                    }
                }
                projected_frames.push(bound);
            }
            missing.push(context.frames as usize - projected_frames.len());
            layers.push(BindingLayer {
                frames: projected_frames,
            });
        }
        result.state.sets.push(layers);
        result.missing.push(missing);
    }
    // Opaque/unavailable contexts remain real references, not invented editable layers. This
    // reference-only tail is never targetable and prevents release from dropping their meshes.
    let retained: Vec<_> = rasters
        .iter()
        .enumerate()
        .filter(|(i, _)| !projected.contains(i))
        .map(|(_, entry)| BindingSlot::Raster(entry.item.clone()))
        .collect();
    if !retained.is_empty() {
        result.state.sets.push(vec![BindingLayer {
            frames: vec![retained],
        }]);
        result.missing.push(vec![0]);
    }
    Ok(result)
}

fn layer_target(
    document: &ObjectDocument,
    context_id: &str,
    layer_uid: &str,
) -> Result<(usize, usize), DispatchErrorCode> {
    if context_id.is_empty() || layer_uid.is_empty() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let all = contexts(document);
    let (set, context) = all
        .iter()
        .enumerate()
        .find(|(_, c)| c.id == context_id)
        .ok_or(DispatchErrorCode::NotFound)?;
    let layer = context
        .layers
        .iter()
        .position(|l| l.layer_uid() == layer_uid)
        .ok_or(DispatchErrorCode::NotFound)?;
    if context_id != ROOT || document.layer_is_locked(layer_uid) {
        return Err(DispatchErrorCode::Unavailable);
    }
    if context.layers[layer].widget.value().is_some() || context.layers[layer].is_null_layer() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok((set, layer))
}

fn slot_target(
    document: &ObjectDocument,
    projection: &Projection,
    target: &ObjectTarget,
) -> Result<BindingTarget, DispatchErrorCode> {
    if !document.valid_target(target) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if target.scope_kind() != &FrameScopeKind::Authored {
        return Err(DispatchErrorCode::Unavailable);
    }
    layer_target(document, target.context_id(), target.layer_uid())?;
    if target.frame() >= document.total_frames() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    projection
        .locations
        .iter()
        .find(|location| &location.target == target)
        .map(|location| location.at)
        .ok_or(DispatchErrorCode::NotFound)
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
            duplicated_layer = Some(layer);
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
                let entry = rasters
                    .iter_mut()
                    .find(|entry| entry.target == location.target)
                    .ok_or(DispatchErrorCode::Internal)?;
                entry.item = item.clone();
            }
        }
    }
    // A missing store stays missing for a genuine no-op (including missing-source duplicate).
    if document.image_meshes().is_some() || !state.store.entries().is_empty() {
        next.image_meshes = Presence::Present(state.store);
    }
    if let (Some(layer), BindingReport::Duplicated { pairs, .. }) = (duplicated_layer, &report) {
        // Baseline duplicateLayer rewrites all THREE owners of a mesh id together: raster
        // tags, mesh-keyed elementMotion holders, and rig binds of the copied layer.
        let uid = next.layers[layer].layer_uid().to_owned();
        if let Presence::Present(holders) = &mut next.property_holders {
            for holder in holders.iter_mut().filter(|holder| {
                holder.target.context_id() == ROOT && holder.target.layer_uid() == uid
            }) {
                remap_id(&mut holder.target.stroke_id, pairs);
            }
        }
        if let Presence::Present(rig) = &mut next.layers[layer].rig {
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
