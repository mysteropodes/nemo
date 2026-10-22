//! Private command-side binding projection of the one retained document.
//! Resolve native mixed slots and full targets here; the parent owns strict request parsing,
//! detached transaction preparation and writeback. Only narrow member projections cross the
//! read-queries port; no document, snapshot or owner crosses that boundary.
use crate::object_document::{
    FrameScopeKind, ImageMeshStore, MeshIdSequence, ObjectDocument, ObjectLayer, ObjectRecord,
    ObjectTarget,
};
use crate::object_snapshot::object_image_mesh_edit_read::{
    BindingLayer, BindingSlot, BindingState, BindingTarget,
};
use crate::request_receipts::DispatchErrorCode;
use std::collections::{BTreeMap, HashMap, HashSet};

const ROOT: &str = "scene-root";

struct Context<'a> {
    id: &'a str,
    layers: &'a [ObjectLayer],
    objects: &'a [ObjectRecord],
    frames: u32,
}

pub(super) struct Location {
    pub(super) source_index: usize,
    pub(super) target: ObjectTarget,
    pub(super) at: BindingTarget,
}

pub(super) struct Projection {
    pub(super) state: BindingState,
    pub(super) locations: Vec<Location>,
    pub(super) missing: Vec<Vec<usize>>,
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
pub(super) fn project(
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
    let order_contexts: HashMap<_, _> = document
        .composition_order
        .value()
        .into_iter()
        .flat_map(|plan| &plan.contexts)
        .map(|context| (context.context_id.as_str(), context))
        .collect();
    let mut context_items: HashMap<_, Vec<_>> = HashMap::new();
    for (target, raster) in rasters
        .iter()
        .enumerate()
        .map(|(index, entry)| (&entry.target, Some(index)))
        .chain(texts.iter().map(|entry| (&entry.target, None)))
    {
        context_items
            .entry(target.context_id())
            .or_default()
            .push((target, raster));
    }
    for (set_index, context) in contexts.iter().enumerate() {
        // Index each authored scope once. A raster lookup uses its full layer/stroke key;
        // animated paint order remains irrelevant to native positional propagation.
        let scopes: HashMap<_, HashMap<_, _>> = order_contexts
            .get(context.id)
            .into_iter()
            .flat_map(|plan| &plan.scopes)
            .filter(|scope| scope.frame_scope.kind == FrameScopeKind::Authored)
            .map(|scope| {
                (
                    scope.frame_scope.frame,
                    scope
                        .elements
                        .iter()
                        .map(|element| {
                            (
                                (element.layer_uid.as_str(), element.stroke_id.as_str()),
                                element.natural_position,
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        // Group once per context while preserving the original path/raster/text encounter order
        // within each layer. Frames stay sparse and sorted exactly as before.
        let mut layer_frames: HashMap<_, BTreeMap<_, Vec<_>>> = HashMap::new();
        let targets = context
            .objects
            .iter()
            .map(|object| (object.target(), None))
            .chain(context_items.remove(context.id).unwrap_or_default());
        for (target, raster) in targets.filter(|(target, _)| {
            target.context_id() == context.id
                && target.scope_kind() == &FrameScopeKind::Authored
                && target.frame() < context.frames
        }) {
            layer_frames
                .entry(target.layer_uid())
                .or_default()
                .entry(target.frame())
                .or_default()
                .push((target, raster));
        }
        let mut layers = vec![];
        let mut missing = vec![];
        for (layer_index, layer) in context.layers.iter().enumerate() {
            // A target plus optional actual raster; nonrasters occupy their real mixed positions.
            let frames = layer_frames.remove(layer.layer_uid()).unwrap_or_default();
            let mut projected_frames = vec![];
            for (frame_number, mut slots) in frames {
                let scope = scopes.get(&frame_number);
                if let Some(scope) = scope {
                    let mut positioned = vec![];
                    for (target, raster) in slots {
                        let position = scope
                            .get(&(target.layer_uid(), target.stroke_id()))
                            .ok_or(DispatchErrorCode::Unavailable)?;
                        positioned.push((*position, target, raster));
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
                    // No cross-table order exists for this context. Never guess a mixed slot
                    // or project only its paths while silently omitting real item positions.
                    return Err(DispatchErrorCode::Unavailable);
                }
                let frame_index = projected_frames.len();
                let mut bound = vec![];
                for (slot_index, (target, raster)) in slots.into_iter().enumerate() {
                    match raster {
                        Some(index) => {
                            projected.insert(index);
                            result.locations.push(Location {
                                source_index: index,
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

pub(super) fn layer_target(
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
    if context.layers[layer].locked()
        || document.duplicator_set().is_some_and(|set| {
            set.duplicators
                .iter()
                .any(|record| record.context_id == context_id && record.layer_uid == layer_uid)
        })
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    if context.layers[layer].widget.value().is_some() || context.layers[layer].is_null_layer() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok((set, layer))
}

pub(super) fn slot_target(
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
    let (set, _) = layer_target(document, target.context_id(), target.layer_uid())?;
    let all = contexts(document);
    if target.frame() >= all[set].frames {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    projection
        .locations
        .iter()
        .find(|location| &location.target == target)
        .map(|location| location.at)
        .ok_or(DispatchErrorCode::NotFound)
}
