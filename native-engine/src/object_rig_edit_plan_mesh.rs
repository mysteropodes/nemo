//! Mesh half of the rig edit plans (L03-O, I12): what `rigBindMesh` and the mesh passes of
//! `rigAutoAssignLayer` (`src/js/app.js`, baseline 1065e9d8) compute from the pinned document, as plain
//! values. Nothing here publishes or mutates anything.
//!
//! The retained mesh owner is the L03-J store (`ObjectDocument::image_meshes`: normalized rest vertices)
//! and the L06-C raster records (`items.raster`: the display rect and the `meshId` tag). A bind records
//! the REST vertices mapped through the raster rect into world space (`SMImageMesh.worldVerts` with every
//! offset zeroed, so an existing sculpt is never baked into the rest pose); the weights are the
//! control-point weights of the stroke bind. Neither the mesh store nor a raster record is read for
//! anything else and none is ever written: a rig edit cannot mutate rest geometry.
//!
//! Where the native model differs from the baseline, each pinned by a test:
//! - the baseline reads the LIVE raster of the current frame; natively the raster is the record authored on
//!   `frame` of the layer. `bindMesh` without a frame accepts the mesh only when every authored raster that
//!   carries it has the same placement (otherwise the rest pose would be a guess) and refuses with
//!   `AmbiguousBind` (`invalid_request`) when they differ;
//! - a mesh the store does not hold (the baseline returns `false` and binds nothing) is `NoMesh`
//!   (`not_found`) for `bindMesh`, and is skipped without counting by `autoAssign`, like the baseline;
//! - a mesh without vertices cannot be weighed: `EmptyStroke`.
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::plan::{
    kernel_bones, resolve_bone_ids, rig_of, PlanRefusal,
};
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::rules::js_number;
use crate::object_document::object_rig_record::{RigPair, RigRecord};
use crate::object_document::{FrameScopeKind, ObjectDocument, RasterEntry};
use crate::object_snapshot::object_rig_edit_read::{
    mesh_rest_world, weigh_control, ControlEntry, MeshPlacement,
};

/// A mesh carried by a raster, its rest vertices already in world space.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshWorld {
    pub(crate) mesh_id: String,
    pub(crate) world: Vec<[f64; 2]>,
}

/// What `rigBindMesh` pushes, before it becomes a record: `{meshId, rest, weights}`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlannedMeshBind {
    pub(crate) mesh_id: String,
    pub(crate) rest: Vec<RigPair>,
    pub(crate) weights: Vec<Vec<ControlEntry>>,
}

fn rasters_of<'a>(
    document: &'a ObjectDocument,
    layer_uid: &'a str,
    frame: Option<u32>,
) -> impl Iterator<Item = &'a RasterEntry> + 'a {
    document
        .item_records()
        .map_or(&[][..], |items| items.raster_entries())
        .iter()
        .filter(move |entry| {
            entry.target.layer_uid() == layer_uid
                && *entry.target.scope_kind() == FrameScopeKind::Authored
                && frame.is_none_or(|frame| entry.target.frame() == frame)
        })
}

/// The mesh of one raster in world space; `None` when the raster carries no mesh the store holds.
fn world_of(
    document: &ObjectDocument,
    entry: &RasterEntry,
) -> Result<Option<MeshWorld>, PlanRefusal> {
    let Some(mesh_id) = entry.item.mesh_id().filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let Some(mesh) = document.image_meshes().and_then(|store| store.get(mesh_id)) else {
        return Ok(None);
    };
    let rect = entry.item.display_rect();
    let placement = MeshPlacement {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        rotation: rect.rotation,
    };
    let world = mesh_rest_world(&mesh.rest_points(), placement)?;
    Ok(Some(MeshWorld {
        mesh_id: mesh_id.to_owned(),
        world,
    }))
}

/// Every meshed raster of the layer at `frame`, in record order (the baseline's layer-children order).
pub(crate) fn layer_meshes(
    document: &ObjectDocument,
    layer_uid: &str,
    frame: u32,
) -> Result<Vec<MeshWorld>, PlanRefusal> {
    let mut found = Vec::new();
    for entry in rasters_of(document, layer_uid, Some(frame)) {
        if let Some(mesh) = world_of(document, entry)? {
            found.push(mesh);
        }
    }
    Ok(found)
}

/// The weights and rest numbers of one mesh bind.
pub(crate) fn bind_for(
    rig: &RigRecord,
    bone_ids: &[String],
    mesh: &MeshWorld,
    radius: Option<f64>,
    softness: Option<f64>,
) -> Result<PlannedMeshBind, PlanRefusal> {
    if mesh.world.is_empty() {
        return Err(PlanRefusal::EmptyStroke);
    }
    let weights = weigh_control(&kernel_bones(rig), bone_ids, &mesh.world, radius, softness)?;
    let rest = mesh
        .world
        .iter()
        .map(|point| {
            let x = js_number(point[0]).ok_or(PlanRefusal::BadVertex)?;
            let y = js_number(point[1]).ok_or(PlanRefusal::BadVertex)?;
            Ok([x, y])
        })
        .collect::<Result<Vec<_>, PlanRefusal>>()?;
    Ok(PlannedMeshBind {
        mesh_id: mesh.mesh_id.clone(),
        rest,
        weights,
    })
}

/// `rigBindMesh(ld, raster, boneIds, radius, rotate, softness)` for the raster that carries `mesh_id`
/// on the layer (at `frame`, or on every authored frame when it is absent); `bone_ids` of `None` means
/// every bone.
pub(crate) fn plan_mesh_bind(
    document: &ObjectDocument,
    layer_uid: &str,
    mesh_id: &str,
    frame: Option<u32>,
    bone_ids: Option<&[String]>,
    radius: Option<f64>,
    softness: Option<f64>,
) -> Result<PlannedMeshBind, PlanRefusal> {
    let rig = rig_of(document, layer_uid)?;
    let mut carried = Vec::new();
    for entry in rasters_of(document, layer_uid, frame) {
        if entry.item.mesh_id() == Some(mesh_id) {
            if let Some(mesh) = world_of(document, entry)? {
                carried.push(mesh);
            }
        }
    }
    let first = carried.first().ok_or(PlanRefusal::NoMesh)?;
    if carried.iter().any(|other| other != first) {
        return Err(PlanRefusal::AmbiguousBind);
    }
    let ids = resolve_bone_ids(rig, bone_ids)?;
    bind_for(rig, &ids, first, radius, softness)
}
