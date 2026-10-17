//! Brush bitmap publication from an admitted embedded PNG asset (L07-G / G06).
//!
//! Joins two existing owners and adds no third: the immutable project asset
//! inventory of an `ObjectSnapshot` (the encoded bytes are an `AssetRecord`'s
//! retained allocation) and the charged `BrushBitmapStore` of L07-D. The PNG is
//! decoded exactly as `reference_image_resources::decode_managed` decodes the
//! reference still: charged workspace, charged row buffer, charged pixel
//! allocation, all reserved from the SAME `ImageResourceBudget` the store charges
//! (`BrushBitmapStore::budget`), never a second quota. The decoded straight RGBA8
//! is then copied into the store's own charged pending bitmap; the decode charge
//! is released before the call returns. The peak is therefore two copies of the
//! pixels (decode + store); `begin` may evict unpinned brush residents for it
//! through the caller's retire port and otherwise refuses with a typed
//! `Budget(Capacity)`.
//!
//! Identity: a published brush remembers the WEAK pointer of the encoded
//! allocation it came from. Reuse requires that same allocation (weak pointer
//! equality, which a recycled address cannot fool while the weak is held) AND the
//! store still serving the version this module published, so an equal id, an
//! equal path or equal bytes in a different allocation never match: they publish
//! a new version through the store's replacement fence.
//!
//! Publication is two-phase (`prepare` -> `commit`). Dropping or cancelling the
//! prepared value releases the charge and the id mark; `commit` re-resolves the
//! asset in the snapshot it is given and refuses (`Stale`) if the record no
//! longer retains the same encoded allocation. Nothing is persisted: the bitmap
//! is runtime cache state derived from the asset and re-derivable from it.

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::{
    BrushBitmap, BrushBitmapStore, BrushStoreError, PendingBrush, RetirePort,
};
use crate::object_codec::{
    decode_png_pixels_with_buffers, AssetKind, ImageAssetError, ImageAssetLimits, PngWorkspace,
};
use crate::object_snapshot::charged_png_workspace;
use crate::revision::{ImageResourceError, ObjectSnapshot};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrushAssetError {
    /// The snapshot carries no project envelope, hence no asset inventory.
    BareDrawing,
    NoInventory,
    MissingAsset,
    /// The record is not an image asset (a linked font, video or audio entry).
    WrongKind(AssetKind),
    /// An image record whose source is a link: there are no bytes to decode.
    NotEmbedded,
    Decode(ImageAssetError),
    /// The shared image budget refused a decode or store charge.
    Budget(ImageResourceError),
    /// The decoded size differs from the planned bitmap size.
    DimensionMismatch {
        expected: (u32, u32),
        actual: (u32, u32),
    },
    Store(BrushStoreError),
    /// At commit the snapshot no longer retains the allocation that was decoded.
    Stale,
}
impl From<ImageAssetError> for BrushAssetError {
    fn from(error: ImageAssetError) -> Self {
        Self::Decode(error)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BrushAssetRequest<'a> {
    /// `AssetRecord` id in the snapshot's inventory.
    pub(crate) asset_id: &'a str,
    /// Store id the bitmap is published under.
    pub(crate) brush_id: &'a str,
    /// Same admission envelope the native project/reference admission uses.
    pub(crate) limits: ImageAssetLimits,
    /// The planned bitmap size (the L07-E stamp plan canvas), when known.
    pub(crate) expected_size: Option<(u32, u32)>,
}

/// The decoded-and-copied, not yet visible half of a publication.
#[derive(Debug)]
pub(crate) struct FreshBrushAsset {
    pending: PendingBrush,
    /// Pins the decoded source allocation until commit or cancel.
    encoded: Arc<Vec<u8>>,
    asset_id: String,
}

#[derive(Debug)]
pub(crate) enum PreparedBrushAsset {
    /// The retained bitmap of the same allocation is still served: nothing to charge.
    Reused(BrushBitmap),
    Fresh(FreshBrushAsset),
}
impl PreparedBrushAsset {
    /// Releases the charge and the id mark; the store is untouched.
    pub(crate) fn cancel(self) {}
}

#[derive(Debug)]
pub(crate) struct BrushAssetPublication {
    pub(crate) bitmap: BrushBitmap,
    pub(crate) reused: bool,
}

#[derive(Debug)]
struct Tie {
    source: Weak<Vec<u8>>,
    version: u64,
}

/// The store plus, per published id, the allocation it was decoded from.
#[derive(Debug)]
pub(crate) struct BrushAssetPublisher {
    store: BrushBitmapStore,
    ties: BTreeMap<String, Tie>,
}

fn encoded_of(snapshot: &ObjectSnapshot, asset_id: &str) -> Result<Arc<Vec<u8>>, BrushAssetError> {
    let project = snapshot.project().ok_or(BrushAssetError::BareDrawing)?;
    let inventory = project
        .asset_inventory()
        .ok_or(BrushAssetError::NoInventory)?;
    let record = inventory
        .records()
        .iter()
        .find(|record| record.id().as_str() == asset_id)
        .ok_or(BrushAssetError::MissingAsset)?;
    if record.kind() != AssetKind::Image {
        return Err(BrushAssetError::WrongKind(record.kind()));
    }
    record
        .source()
        .encoded_allocation()
        .map(Arc::clone)
        .ok_or(BrushAssetError::NotEmbedded)
}

fn check_size(actual: (u32, u32), expected: Option<(u32, u32)>) -> Result<(), BrushAssetError> {
    match expected {
        Some(expected) if expected != actual => {
            Err(BrushAssetError::DimensionMismatch { expected, actual })
        }
        _ => Ok(()),
    }
}

impl BrushAssetPublisher {
    pub(crate) fn new(store: BrushBitmapStore) -> Self {
        Self {
            store,
            ties: BTreeMap::new(),
        }
    }
    pub(crate) fn store(&self) -> &BrushBitmapStore {
        &self.store
    }
    pub(crate) fn store_mut(&mut self) -> &mut BrushBitmapStore {
        &mut self.store
    }

    /// The resident bitmap of `brush_id` iff this publisher published it from the
    /// allocation `snapshot` retains for `asset_id` and the store still serves
    /// that version. Touches the LRU like any use.
    pub(crate) fn published_from(
        &mut self,
        snapshot: &ObjectSnapshot,
        brush_id: &str,
        asset_id: &str,
    ) -> Option<BrushBitmap> {
        let encoded = encoded_of(snapshot, asset_id).ok()?;
        self.reusable(brush_id, &encoded)
    }

    fn reusable(&mut self, brush_id: &str, encoded: &Arc<Vec<u8>>) -> Option<BrushBitmap> {
        let tie = self.ties.get(brush_id)?;
        if !Weak::ptr_eq(&tie.source, &Arc::downgrade(encoded)) {
            return None;
        }
        let version = tie.version;
        let bitmap = self.store.lease(brush_id)?;
        (bitmap.version() == version).then_some(bitmap)
    }

    /// Phase 1. A missing, linked, wrong-kind, corrupt, unsupported, oversize or
    /// unaffordable asset, or a size that disagrees with the plan, fails typed
    /// with the store, its marks and the budget as they were.
    pub(crate) fn prepare(
        &mut self,
        snapshot: &ObjectSnapshot,
        request: BrushAssetRequest<'_>,
        retire: RetirePort<'_>,
    ) -> Result<PreparedBrushAsset, BrushAssetError> {
        let encoded = encoded_of(snapshot, request.asset_id)?;
        if request.brush_id.is_empty() {
            return Err(BrushAssetError::Store(BrushStoreError::InvalidId));
        }
        if let Some(bitmap) = self.reusable(request.brush_id, &encoded) {
            check_size(bitmap.dimensions(), request.expected_size)?;
            return Ok(PreparedBrushAsset::Reused(bitmap));
        }
        let pending = self.decode_pending(&encoded, request, retire)?;
        Ok(PreparedBrushAsset::Fresh(FreshBrushAsset {
            pending,
            encoded,
            asset_id: request.asset_id.to_string(),
        }))
    }

    fn decode_pending(
        &mut self,
        encoded: &Arc<Vec<u8>>,
        request: BrushAssetRequest<'_>,
        retire: RetirePort<'_>,
    ) -> Result<PendingBrush, BrushAssetError> {
        let budget = self.store.budget().clone();
        // Declared before the decoded pixels: errors and unwinding destroy the
        // pixels first and only then return the quota.
        let mut charge = None;
        let decoded = decode_png_pixels_with_buffers(
            encoded.as_slice(),
            request.limits,
            || {
                charged_png_workspace(&budget, || Ok(PngWorkspace::new()))
                    .map_err(BrushAssetError::Budget)
            },
            |n| budget.allocate_bytes(n).map_err(BrushAssetError::Budget),
            |cost| {
                charge = Some(budget.reserve(cost).map_err(BrushAssetError::Budget)?);
                charge
                    .as_mut()
                    .expect("reserved before allocation")
                    .allocate()
                    .map_err(BrushAssetError::Budget)
            },
        )?;
        let actual = decoded.dimensions();
        check_size(actual, request.expected_size)?;
        let mut pending = self
            .store
            .begin(request.brush_id, actual.0, actual.1, retire)
            .map_err(BrushAssetError::Store)?;
        pending
            .fill(decoded.rgba8())
            .map_err(BrushAssetError::Store)?;
        drop(decoded);
        drop(charge);
        Ok(pending)
    }

    /// Phase 2. `current` is the snapshot the publication is for; the asset must
    /// still retain the decoded allocation. On any refusal the pending bitmap is
    /// dropped, which releases its charge and mark.
    pub(crate) fn commit(
        &mut self,
        prepared: PreparedBrushAsset,
        current: &ObjectSnapshot,
    ) -> Result<BrushBitmap, BrushAssetError> {
        let fresh = match prepared {
            PreparedBrushAsset::Reused(bitmap) => return Ok(bitmap),
            PreparedBrushAsset::Fresh(fresh) => fresh,
        };
        let FreshBrushAsset {
            pending,
            encoded,
            asset_id,
        } = fresh;
        let unchanged =
            encoded_of(current, &asset_id).is_ok_and(|retained| Arc::ptr_eq(&retained, &encoded));
        if !unchanged {
            return Err(BrushAssetError::Stale);
        }
        let brush_id = pending.id().to_string();
        let bitmap = self.store.commit(pending).map_err(BrushAssetError::Store)?;
        self.ties.insert(
            brush_id,
            Tie {
                source: Arc::downgrade(&encoded),
                version: bitmap.version(),
            },
        );
        let store = &self.store;
        self.ties.retain(|id, _| store.contains(id));
        Ok(bitmap)
    }

    pub(crate) fn publish(
        &mut self,
        snapshot: &ObjectSnapshot,
        request: BrushAssetRequest<'_>,
        retire: RetirePort<'_>,
    ) -> Result<BrushAssetPublication, BrushAssetError> {
        let prepared = self.prepare(snapshot, request, retire)?;
        let reused = matches!(prepared, PreparedBrushAsset::Reused(_));
        let bitmap = self.commit(prepared, snapshot)?;
        Ok(BrushAssetPublication { bitmap, reused })
    }
}

#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_publish_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_publish_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_publish.rs"]
mod tests;
