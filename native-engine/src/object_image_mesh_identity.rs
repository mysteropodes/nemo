//! Mesh identities belong to the revision owner's lifetime, including initial admission and
//! successful replacement. A retained holder reserves its identity without retaining a mesh resource.
use crate::commands::ObjectFillOwner;
use crate::object_document::{MeshIdSequence, ObjectDocument};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectRevisionOwner;

/// Observe a detached copy first: any refusal leaves the authoritative high-water mark unchanged.
fn observed_sequence(
    mut sequence: MeshIdSequence,
    document: &ObjectDocument,
) -> Result<MeshIdSequence, DispatchErrorCode> {
    if let Some(store) = document.image_meshes() {
        sequence
            .observe_store(store)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
    }
    for holder in document.property_holders().unwrap_or(&[]) {
        if holder.has_retained_mesh_source() {
            // Admitted provenance requires meshSource.meshId == this unchanged target identity.
            let id = holder
                .target()
                .stroke_id()
                .ok_or(DispatchErrorCode::InvalidRequest)?;
            sequence
                .observe(id)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
    }
    Ok(sequence)
}

impl ObjectFillOwner {
    pub(super) fn from_identity_snapshot(
        snapshot: ObjectSnapshot,
    ) -> Result<Self, DispatchErrorCode> {
        let mesh_id_sequence = observed_sequence(MeshIdSequence::default(), snapshot.document())?;
        Ok(Self {
            revisions: ObjectRevisionOwner::from_admitted(snapshot),
            mesh_id_sequence,
        })
    }

    #[cfg(feature = "project_media_codec")]
    pub(crate) fn replace_admitted_snapshot(
        &mut self,
        snapshot: ObjectSnapshot,
    ) -> Result<(), DispatchErrorCode> {
        let mesh_id_sequence = observed_sequence(self.mesh_id_sequence, snapshot.document())?;
        self.revisions = ObjectRevisionOwner::from_admitted(snapshot);
        self.mesh_id_sequence = mesh_id_sequence;
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "history",
    feature = "evaluation",
    feature = "project_media_codec"
))]
#[path = "../tests/object_image_mesh_binding_identity.rs"]
mod tests;
