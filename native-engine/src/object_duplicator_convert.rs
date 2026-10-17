//! Duplicator setup plans (L04-M, I10): `toggleLayerDuplicator` (`motion.js`),
//! `convertSelectionToObjectDuplicator` and the path-layer picker (`app.js`,
//! `timeline.js`) at fbe4b8a3, as detached candidate documents over the lane-4
//! duplicator table. Declared as a private child of `object_fill_owner.rs`.
//!
//! - enable: the default shape-mode descriptor on the layer (`rows 2, cols 3, ...`,
//!   the seed is the caller's `Math.floor(Math.random() * 1e6)`), the layer locked;
//! - disable: the descriptor removed and the layer unlocked (`ld.duplicator = null`);
//! - object duplicator: a fresh locked "Duplicator" host (`uniqueLayerName`) whose
//!   descriptor pools the selection in document order (`rows 1, cols N`,
//!   `sourceMode 'objects'`); layers that already host a duplicator are skipped, the
//!   pool must keep at least two layers and the pooled layers are only hidden;
//! - path layer: `d.pathLayerUid = value || null`, with the picker's exclusions made
//!   hard refusals (not the layer itself, not a duplicator, an existing layer), the
//!   same references `_resolveDuplicatorPath` answers `null` for.
//!
//! The toggle is expressed as a target state (`enabled`) so that asking for the state
//! a layer already has is a no-op: presence, bytes, history and redo stay untouched.
//! An unknown layer in an object-duplicator selection is refused instead of skipped.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    append_layer, empty_duplicator_set, layer_index, mint_layer_uid, require_v2, update,
    with_tables, SetupError, SetupPlan,
};
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    DuplicatorDescriptor, DuplicatorRecord, StaggerRandom, TimeOffset,
};
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::unique_layer_name;
use crate::object_document::{ObjectDocument, ObjectLayer, Presence};
use serde_json::Number;

/// Largest seed the baseline can draw: `Math.floor(Math.random() * 1e6)`.
pub(crate) const MAX_SEED: u32 = 999_999;

fn number(value: u32) -> Presence<Number> {
    Presence::Present(Number::from(value))
}
fn flag(value: bool) -> Presence<bool> {
    Presence::Present(value)
}

/// The descriptor literal both baseline entry points share; the object mode only
/// changes the grid size, the count and the source pool.
fn descriptor(rows: u32, cols: u32, count: u32, seed: u32) -> DuplicatorDescriptor {
    DuplicatorDescriptor {
        mode: Presence::Present("grid".to_string()),
        rows: number(rows),
        cols: number(cols),
        spacing_x: number(150),
        spacing_y: number(150),
        count: number(count),
        radius: number(200),
        start_angle: number(0),
        end_angle: Presence::Present(None),
        radial_orient: flag(false),
        path_layer_uid: Presence::Present(None),
        path_align_tangent: flag(true),
        seed: number(seed),
        stagger_random: Presence::Present(StaggerRandom {
            position: flag(false),
            rotation: flag(false),
            scale: flag(false),
            opacity: flag(false),
            hue: flag(false),
        }),
        time_offset: Presence::Present(TimeOffset {
            enabled: flag(false),
            offset_frames: number(1),
            direction: Presence::Present("forward".to_string()),
        }),
        source_mode: Presence::Missing,
        source_layer_uids: Presence::Missing,
        effectors: Presence::Missing,
    }
}

fn hosts_duplicator(document: &ObjectDocument, layer_uid: &str) -> bool {
    document
        .duplicator_set()
        .is_some_and(|set| set.duplicator(SCENE_CONTEXT, layer_uid).is_some())
}

fn checked_seed(seed: Option<u32>) -> Result<u32, SetupError> {
    seed.filter(|value| *value <= MAX_SEED)
        .ok_or(SetupError::SeedRange)
}

/// `toggleLayerDuplicator(li)` to the requested state.
pub(crate) fn plan_duplicator_set(
    document: &ObjectDocument,
    layer_uid: &str,
    enabled: bool,
    seed: Option<u32>,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    let index = layer_index(document, layer_uid)?;
    if hosts_duplicator(document, layer_uid) == enabled {
        return Ok(SetupPlan::unchanged(document, None));
    }
    let seed = if enabled { checked_seed(seed)? } else { 0 };
    let mut candidate = document.clone();
    candidate.layers[index].locked = Presence::Present(enabled);
    with_tables(&mut candidate, |tables| {
        update(&mut tables.duplicators, empty_duplicator_set, |set| {
            if enabled {
                set.duplicators.push(DuplicatorRecord {
                    context_id: SCENE_CONTEXT.to_string(),
                    layer_uid: layer_uid.to_string(),
                    duplicator: descriptor(2, 3, 8, seed),
                });
            } else {
                set.duplicators
                    .retain(|r| !(r.context_id == SCENE_CONTEXT && r.layer_uid == layer_uid));
            }
        });
        Ok(())
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: None,
        layer_uid: Some(layer_uid.to_string()),
        created: None,
    })
}

/// `convertSelectionToObjectDuplicator(indices)`.
pub(crate) fn plan_object_duplicator(
    document: &ObjectDocument,
    next_revision: u64,
    layer_uids: &[String],
    seed: Option<u32>,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    let seed = checked_seed(seed)?;
    let mut indices: Vec<usize> = Vec::new();
    for uid in layer_uids {
        let index = layer_index(document, uid)?;
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    indices.sort_unstable();
    let pool: Vec<usize> = indices
        .into_iter()
        .filter(|&index| !hosts_duplicator(document, &document.layers[index].layer_uid))
        .collect();
    if pool.len() < 2 {
        return Err(SetupError::TooFewLayers);
    }
    let uids: Vec<String> = pool
        .iter()
        .map(|&index| document.layers[index].layer_uid.clone())
        .collect();
    let width = u32::try_from(uids.len()).map_err(|_| SetupError::TooMany)?;
    let host_uid = mint_layer_uid(document, next_revision)?;
    let names: Vec<&str> = document
        .layers
        .iter()
        .filter_map(|layer| layer.name.value().map(String::as_str))
        .collect();
    let host_name = unique_layer_name("Duplicator", &names);
    let mut candidate = document.clone();
    for &index in &pool {
        candidate.layers[index].visible = Presence::Present(false);
    }
    let mut host = ObjectLayer::blank_drawing(host_uid.clone(), host_name, candidate.layers.len());
    host.locked = Presence::Present(true);
    append_layer(&mut candidate, host)?;
    let mut described = descriptor(1, width, width, seed);
    described.source_mode = Presence::Present("objects".to_string());
    described.source_layer_uids = Presence::Present(uids);
    with_tables(&mut candidate, |tables| {
        update(&mut tables.duplicators, empty_duplicator_set, |set| {
            set.duplicators.push(DuplicatorRecord {
                context_id: SCENE_CONTEXT.to_string(),
                layer_uid: host_uid.clone(),
                duplicator: described,
            });
        });
        Ok(())
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: None,
        layer_uid: Some(host_uid.clone()),
        created: Some(host_uid),
    })
}

/// The path-layer `<select>`: `d.pathLayerUid = this.value || null`.
pub(crate) fn plan_duplicator_path(
    document: &ObjectDocument,
    layer_uid: &str,
    path_layer_uid: Option<&str>,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    layer_index(document, layer_uid)?;
    let record = document
        .duplicator_set()
        .and_then(|set| set.duplicator(SCENE_CONTEXT, layer_uid))
        .ok_or_else(|| SetupError::NotADuplicator(layer_uid.to_string()))?;
    let path = path_layer_uid.filter(|value| !value.is_empty());
    if let Some(path) = path {
        if path == layer_uid {
            return Err(SetupError::PathSelf(path.to_string()));
        }
        if hosts_duplicator(document, path) {
            return Err(SetupError::PathChained(path.to_string()));
        }
        layer_index(document, path).map_err(|_| SetupError::PathUnknown(path.to_string()))?;
    }
    let wanted: Presence<Option<String>> = Presence::Present(path.map(str::to_owned));
    if record.duplicator.path_layer_uid == wanted {
        return Ok(SetupPlan::unchanged(document, None));
    }
    let mut candidate = document.clone();
    with_tables(&mut candidate, |tables| {
        update(
            &mut tables.duplicators,
            empty_duplicator_set,
            |set| match set
                .duplicators
                .iter_mut()
                .find(|r| r.context_id == SCENE_CONTEXT && r.layer_uid == layer_uid)
            {
                Some(found) => {
                    found.duplicator.path_layer_uid = wanted;
                    Ok(())
                }
                None => Err(SetupError::NotADuplicator(layer_uid.to_string())),
            },
        )
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: None,
        layer_uid: Some(layer_uid.to_string()),
        created: None,
    })
}

#[cfg(test)]
#[path = "../tests/object_component_convert_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_duplicator_convert.rs"]
mod tests;
