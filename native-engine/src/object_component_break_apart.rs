//! Break a component apart (`convertComponentToLayer`, `src/js/app.js` at 1065e9d8): the inverse of the convert.
//!
//! The baseline evaluates `getEffectiveStrokes(layerIdx, fi)` for every frame of the scene, collapses identical
//! consecutive frames into one keyframe plus holds, deletes `symbolId` and the `sym*` playback fields and unlocks the layer;
//! the symbol itself stays in the library for any other instance. Here the per-frame evaluation is the one component join
//! (playback, held keys, exposed overrides, component camera, `symMatrix`; the evaluation half is
//! `object_component_break_apart_bake.rs`) and this plan is the pure document half: the baked records land on the instance
//! layer as ordinary authored-frame records, the layer takes a timing keyed where the content changes, and the instance
//! record goes. The layer keeps its own Motion, range, duplicator and every other layer field, as `ld` does. One plan is
//! one candidate document, so one request is one revision and one history entry.
//!
//! What a flat layer cannot hold yet is refused whole, typed, before anything changes (stable code `component-break-apart`):
//! text blocks inside the component, brush images and image meshes, Motion on the inner layers other than the layer opacity
//! (the one the viewport applies; it is folded into the baked opacity), and stroke ids that collide once the inner layers
//! share one layer. Inner groups and rigs are not content of the baked frames: the baseline flattens the strokes only, and so
//! does this (the component keeps its own groups and rig for every other instance).
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    layer_index, require_v2, with_tables, SetupError, SetupPlan,
};
use crate::commands::{insert_order_element, OrderEditError};
use crate::object_document::object_component_record_kernels::object_component_records::{Baked, SCENE_CONTEXT};
use crate::object_document::{
    FrameScope, FrameScopeKind, ItemRecords, LayerTiming, ObjectDocument, ObjectTarget, Presence,
};
use std::collections::HashSet;

/// The content of the instance on one key frame of the scene: `(frame, items)`, the items in inner paint order, geometry
/// already placed (component camera, then `symMatrix`), their targets still naming the component source.
pub(crate) type BakedFrame = (u32, Vec<Baked>);

fn order_error(_: OrderEditError) -> SetupError {
    SetupError::BreakApartUnavailable(
        "component-break-apart: the layer order could not take the baked shapes.",
    )
}

/// What the component holds that the bake would drop.
fn unbakeable(document: &ObjectDocument, symbol_id: &str) -> Option<&'static str> {
    let items = document.item_records()?;
    if items
        .text_entries()
        .iter()
        .any(|entry| entry.target.context_id() == symbol_id)
    {
        return Some("component-break-apart: text blocks inside the component cannot be baked into the layer yet.");
    }
    if items.raster_entries().iter().any(|entry| {
        entry.target.context_id() == symbol_id
            && (entry.item.is_bitmap_brush() == Some(true)
                || entry.item.brush_group_id().is_some()
                || entry.item.is_brush_texture_copy() == Some(true)
                || entry.item.mesh_id().is_some())
    }) {
        return Some("component-break-apart: brush images and image meshes inside the component cannot be baked into the layer yet.");
    }
    None
}

/// The baked item on the host layer at `frame`.
fn retarget(item: &Baked, layer_uid: &str, frame: u32) -> Baked {
    let place = |target: &mut ObjectTarget| {
        target.context_id = SCENE_CONTEXT.to_string();
        target.layer_uid = layer_uid.to_string();
        target.frame_scope = FrameScope {
            kind: FrameScopeKind::Authored,
            frame,
        };
    };
    let mut moved = item.clone();
    match &mut moved {
        Baked::Vector(record) => place(&mut record.target),
        Baked::Image(entry) => place(&mut entry.target),
    }
    moved
}

fn target_of(item: &Baked) -> &ObjectTarget {
    match item {
        Baked::Vector(record) => &record.target,
        Baked::Image(entry) => &entry.target,
    }
}

/// The refusals that need no evaluation: the layer is an instance of a component whose content a flat layer can hold.
/// Returns the layer's index and the component's symbol id.
pub(crate) fn precheck(
    document: &ObjectDocument,
    layer_uid: &str,
) -> Result<(usize, String), SetupError> {
    require_v2(document)?;
    let index = layer_index(document, layer_uid)?;
    let symbol_id = document
        .component_set()
        .and_then(|set| set.instance(SCENE_CONTEXT, layer_uid))
        .map(|instance| instance.symbol_id().to_owned())
        .ok_or_else(|| SetupError::NotAnInstance(layer_uid.to_string()))?;
    match unbakeable(document, &symbol_id) {
        Some(why) => Err(SetupError::BreakApartUnavailable(why)),
        None => Ok((index, symbol_id)),
    }
}

/// `convertComponentToLayer(layerIdx)` over the frames the evaluation produced (`baked`: ascending key frames, the first at 0).
pub(crate) fn plan_break_apart(
    document: &ObjectDocument,
    layer_uid: &str,
    baked: &[BakedFrame],
) -> Result<SetupPlan, SetupError> {
    let (index, symbol_id) = precheck(document, layer_uid)?;
    let mut candidate = document.clone();
    let mut keys = Vec::with_capacity(baked.len());
    let (mut rasters, texts) = document
        .item_records()
        .map(|items| (items.raster_entries().to_vec(), items.text_entries().to_vec()))
        .unwrap_or_default();
    let images_before = rasters.len();
    for (frame, items) in baked {
        let mut seen = HashSet::new();
        for item in items {
            if !seen.insert(target_of(item).stroke_id.clone()) {
                return Err(SetupError::BreakApartUnavailable(
                    "component-break-apart: two inner layers share a stroke id, which one layer cannot hold.",
                ));
            }
            let moved = retarget(item, layer_uid, *frame);
            let target = target_of(&moved).clone();
            match moved {
                Baked::Vector(record) => candidate.objects.push(record),
                Baked::Image(entry) => rasters.push(entry),
            }
            if let Presence::Present(plan) = &mut candidate.composition_order {
                insert_order_element(plan, &target).map_err(order_error)?;
            }
        }
        keys.push(*frame);
    }
    if rasters.len() != images_before {
        candidate.items = ItemRecords::normalized(rasters, texts);
    }
    let layer = &mut candidate.layers[index];
    layer.timing = Presence::Present(LayerTiming::from_key_frames(&keys));
    layer.locked = Presence::Present(false);
    with_tables(&mut candidate, |tables| {
        if let Presence::Present(set) = &mut tables.components {
            set.instances
                .retain(|i| !(i.context_id == SCENE_CONTEXT && i.layer_uid == layer_uid));
        }
        Ok(())
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: Some(symbol_id),
        layer_uid: Some(layer_uid.to_string()),
        created: None,
    })
}
