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
//! images and text inside the component, Motion on its inner layers (opacity tracks are applied at paint time, not baked),
//! inner groups and rigs, and stroke ids that collide once the inner layers share one layer.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    layer_index, require_v2, with_tables, SetupError, SetupPlan,
};
use crate::commands::{insert_order_element, OrderEditError};
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::{
    FrameScope, FrameScopeKind, LayerTiming, ObjectDocument, ObjectRecord, Presence,
};
use std::collections::HashSet;

/// The content of the instance on one key frame of the scene: `(frame, records)`, the records in inner stack order, geometry
/// already placed (component camera, then `symMatrix`), their targets still naming the component source.
pub(crate) type BakedFrame = (u32, Vec<ObjectRecord>);

fn order_error(_: OrderEditError) -> SetupError {
    SetupError::BreakApartUnavailable(
        "component-break-apart: the layer order could not take the baked shapes.",
    )
}

/// What the component holds that the bake would drop.
fn unbakeable(document: &ObjectDocument, symbol_id: &str) -> Option<&'static str> {
    let component = document.component_set()?.component(symbol_id)?;
    if document.item_records().is_some_and(|items| {
        items
            .targets()
            .any(|target| target.context_id() == symbol_id)
    }) {
        return Some("component-break-apart: images and text inside the component cannot be baked into the layer yet.");
    }
    if document
        .property_holders()
        .is_some_and(|holders| holders.iter().any(|h| h.target().context_id() == symbol_id))
    {
        return Some(
            "component-break-apart: Motion on the inner layers cannot be baked into the layer yet.",
        );
    }
    if component
        .layers()
        .iter()
        .any(|layer| layer.groups.value().is_some() || layer.rig.value().is_some())
    {
        return Some(
            "component-break-apart: inner groups and rigs cannot be baked into the layer yet.",
        );
    }
    None
}

/// The inner records of `baked`, retargeted onto the host layer at `frame`.
fn retarget(record: &ObjectRecord, layer_uid: &str, frame: u32) -> ObjectRecord {
    let mut moved = record.clone();
    moved.target.context_id = SCENE_CONTEXT.to_string();
    moved.target.layer_uid = layer_uid.to_string();
    moved.target.frame_scope = FrameScope {
        kind: FrameScopeKind::Authored,
        frame,
    };
    moved
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
    for (frame, records) in baked {
        let mut seen = HashSet::new();
        for record in records {
            if !seen.insert(record.target.stroke_id.clone()) {
                return Err(SetupError::BreakApartUnavailable(
                    "component-break-apart: two inner layers share a stroke id, which one layer cannot hold.",
                ));
            }
            let moved = retarget(record, layer_uid, *frame);
            let target = moved.target.clone();
            candidate.objects.push(moved);
            if let Presence::Present(plan) = &mut candidate.composition_order {
                insert_order_element(plan, &target).map_err(order_error)?;
            }
        }
        keys.push(*frame);
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
