//! The mesh half of a reset (I13): `rigResetPose` ends with `applyRigDeform`, which at the rest pose
//! writes an exact `[0, 0]` into the offsets of every mesh bind the relink kept
//! (`if(!d[0]&&!d[1]){mesh.offsets[mi]=[0,0];continue;}`), overwriting the stored sculpt as well.
//!
//! Natively the pose is derived, so what remains to persist is the zero. The image-mesh store of L03-J
//! (`ObjectDocument::image_meshes`) holds the offsets; a bind is reached exactly when its mesh is in the
//! store with the vertex count the bind was made against (the same predicate `bind_link` applies; the
//! raster that carries the mesh is the documented gap of L03-N/L03-R). Binds that are not reached are
//! counted, never dropped or rewritten. An offsets array that already is all zero (or absent, which
//! `load()` reads as zeros) is left as authored, so a reset that changes nothing rewrites no byte.
//! Unlike a commit, a reset needs no raster rect: zero displacement needs no placement.
use crate::commands::object_fill_owner::widget_edit::object_rig_pose_commit::PoseRefusal;
use crate::object_document::object_rig_record::{BindTarget, RigRecord};
use crate::object_document::{ImageMesh, ImageMeshStore, ObjectDocument, Presence};
use serde_json::Number;

/// How many binds a reset zeroed and how many it could not reach.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct MeshCounts {
    pub(super) zeroed: u32,
    pub(super) unlinked: u32,
}

/// Recomputes the offsets of every reachable mesh bind of `rig` to zero in `document`.
pub(super) fn zero_pose_offsets(
    document: &mut ObjectDocument,
    rig: &RigRecord,
) -> Result<MeshCounts, PoseRefusal> {
    let mut binds: Vec<(&str, usize)> = Vec::new();
    for bind in &rig.binds {
        if let Some(BindTarget::Mesh(id)) = bind.target() {
            binds.push((id, bind.rest.len()));
        }
    }
    let mut counts = MeshCounts::default();
    if binds.is_empty() {
        return Ok(counts);
    }
    let mut entries: Vec<(String, ImageMesh)> = match document.image_meshes.value() {
        Some(store) => store.entries().to_vec(),
        None => {
            counts.unlinked = binds.len() as u32;
            return Ok(counts);
        }
    };
    let mut changed = false;
    for (id, rest_len) in binds {
        let slot = entries.iter().position(|(key, _)| key.as_str() == id);
        match slot {
            Some(at) if entries[at].1.verts.len() == rest_len => {
                let mesh = &mut entries[at].1;
                let at_rest = mesh
                    .effective_offsets()
                    .iter()
                    .all(|offset| offset[0] == 0.0 && offset[1] == 0.0);
                if !at_rest {
                    let zero = [Number::from(0i64), Number::from(0i64)];
                    mesh.offsets = Presence::Present(vec![zero; mesh.verts.len()]);
                    counts.zeroed += 1;
                    changed = true;
                }
            }
            _ => counts.unlinked += 1,
        }
    }
    if changed {
        let store = ImageMeshStore::try_new(entries).map_err(|_| PoseRefusal::Internal)?;
        document.image_meshes = Presence::Present(store);
    }
    Ok(counts)
}
