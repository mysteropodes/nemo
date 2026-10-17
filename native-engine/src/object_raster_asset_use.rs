//! The single answer to "is this asset still referenced" (L06-H, G01 / G02 / G27).
//!
//! The attach/remove transaction asks it twice: once while it prepares a removal
//! (to report a typed `asset_in_use` refusal) and once more inside the revision
//! store right before publication (defence in depth), so both layers share one
//! definition and cannot drift. An asset is in use when
//! * an asset placement names it, or
//! * the project's still-image viewport reference carries its id, or
//! * the viewport reference shares its encoded allocation (pointer identity, the
//!   rule the owner ledger uses to charge shared bytes once).
//!
//! It is a pure read of one immutable project value: no I/O, no budget, no state.
//! The baseline media library (`removeEntry`, media-library.js) had no such check
//! because its entries never owned the canvas rasters; the in-use refusal is a
//! native rule required by the lane acceptance and is characterised only against
//! the native reference/placement values.
use crate::object_document::{ProjectEnvelope, ProjectReferenceState};
use std::sync::Arc;

// The read-queries layer may not import the document layer, but it may import the
// codec layer, which aliases this module (object_codec.rs). These re-exports hand
// it the asset value types through that one alias, so the codec file needs a single
// declaration and no per-name edit.
#[allow(unused_imports)]
pub(crate) use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource,
};

/// True when removing `id` would orphan a placement or the viewport reference.
pub(crate) fn asset_in_use(project: &ProjectEnvelope, id: &AssetId) -> bool {
    let Some(inventory) = project.asset_inventory() else {
        return false;
    };
    if inventory
        .placements()
        .iter()
        .any(|placement| placement.asset_id() == id)
    {
        return true;
    }
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        return false;
    };
    if reference.id() == id.as_str() {
        return true;
    }
    inventory
        .get(id)
        .and_then(|record| record.source().encoded_allocation())
        .is_some_and(|allocation| Arc::ptr_eq(allocation, reference.encoded_allocation()))
}
