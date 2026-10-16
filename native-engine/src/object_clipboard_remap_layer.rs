//! Duplicate-layer reference remap (`dupOne` of `src/js/timeline.js`, L02-C).
//!
//! A layer duplicate keeps every per-layer id (stroke ids, groups, rig target
//! stroke ids, matte and parent uids, a component's `symbolId`) and replaces
//! exactly one family: image-mesh ids, which index a PROJECT-level store and
//! would otherwise leave two layers driving one mesh. The mesh map is applied
//! to every frame stroke, the video layer's mesh id, the element-motion
//! holder keys and the rig binds. Mesh, rig and component references are
//! refused as `UnadmittedKind` until their records are native.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_clipboard_remap::{
    live, lookup, IdIssuer, IdKind, IdSource, ReferenceKind, RemapError,
};

/// Which not-yet-native reference kinds the caller can honour. `default()`
/// admits none: any mesh, rig or component reference refuses the batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct AdmittedKinds {
    pub(crate) mesh: bool,
    pub(crate) rig: bool,
    pub(crate) component: bool,
}

/// The reference-bearing fields of one source layer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LayerSource {
    /// `meshId` of every stroke dict, per frame, in frame then stroke order.
    pub(crate) frame_mesh_ids: Vec<Vec<Option<String>>>,
    /// `videoMeshId` (#779).
    pub(crate) video_mesh_id: Option<String>,
    /// Keys of `elementMotion` holding a record, in key order.
    pub(crate) element_motion_keys: Vec<String>,
    /// `rig.binds[].meshId`; `Some` means the layer has a rig.
    pub(crate) rig_bind_mesh_ids: Option<Vec<Option<String>>>,
    pub(crate) symbol_id: Option<String>,
    pub(crate) duplicator: bool,
    pub(crate) lfs_group: bool,
    pub(crate) locked: bool,
    pub(crate) matte_source_uid: Option<String>,
    pub(crate) mattes_more_uids: Vec<String>,
    /// `parentLayerUid`, `parentLayerUidB` and `parentsMore[].uid`.
    pub(crate) parent_uids: Vec<String>,
    /// Names of reference-bearing fields this kernel has no rule for.
    pub(crate) unknown_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerPlan {
    /// `(old, new)` in first-encounter order (frames, then video).
    pub(crate) mesh_map: Vec<(String, String)>,
    pub(crate) frame_mesh_ids: Vec<Vec<Option<String>>>,
    pub(crate) video_mesh_id: Option<String>,
    /// Unmoved keys keep their order; moved keys follow in map order.
    pub(crate) element_motion_keys: Vec<String>,
    pub(crate) rig_bind_mesh_ids: Option<Vec<Option<String>>>,
    /// The duplicate's `locked` flag.
    pub(crate) locked: bool,
    pub(crate) symbol_id: Option<String>,
    pub(crate) matte_source_uid: Option<String>,
    pub(crate) mattes_more_uids: Vec<String>,
    pub(crate) parent_uids: Vec<String>,
}

fn mesh_for<S: IdSource + ?Sized>(
    issuer: &mut IdIssuer<'_, S>,
    map: &mut Vec<(String, String)>,
    old: &str,
) -> Result<String, RemapError> {
    if let Some(existing) = lookup(map, old) {
        return Ok(existing.to_string());
    }
    let fresh = issuer.issue(IdKind::Mesh, Some(old))?;
    map.push((old.to_string(), fresh.clone()));
    Ok(fresh)
}

fn mesh_ids(src: &LayerSource) -> Vec<&str> {
    let mut found: Vec<&str> = Vec::new();
    for frame in &src.frame_mesh_ids {
        found.extend(frame.iter().filter_map(live));
    }
    found.extend(live(&src.video_mesh_id));
    if let Some(binds) = &src.rig_bind_mesh_ids {
        found.extend(binds.iter().filter_map(live));
    }
    found
}

/// The `locked` flag of the duplicate, in the baseline's assignment order:
/// the later `lfsGroup` write wins over the `symbolId` copy of `src.locked`,
/// which wins over the duplicator force; any other layer comes out unlocked.
fn duplicate_locked(src: &LayerSource) -> bool {
    if src.lfs_group {
        true
    } else if src.symbol_id.is_some() {
        src.locked
    } else {
        src.duplicator
    }
}

pub(crate) fn plan_layer_duplicate<S: IdSource + ?Sized>(
    src: &LayerSource,
    admitted: AdmittedKinds,
    ids: &mut S,
) -> Result<LayerPlan, RemapError> {
    if let Some(name) = src.unknown_references.first() {
        return Err(RemapError::UnknownReferenceKind(name.clone()));
    }
    if src.symbol_id.is_some() && !admitted.component {
        return Err(RemapError::UnadmittedKind(ReferenceKind::Component));
    }
    if src.rig_bind_mesh_ids.is_some() && !admitted.rig {
        return Err(RemapError::UnadmittedKind(ReferenceKind::Rig));
    }
    let used = mesh_ids(src);
    if !used.is_empty() && !admitted.mesh {
        return Err(RemapError::UnadmittedKind(ReferenceKind::Mesh));
    }
    let mut taken: HashSet<String> = used.iter().map(|id| id.to_string()).collect();
    taken.extend(src.element_motion_keys.iter().cloned());
    let mut issuer = IdIssuer::new(ids, taken);
    let mut mesh_map: Vec<(String, String)> = Vec::new();
    let mut frames: Vec<Vec<Option<String>>> = Vec::new();
    for frame in &src.frame_mesh_ids {
        let mut out: Vec<Option<String>> = Vec::new();
        for id in frame {
            out.push(match live(id) {
                Some(old) => Some(mesh_for(&mut issuer, &mut mesh_map, old)?),
                None => id.clone(),
            });
        }
        frames.push(out);
    }
    let video_mesh_id = match live(&src.video_mesh_id) {
        Some(old) => Some(mesh_for(&mut issuer, &mut mesh_map, old)?),
        None => src.video_mesh_id.clone(),
    };
    let mut keys: Vec<String> = src
        .element_motion_keys
        .iter()
        .filter(|key| lookup(&mesh_map, key.as_str()).is_none())
        .cloned()
        .collect();
    for (old, new) in &mesh_map {
        if src.element_motion_keys.iter().any(|key| key == old) {
            keys.push(new.clone());
        }
    }
    let rig_bind_mesh_ids = src.rig_bind_mesh_ids.as_ref().map(|binds| {
        binds
            .iter()
            .map(
                |bind| match live(bind).and_then(|old| lookup(&mesh_map, old)) {
                    Some(new) => Some(new.to_string()),
                    None => bind.clone(),
                },
            )
            .collect::<Vec<Option<String>>>()
    });
    Ok(LayerPlan {
        mesh_map,
        frame_mesh_ids: frames,
        video_mesh_id,
        element_motion_keys: keys,
        rig_bind_mesh_ids,
        locked: duplicate_locked(src),
        symbol_id: src.symbol_id.clone(),
        matte_source_uid: src.matte_source_uid.clone(),
        mattes_more_uids: src.mattes_more_uids.clone(),
        parent_uids: src.parent_uids.clone(),
    })
}
