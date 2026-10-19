//! Real frame-independent Motion holder sources. A raster's mesh holder uses
//! its persistent meshId (the baseline elementMotion key), never a frame-local
//! raster id. Reading a vertex offset does not include the mesh's rest sculpt.
use crate::object_document::{FrameScopeKind, ImageMesh, ImageMeshStore, ItemRecords,
    ObjectDocument, ObjectRecord, ObjectTarget, PropertyHolder, PropertyHolderTarget};

fn same_layer(item: &ObjectTarget, target: &PropertyHolderTarget) -> bool {
    item.context_id() == target.context_id() && item.layer_uid() == target.layer_uid()
        && item.scope_kind() == &FrameScopeKind::Authored
}

pub(super) fn mesh<'a>(items: Option<&ItemRecords>, meshes: Option<&'a ImageMeshStore>,
    target: &PropertyHolderTarget) -> Option<&'a ImageMesh> {
    let id = target.stroke_id()?;
    items?.raster_entries().iter().any(|r|same_layer(&r.target,target) && r.item.mesh_id()==Some(id))
        .then_some(())?;
    meshes?.get(id)
}

pub(super) fn item_target(items: Option<&ItemRecords>, target: &PropertyHolderTarget) -> bool {
    let Some(id)=target.stroke_id() else { return false };
    items.is_some_and(|items|items.targets().any(|item|same_layer(item,target) && item.stroke_id()==id))
}

fn path_vertices(records: &[ObjectRecord], target: &PropertyHolderTarget) -> Option<usize> {
    records.iter().filter(|r|same_layer(r.target(),target) && Some(r.target().stroke_id())==target.stroke_id())
        .map(|r|r.geometry.segments.len()).max()
}

impl PropertyHolder {
    pub(crate) fn vertex_index(property: &str) -> Option<usize> {
        super::catalog::vertex_index(property)
    }
    pub(crate) fn bound_mesh<'a>(document: &'a ObjectDocument, target: &PropertyHolderTarget) -> Option<&'a ImageMesh> {
        mesh(document.items.value(),document.image_meshes(),target)
    }
    pub(crate) fn item_target_exists(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
        item_target(document.items.value(),target) || Self::bound_mesh(document,target).is_some()
    }
    pub(crate) fn vertex_count(document: &ObjectDocument, target: &PropertyHolderTarget) -> Option<usize> {
        if let Some(mesh)=Self::bound_mesh(document,target) { return Some(mesh.verts.len()); }
        let records = document.component_set().and_then(|set|set.component(target.context_id()))
            .map_or(document.objects(),|c|c.objects());
        path_vertices(records,target)
    }
    /// Stored tracks survive a topology shrink. New selectors cannot invent a
    /// vertex, but an already-authored orphan track is retained and readable.
    pub(crate) fn metadata_in_document(&self, document: &ObjectDocument, property: &str)
        -> Option<super::PropertyMetadata> {
        let mut metadata=self.metadata(property)?;
        if let Some(index)=super::catalog::vertex_index(property) {
            let count=Self::vertex_count(document,self.target())?;
            if index>=count && !self.properties().iter().any(|r|r.property()==property) { return None; }
            metadata.unit=if Self::bound_mesh(document,self.target()).is_some() { "%" } else { "px" };
        }
        Some(metadata)
    }
}

pub(super) fn has_vertex_source(items: Option<&ItemRecords>, meshes: Option<&ImageMeshStore>,
    records: &[ObjectRecord], target: &PropertyHolderTarget) -> bool {
    mesh(items,meshes,target).is_some() || path_vertices(records,target).is_some()
}
