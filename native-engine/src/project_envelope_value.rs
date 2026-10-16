//! Immutable Envelope value responsibility.
use crate::object_document::{
    AssetInventory, ObjectDocument, ProjectMetadata, ProjectReferenceState,
};
use std::sync::Arc;

/// Immutable composite value retained by the single revision/history owner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectEnvelope {
    project: ProjectMetadata,
    drawing: Arc<ObjectDocument>,
    reference: ProjectReferenceState,
    asset_inventory: Option<AssetInventory>,
}
impl ProjectEnvelope {
    /// The input drawing has ALREADY passed the existing strict storage codec.
    /// Metadata validation is embodied by ProjectMetadata; no second admission,
    /// migration, history, decoder or wire interpretation runs at this floor.
    pub(crate) fn no_media(project: ProjectMetadata, drawing: ObjectDocument) -> Self {
        Self {
            project,
            drawing: Arc::new(drawing),
            reference: ProjectReferenceState::NoMedia,
            asset_inventory: None,
        }
    }
    /// Replace settings without copying drawing or any retained media allocation.
    pub(crate) fn with_metadata(&self, project: ProjectMetadata) -> Self {
        Self {
            project,
            ..self.clone()
        }
    }
    pub(crate) fn project(&self) -> &ProjectMetadata {
        &self.project
    }
    pub(crate) fn drawing(&self) -> &ObjectDocument {
        &self.drawing
    }
    /// Replacement drawing has already passed the existing command admission.
    pub(crate) fn with_drawing(&self, drawing: ObjectDocument) -> Self {
        Self {
            project: self.project,
            drawing: Arc::new(drawing),
            reference: self.reference.clone(),
            asset_inventory: self.asset_inventory.clone(),
        }
    }
    /// Returns a new immutable value sharing the ORIGINAL drawing authority.
    /// It does not publish a revision, mutate history or activate media.
    pub(crate) fn with_reference(&self, reference: ProjectReferenceState) -> Self {
        Self {
            project: self.project,
            drawing: Arc::clone(&self.drawing),
            reference,
            asset_inventory: self.asset_inventory.clone(),
        }
    }
    pub(crate) fn asset_inventory(&self) -> Option<&AssetInventory> {
        self.asset_inventory.as_ref()
    }
    pub(crate) fn with_asset_inventory(&self, assets: Option<AssetInventory>) -> Self {
        Self {
            project: self.project,
            drawing: Arc::clone(&self.drawing),
            reference: self.reference.clone(),
            asset_inventory: assets,
        }
    }
    pub(crate) fn reference_state(&self) -> ProjectReferenceState {
        self.reference.clone()
    }
}
