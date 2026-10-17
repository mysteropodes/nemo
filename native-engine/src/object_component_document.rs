//! Document admission of the lane-4 tables (L04-I, I14/I35): the optional
//! `componentTables` member of a v2 `ObjectDocument`, its read accessors and
//! the project-level resolution of StoryBoard sound assets.
//!
//! The member groups the three L04-G record tables behind ONE presence flag so
//! that every document constructor adds a single field:
//!
//! - `components`: the symbol table, the instance `sym*` placement records,
//!   the exposed properties and the held `componentFrame` / `blankOverride`
//!   frame keys (`state.symbols` and the instance layers of the baseline);
//! - `storyboard`: the StoryBoard modules and chains (`state.storyboard`);
//! - `duplicators`: the duplicator descriptors with their inline effectors and
//!   the effector layers (`ld.duplicator`, `ld.isEffectorLayer`).
//!
//! Absent, present-empty and explicit are kept apart at both levels: a missing
//! `componentTables` is not an empty one, and a table that is not authored
//! stays missing instead of becoming an empty record. The member is a v2
//! member only (components are nested contexts, which the v1 format cannot
//! name); v1 keeps rejecting it as an unknown member, exactly as it rejects
//! `compositionOrder`. Cross-record admission lives in `object_component_codec`
//! (codec layer); this file owns the storage shape and the document-layer
//! reads only. Component inner layers and strokes stay inside their component
//! record, so `ObjectDocument.layers` is still the scene-root layer list.
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::object_component_record_kernels::object_duplicator_records::DuplicatorSet;
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecord;
use crate::object_document::{
    object_deserialize, AssetId, AssetInventory, AssetKind, ObjectDocument, Presence,
};
use serde::{Deserialize, Serialize};

/// The three record tables of lane 4. Each table is optional on its own.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentTables {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) components: Presence<ComponentSet>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) storyboard: Presence<StoryboardRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) duplicators: Presence<DuplicatorSet>,
}
object_deserialize!(ComponentTables {
    #[serde(default)] components: Presence<ComponentSet>,
    #[serde(default)] storyboard: Presence<StoryboardRecord>,
    #[serde(default)] duplicators: Presence<DuplicatorSet>
});

impl ObjectDocument {
    /// The authored tables; `None` is an absent member, distinct from `Some` of empty tables.
    pub(crate) fn component_tables(&self) -> Option<&ComponentTables> {
        self.component_tables.value()
    }
    pub(crate) fn component_set(&self) -> Option<&ComponentSet> {
        self.component_tables
            .value()
            .and_then(|tables| tables.components.value())
    }
    pub(crate) fn storyboard(&self) -> Option<&StoryboardRecord> {
        self.component_tables
            .value()
            .and_then(|tables| tables.storyboard.value())
    }
    pub(crate) fn duplicator_set(&self) -> Option<&DuplicatorSet> {
        self.component_tables
            .value()
            .and_then(|tables| tables.duplicators.value())
    }
}

/// Why a StoryBoard sound cannot be resolved against the project's assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SoundAssetError {
    /// The sound names an asset the project does not hold, or the project has no asset inventory.
    Unresolved,
    /// The asset exists but is not an audio asset.
    NotAudio,
}

/// Every StoryBoard sound `assetId` must name an audio record of the project
/// inventory. A drawing alone (no project, or a project without an inventory)
/// cannot resolve a sound, so a document that authors one is refused there
/// instead of keeping a dangling reference.
pub(crate) fn resolve_sound_assets(
    document: &ObjectDocument,
    inventory: Option<&AssetInventory>,
) -> Result<(), SoundAssetError> {
    let Some(storyboard) = document.storyboard() else {
        return Ok(());
    };
    for module in storyboard.modules() {
        if module.kind() != "sound" {
            continue;
        }
        let asset = module.asset_id.value().ok_or(SoundAssetError::Unresolved)?;
        let id = AssetId::new(asset.clone()).map_err(|_| SoundAssetError::Unresolved)?;
        let record = inventory
            .and_then(|inventory| inventory.get(&id))
            .ok_or(SoundAssetError::Unresolved)?;
        if record.kind() != AssetKind::Audio {
            return Err(SoundAssetError::NotAudio);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_component_document_assets.rs"]
mod assets_tests;
#[cfg(test)]
#[path = "../tests/object_component_document_cases.rs"]
mod cases;
#[cfg(test)]
#[path = "../tests/object_component_document_controls.rs"]
mod controls_tests;
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_component_document_history.rs"]
mod history_tests;
#[cfg(test)]
#[path = "../tests/object_component_document.rs"]
mod tests;
#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_component_document_views.rs"]
mod views_tests;
