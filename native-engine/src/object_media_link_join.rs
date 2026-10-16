//! Join of the link-state kernel to the native asset catalog (G32).
//!
//! The catalog keeps a linked source as ONE opaque locator
//! (`AssetSource::locator()`); the baseline kept two persisted spellings, a
//! desktop `linkedPath` and a web `linkedHandleId`. The catalog does not record
//! which, so the caller names the [`LocatorNamespace`]; this module never
//! guesses it from the text and never resolves, trims or normalizes a locator.
//!
//! [`export_gate`] is the typed disposition an export consumes before it asks
//! for pixels: a link that is not resolved is a [`MissingLink`] naming the asset
//! and the placement, never a blank frame presented as success. The baseline
//! links images and videos only (audio is always embedded and fonts are another
//! owner's), so other kinds are refused instead of given invented semantics.

use crate::object_document::media_timing::object_media_link_state::{
    FailureReason, LinkDisposition, LinkedRef, ResolveTable,
};
use crate::object_document::{AssetId, AssetKind, AssetPlacement, AssetRecord};

/// Which persisted spelling the opaque locator stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocatorNamespace {
    /// A filesystem path (`linkedPath`, key `'p:' + path`).
    DesktopPath,
    /// A web file-handle id (`linkedHandleId`, key `'w:' + id`).
    WebHandle,
}

/// Why a linked asset cannot supply pixels now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MissingLinkReason {
    /// The locator yields no reference at all.
    Unlinked,
    /// No verdict yet (never attempted, or in flight).
    NotResolved,
    Missing,
    PermissionNeeded,
}

/// A linked asset that cannot be used, with the ids an error report needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MissingLink {
    pub(crate) asset_id: AssetId,
    pub(crate) placement_id: String,
    pub(crate) reason: MissingLinkReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkGateError {
    MissingLink(MissingLink),
    /// The placement belongs to a different asset than the record given.
    PlacementAssetMismatch,
    /// The baseline has no linked-media semantics for this kind.
    UnsupportedKind(AssetKind),
}

/// A gate that passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkReadiness {
    /// Bytes are embedded; there is no link to resolve.
    Embedded,
    /// The linked bytes are cached.
    Resolved,
}

/// The reference of a linked record, borrowing the locator unchanged; `None`
/// for an embedded source.
pub(crate) fn link_ref_of(record: &AssetRecord, ns: LocatorNamespace) -> Option<LinkedRef<'_>> {
    let locator = record.source().locator()?;
    Some(match ns {
        LocatorNamespace::DesktopPath => LinkedRef {
            linked_path: Some(locator),
            linked_handle_id: None,
        },
        LocatorNamespace::WebHandle => LinkedRef {
            linked_path: None,
            linked_handle_id: Some(locator),
        },
    })
}

/// Whether `placement` of `record` may be exported at `now_ms`.
pub(crate) fn export_gate(
    table: &ResolveTable,
    record: &AssetRecord,
    placement: &AssetPlacement,
    ns: LocatorNamespace,
    now_ms: i64,
) -> Result<LinkReadiness, LinkGateError> {
    if placement.asset_id() != record.id() {
        return Err(LinkGateError::PlacementAssetMismatch);
    }
    let Some(r) = link_ref_of(record, ns) else {
        return Ok(LinkReadiness::Embedded);
    };
    if !matches!(record.kind(), AssetKind::Image | AssetKind::Video) {
        return Err(LinkGateError::UnsupportedKind(record.kind()));
    }
    let by_failure = || match table.failure(&r).map(|f| f.reason) {
        Some(FailureReason::Missing) => MissingLinkReason::Missing,
        Some(FailureReason::PermissionNeeded) => MissingLinkReason::PermissionNeeded,
        None => MissingLinkReason::NotResolved,
    };
    let reason = match table.disposition(&r, now_ms) {
        LinkDisposition::Resolved => return Ok(LinkReadiness::Resolved),
        LinkDisposition::Unlinked => MissingLinkReason::Unlinked,
        LinkDisposition::Pending => MissingLinkReason::NotResolved,
        LinkDisposition::CoolingDown => by_failure(),
        LinkDisposition::Missing => MissingLinkReason::Missing,
        LinkDisposition::PermissionNeeded => MissingLinkReason::PermissionNeeded,
    };
    Err(LinkGateError::MissingLink(MissingLink {
        asset_id: record.id().clone(),
        placement_id: placement.id().to_owned(),
        reason,
    }))
}

#[cfg(test)]
#[path = "../tests/object_media_link_join.rs"]
mod tests;
