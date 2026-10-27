//! Component convert plans (L04-M, I14/I35): `convertLayerToComponent`,
//! `convertLayersToComponent` and `badComponentSourceReason` of `src/js/app.js` at
//! fbe4b8a3 (place and expose are in `object_component_place.rs`), as detached candidate
//! documents over the existing `ObjectDocument` and its lane-4 tables. Declared as
//! a private child of `object_fill_owner.rs`.
//!
//! Conversion is explicit only: nothing here is reachable from a Motion edit, so
//! keying Position on a flat layer never creates a component (CLAUDE.md section 8).
//! Mapping of the baseline to the native model:
//!
//! - one layer: the layer stays and becomes the instance (`ld.symbolId`, locked,
//!   `once`, speed 1, placed 0, single frame 0); its drawing moves to a fresh inner
//!   layer "Layer 1" with the same stroke ids, timing and group table; its own
//!   layer-level Motion holder and its time range stay on the instance (the instance
//!   keeps its motion and its window); its rig moves inside with the strokes it binds
//!   (`cloneRigForSymbol` + `delete ld.rig`), so the instance is left with none;
//! - several layers: each source becomes an inner layer (same layerUid, unlocked,
//!   groups, path effects and rig carried) in index order, the block is replaced by ONE
//!   new locked "Composant" instance at the position of the first source, and the
//!   first source's duplicator descriptor and time window (in point, out point, time
//!   link; its markers stay inside) move to it (`firstSrc.duplicator`, `lifted`);
//! - the component is named `ld.name + ' (Comp)'` (one layer) or `'Composant'`;
//! - a StoryBoard instance module is added (`SMStoryboard.addInstanceAuto`).
//!
//! Deliberate differences, each pinned by a control: group ids are not remapped
//! because every native group table is per layer and context (the baseline remap
//! only avoids collisions in the flattened outer registry); the outer instance keeps
//! no copy of the groups (that copy was a rendering detail of the flattened
//! registry); an unknown layer is refused instead of silently skipped. Motion holders
//! follow their stable identities through the existing native holder table; the first
//! merged layer's expression/control chain belongs to the outer instance exactly once.
//! A layer's plain images move into the component with their stack position
//! (`object_component_convert_items.rs`). A layer holding text, brush rasters or mesh-bound
//! images is refused (`Unavailable`, `component-convert-items`), never left behind on an
//! instance that no longer owns the strokes.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    add_instance_module, append_layer, empty_component_set, layer_index, lift_window,
    mint_layer_uid, mint_symbol_id, new_instance, require_v2, update, with_tables, PermitAll,
    SetupError, SetupPlan, SourceRefusal,
};
use crate::commands::remove_order_element;
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentRecord, SCENE_CONTEXT,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::DuplicatorRecord;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    plan_delete, plan_reorder, LayerSelection,
};
use crate::object_document::{
    FrameScopeKind, GroupTable, LayerTiming, ObjectDocument, ObjectLayer, ObjectRecord,
    ObjectTarget, Presence,
};

#[path = "object_component_convert_holders.rs"]
mod holders;
#[path = "object_component_convert_items.rs"]
mod items;

/// What a layer is, as far as the baseline refusal list needs it. The native
/// document stores a record for three of the eight kinds (a component instance in
/// the tables, an effect layer and a widget on the layer); the other five have no
/// native record yet, so `facts_from_document` reports them false instead of guessing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SourceFacts {
    pub(crate) component: bool,
    pub(crate) lfs_group: bool,
    pub(crate) montage: bool,
    pub(crate) native_video: bool,
    pub(crate) null_layer: bool,
    pub(crate) effect_layer: bool,
    pub(crate) guide_layer: bool,
    pub(crate) widget_layer: bool,
}

/// `badComponentSourceReason(l)`: the first matching kind in the baseline order.
/// Effector layers, folders and duplicator hosts are not in the list and are not refused.
pub(crate) fn bad_component_source_reason(facts: &SourceFacts) -> Option<SourceRefusal> {
    let order = [
        (facts.component, SourceRefusal::Component),
        (facts.lfs_group, SourceRefusal::LfsGroup),
        (facts.montage, SourceRefusal::Montage),
        (facts.native_video, SourceRefusal::NativeVideo),
        (facts.null_layer, SourceRefusal::Null),
        (facts.effect_layer, SourceRefusal::Effect),
        (facts.guide_layer, SourceRefusal::Guide),
        (facts.widget_layer, SourceRefusal::Widget),
    ];
    order
        .iter()
        .find(|(present, _)| *present)
        .map(|(_, refusal)| *refusal)
}

pub(crate) fn facts_from_document(document: &ObjectDocument, layer: &ObjectLayer) -> SourceFacts {
    SourceFacts {
        component: document
            .component_set()
            .is_some_and(|set| set.instance(SCENE_CONTEXT, layer.layer_uid()).is_some()),
        effect_layer: layer.is_effect_layer(),
        widget_layer: layer.widget.value().is_some(),
        ..SourceFacts::default()
    }
}

/// The authored-frame records of a scene layer, re-targeted into a component
/// context. Stroke ids and frames are kept.
fn retarget_objects(
    document: &ObjectDocument,
    layer_uid: &str,
    symbol_id: &str,
    inner_uid: &str,
) -> Result<Vec<ObjectRecord>, SetupError> {
    let mut moved = Vec::new();
    for record in document
        .objects
        .iter()
        .filter(|r| is_scene_record(r, layer_uid))
    {
        if record.target.frame_scope.kind != FrameScopeKind::Authored {
            return Err(SetupError::ObjectScope {
                layer_uid: layer_uid.to_string(),
            });
        }
        let mut record = record.clone();
        record.target.context_id = symbol_id.to_string();
        record.target.layer_uid = inner_uid.to_string();
        moved.push(record);
    }
    Ok(moved)
}

fn is_scene_record(record: &ObjectRecord, layer_uid: &str) -> bool {
    record.target.layer_uid == layer_uid && record.target.context_id == SCENE_CONTEXT
}

fn retarget_groups(groups: &Presence<GroupTable>, symbol_id: &str) -> Presence<GroupTable> {
    match groups {
        Presence::Present(table) => {
            let mut table = table.clone();
            table.context_id = symbol_id.to_string();
            Presence::Present(table)
        }
        Presence::Missing => Presence::Missing,
    }
}

/// `convertLayerToComponent` / `convertLayersToComponent`: one uid converts in
/// place, several merge into one new instance. `next_revision` seeds minted ids.
pub(crate) fn plan_convert(
    document: &ObjectDocument,
    next_revision: u64,
    layer_uids: &[String],
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    let mut indices: Vec<usize> = Vec::new();
    for uid in layer_uids {
        let index = layer_index(document, uid)?;
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    indices.sort_unstable();
    if indices.is_empty() {
        return Err(SetupError::NoLayers);
    }
    for &index in &indices {
        let layer = &document.layers[index];
        if let Some(refusal) = bad_component_source_reason(&facts_from_document(document, layer)) {
            return Err(SetupError::BadSource {
                layer_uid: layer.layer_uid.clone(),
                refusal,
            });
        }
    }
    for &index in &indices {
        let layer_uid = document.layers[index].layer_uid();
        if items::refused_on(document, layer_uid) {
            return Err(SetupError::ItemsUnavailable {
                layer_uid: layer_uid.to_string(),
            });
        }
    }
    if indices.len() == 1 {
        convert_single(document, next_revision, indices[0])
    } else {
        convert_merge(document, next_revision, &indices)
    }
}

fn convert_single(
    document: &ObjectDocument,
    next_revision: u64,
    index: usize,
) -> Result<SetupPlan, SetupError> {
    let source = &document.layers[index];
    let uid = source.layer_uid.clone();
    let name = source
        .name
        .value()
        .ok_or_else(|| SetupError::Unnamed(uid.clone()))?;
    let symbol_id = mint_symbol_id(document, next_revision)?;
    let inner_uid = format!("{symbol_id}:layer-1");
    let moved = retarget_objects(document, &uid, &symbol_id, &inner_uid)?;
    let mut inner = ObjectLayer::identified(inner_uid.clone());
    inner.name = Presence::Present("Layer 1".to_string());
    inner.visible = Presence::Present(true);
    inner.locked = Presence::Present(false);
    inner.timing = source.timing.clone();
    inner.groups = retarget_groups(&source.groups, &symbol_id);
    // `rig: cloneRigForSymbol(ld.rig)` then `delete ld.rig`: the binds name strokes that now live inside.
    inner.rig = source.rig.clone();
    let record = ComponentRecord {
        symbol_id: symbol_id.clone(),
        name: Presence::Present(format!("{name} (Comp)")),
        total_frames: document.total_frames,
        fps: Presence::Missing,
        layers: vec![inner],
        objects: moved,
        exposed_props: Presence::Missing,
    };
    let mut candidate = document.clone();
    let order = items::move_images(
        &mut candidate,
        document,
        &symbol_id,
        &[(uid.clone(), inner_uid.clone())],
    )?;
    items::declare(&mut candidate, order);
    candidate.property_holders = holders::single(document, &uid, &symbol_id, &inner_uid);
    drop_scene_objects(&mut candidate, &uid)?;
    let instance = &mut candidate.layers[index];
    instance.timing = Presence::Present(LayerTiming::blank_drawing());
    instance.groups = Presence::Missing;
    instance.rig = Presence::Missing;
    instance.locked = Presence::Present(true);
    with_tables(&mut candidate, |tables| {
        update(&mut tables.components, empty_component_set, |set| {
            set.components.push(record);
            set.instances.push(new_instance(&uid, &symbol_id));
        });
        add_instance_module(tables, &symbol_id)
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: Some(symbol_id),
        layer_uid: Some(uid),
        created: None,
    })
}

/// Remove a scene layer's records and their order elements (the records now live
/// in the component).
fn drop_scene_objects(candidate: &mut ObjectDocument, layer_uid: &str) -> Result<(), SetupError> {
    let targets: Vec<ObjectTarget> = candidate
        .objects
        .iter()
        .filter(|record| is_scene_record(record, layer_uid))
        .map(|record| record.target.clone())
        .collect();
    if let Presence::Present(plan) = &mut candidate.composition_order {
        for target in &targets {
            remove_order_element(plan, target).map_err(SetupError::Order)?;
        }
    }
    candidate
        .objects
        .retain(|record| !is_scene_record(record, layer_uid));
    Ok(())
}

fn convert_merge(
    document: &ObjectDocument,
    next_revision: u64,
    indices: &[usize],
) -> Result<SetupPlan, SetupError> {
    let symbol_id = mint_symbol_id(document, next_revision)?;
    let instance_uid = mint_layer_uid(document, next_revision)?;
    let mut inner_layers: Vec<ObjectLayer> = Vec::new();
    let mut moved: Vec<ObjectRecord> = Vec::new();
    let mut window = Presence::Missing;
    for &index in indices {
        let source = &document.layers[index];
        moved.extend(retarget_objects(
            document,
            &source.layer_uid,
            &symbol_id,
            &source.layer_uid,
        )?);
        let mut inner = source.clone();
        inner.locked = Presence::Present(false);
        inner.folder_id = Presence::Missing;
        inner.groups = retarget_groups(&source.groups, &symbol_id);
        if inner_layers.is_empty() {
            window = lift_window(&mut inner);
        }
        inner_layers.push(inner);
    }
    let mut instance = ObjectLayer::identified(instance_uid.clone());
    instance.range = window;
    instance.name = Presence::Present("Composant".to_string());
    instance.visible = Presence::Present(true);
    instance.locked = Presence::Present(true);
    instance.timing = Presence::Present(LayerTiming::blank_drawing());
    let gone: Vec<String> = indices
        .iter()
        .map(|&index| document.layers[index].layer_uid.clone())
        .collect();
    let record = ComponentRecord {
        symbol_id: symbol_id.clone(),
        name: Presence::Present("Composant".to_string()),
        total_frames: document.total_frames,
        fps: Presence::Missing,
        layers: inner_layers,
        objects: moved,
        exposed_props: Presence::Missing,
    };
    let lifted = document
        .duplicator_set()
        .and_then(|set| set.duplicator(SCENE_CONTEXT, &gone[0]))
        .map(|found| found.duplicator.clone());
    let mut candidate = document.clone();
    let pairs: Vec<_> = gone.iter().map(|uid| (uid.clone(), uid.clone())).collect();
    let order = items::move_images(&mut candidate, document, &symbol_id, &pairs)?;
    append_layer(&mut candidate, instance)?;
    with_tables(&mut candidate, |tables| {
        update(&mut tables.components, empty_component_set, |set| {
            set.components.push(record);
            set.instances.push(new_instance(&instance_uid, &symbol_id));
        });
        add_instance_module(tables, &symbol_id)?;
        if let Presence::Present(set) = &mut tables.duplicators {
            set.duplicators
                .retain(|r| !(r.context_id == SCENE_CONTEXT && gone.contains(&r.layer_uid)));
            set.effector_layers
                .retain(|r| !(r.context_id == SCENE_CONTEXT && gone.contains(&r.layer_uid)));
            if let Some(descriptor) = lifted {
                set.duplicators.push(DuplicatorRecord {
                    context_id: SCENE_CONTEXT.to_string(),
                    layer_uid: instance_uid.clone(),
                    duplicator: descriptor,
                });
            }
        }
        Ok(())
    })?;
    let selection = LayerSelection {
        active: instance_uid.clone(),
        selected: vec![instance_uid.clone()],
        anchor: None,
    };
    let top = i64::try_from(candidate.layers.len() - 1).map_err(|_| SetupError::TooMany)?;
    let placed = plan_reorder(
        &candidate,
        &[top],
        indices[0] as f64,
        &selection,
        &PermitAll,
    )
    .map_err(SetupError::Lifecycle)?;
    let selection = LayerSelection {
        active: gone[0].clone(),
        selected: gone.clone(),
        anchor: None,
    };
    let mut removed =
        plan_delete(&placed.document, &selection, &PermitAll).map_err(SetupError::Lifecycle)?;
    // Root-layer deletion cannot identify a same-UID holder in another context.
    // Restore the complete scoped transform of the retained origin after deletion.
    removed.document.property_holders = holders::merged(document, &gone, &symbol_id, &instance_uid);
    items::declare(&mut removed.document, order);
    Ok(SetupPlan {
        document: removed.document,
        changed: true,
        symbol_id: Some(symbol_id),
        layer_uid: Some(instance_uid.clone()),
        created: Some(instance_uid),
    })
}

#[cfg(test)]
#[path = "../tests/object_component_convert_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_component_convert_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_component_convert.rs"]
mod tests;
#[cfg(test)]
#[path = "../tests/object_component_convert_verify.rs"]
mod verify;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_component_convert_motion.rs"]
mod motion_tests;
