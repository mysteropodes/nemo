//! L06-E (lane #2112, package #2106, G27): the immutable decoded-pixel
//! resource handle of ONE embedded raster asset.
//!
//! `AssetPixels::retain(snapshot, asset_id, limits)` looks the asset up in the
//! snapshot's `AssetInventory` and decodes its retained PNG into straight-alpha
//! RGBA8 under the SHARED image-payload budget (`shared_image_resource_budget`,
//! the same domain as the viewport reference pixels). The handle pins, until the
//! last clone drops: the decoded RGBA allocation, its quota charge, the exact
//! `ObjectSnapshot` it was read from and the exact encoded `Arc<Vec<u8>>`.
//! Fields drop in declaration order: pixels, then the charge, then the pins.
//!
//! Identity is the encoded allocation, never an id string or equal bytes: two
//! records that share one `Arc` (the owner ledger's pointer-identity dedup)
//! share ONE decode and ONE charge through `retain_reusing`; equal bytes in
//! distinct allocations decode separately. A linked source is never opened or
//! resolved (`Unavailable`). No cache, registry, thread, file or GPU access
//! lives here; a later join (L06-K) and the owner transactions (L06-H) hold the
//! handles and decide when to release them.
//!
//! Layer note: this read-queries file names the document-layer asset types only
//! through the `object_codec` re-exports and imports no `object_document` path.
use crate::object_codec::{
    decode_png_pixels_with_buffers, AssetId, AssetInventory, AssetKind, DecodedPngPixels,
    ImageAssetError, ImageAssetLimits, PngWorkspace,
};
use crate::object_snapshot::charged_png_workspace;
use crate::revision::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceCharge, ImageResourceError,
    ObjectSnapshot,
};
use std::sync::Arc;

/// The only embedded media type the project owner admits for a raster asset
/// (`project_asset_admission.rs` compares it exactly).
const PNG_MEDIA_TYPE: &str = "image/png";

/// Distinct typed refusals; none of them leaves a charge or a partial handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetPixelsError {
    /// The snapshot is a bare drawing: it has no project and no asset inventory.
    BareDrawing,
    /// No such asset (unknown, invalid id spelling, or a project without a catalog).
    Missing,
    /// The record exists but its kind is not `Image`.
    WrongKind,
    /// A linked image: its locator is opaque and is never opened here.
    Unavailable,
    /// An embedded image whose media type has no decoder (only `image/png` has).
    DecoderUnavailable,
    /// The caller's cancel probe fired before any handle was published.
    Cancelled,
    Budget(ImageResourceError),
    Decode(ImageAssetError),
}
impl From<ImageAssetError> for AssetPixelsError {
    fn from(error: ImageAssetError) -> Self {
        Self::Decode(error)
    }
}

/// Fields drop in declaration order: pixels BEFORE the quota charge, then the
/// original pins. Never reorder: the quota must not be reusable while the
/// decoded bytes still exist.
#[derive(Debug)]
struct RetainedAsset {
    pixels: DecodedPngPixels,
    _charge: ImageResourceCharge,
    snapshot: ObjectSnapshot,
    encoded: Arc<Vec<u8>>,
}

/// Aliases own the same allocation AND charge. No detached Vec/Arc accessor.
#[derive(Debug, Clone)]
pub(crate) struct AssetPixels(Arc<RetainedAsset>);
impl AssetPixels {
    /// Decode one asset of `snapshot` under the shared budget.
    pub(crate) fn retain(
        snapshot: &ObjectSnapshot,
        asset_id: &str,
        limits: ImageAssetLimits,
    ) -> Result<Self, AssetPixelsError> {
        Self::retain_cancellable(snapshot, asset_id, limits, &[], &|| false)
    }
    /// Like `retain`, but an already held handle whose encoded allocation is the
    /// record's exact `Arc` is returned as an alias (one decode, one charge).
    /// The `limits` bound only a fresh decode; a reused handle keeps the limits
    /// of the decode that created it.
    pub(crate) fn retain_reusing(
        snapshot: &ObjectSnapshot,
        asset_id: &str,
        limits: ImageAssetLimits,
        held: &[AssetPixels],
    ) -> Result<Self, AssetPixelsError> {
        Self::retain_cancellable(snapshot, asset_id, limits, held, &|| false)
    }
    /// `cancel` is polled once before any work and, when a fresh decode ran,
    /// once more before the handle is returned; `true` drops every byte and
    /// charge and returns `Cancelled`.
    pub(crate) fn retain_cancellable(
        snapshot: &ObjectSnapshot,
        asset_id: &str,
        limits: ImageAssetLimits,
        held: &[AssetPixels],
        cancel: &dyn Fn() -> bool,
    ) -> Result<Self, AssetPixelsError> {
        let request = Request {
            snapshot,
            asset_id,
            limits,
            held,
            cancel,
        };
        retain_core(
            &request,
            shared_image_resource_budget(),
            |charge| charge.allocate(),
            |retained| Ok(Self(Arc::new(retained))),
        )
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        self.0.pixels.dimensions()
    }
    pub(crate) fn row_stride(&self) -> usize {
        self.0.pixels.row_stride()
    }
    pub(crate) fn rgba8(&self) -> &[u8] {
        self.0.pixels.rgba8()
    }
    pub(crate) fn encoded_png(&self) -> &[u8] {
        &self.0.encoded
    }
    /// The exact immutable snapshot this handle was read from.
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.0.snapshot
    }
    pub(crate) fn same_allocation_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// True when `asset_id` of `snapshot` is an embedded PNG image whose encoded
    /// allocation is the very `Arc` this handle pinned (identity, not equality).
    pub(crate) fn matches_asset(&self, snapshot: &ObjectSnapshot, asset_id: &str) -> bool {
        resolve(snapshot, asset_id).is_ok_and(|encoded| Arc::ptr_eq(&self.0.encoded, &encoded))
    }
}
impl AsRef<[u8]> for AssetPixels {
    fn as_ref(&self) -> &[u8] {
        self.rgba8()
    }
}

/// Inventory lookup. The kind is checked before the source, so a linked audio
/// asset is `WrongKind` and only a linked IMAGE is `Unavailable`. The locator of
/// a linked source is not read.
fn resolve_in(
    inventory: &AssetInventory,
    asset_id: &str,
) -> Result<Arc<Vec<u8>>, AssetPixelsError> {
    let id = AssetId::new(asset_id.to_owned()).map_err(|_| AssetPixelsError::Missing)?;
    let record = inventory.get(&id).ok_or(AssetPixelsError::Missing)?;
    if record.kind() != AssetKind::Image {
        return Err(AssetPixelsError::WrongKind);
    }
    let source = record.source();
    let Some(encoded) = source.encoded_allocation() else {
        return Err(AssetPixelsError::Unavailable);
    };
    if source.media_type() != Some(PNG_MEDIA_TYPE) {
        return Err(AssetPixelsError::DecoderUnavailable);
    }
    Ok(Arc::clone(encoded))
}
fn resolve(snapshot: &ObjectSnapshot, asset_id: &str) -> Result<Arc<Vec<u8>>, AssetPixelsError> {
    let project = snapshot.project().ok_or(AssetPixelsError::BareDrawing)?;
    let inventory = project.asset_inventory().ok_or(AssetPixelsError::Missing)?;
    resolve_in(inventory, asset_id)
}

struct Request<'a> {
    snapshot: &'a ObjectSnapshot,
    asset_id: &'a str,
    limits: ImageAssetLimits,
    held: &'a [AssetPixels],
    cancel: &'a dyn Fn() -> bool,
}

/// `allocate` and `wrap` are the only seams: production passes the charge's own
/// allocator and the `Arc` carrier, the tests pass observers.
fn retain_core(
    request: &Request<'_>,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
    wrap: impl FnOnce(RetainedAsset) -> Result<AssetPixels, AssetPixelsError>,
) -> Result<AssetPixels, AssetPixelsError> {
    if (request.cancel)() {
        return Err(AssetPixelsError::Cancelled);
    }
    let encoded = resolve(request.snapshot, request.asset_id)?;
    if let Some(shared) = request
        .held
        .iter()
        .find(|held| Arc::ptr_eq(&held.0.encoded, &encoded))
    {
        return Ok(shared.clone());
    }
    // Pin the immutable document before decoding; the encoded Arc is already held.
    let snapshot = request.snapshot.clone();
    // Declared before the decoder's Vec: errors and unwind destroy bytes before charge.
    let mut reserved = None;
    let pixels = decode_png_pixels_with_buffers(
        &encoded,
        request.limits,
        || {
            charged_png_workspace(budget, || Ok(PngWorkspace::new()))
                .map_err(AssetPixelsError::Budget)
        },
        |n| budget.allocate_bytes(n).map_err(AssetPixelsError::Budget),
        |cost| {
            let charge = reserved.insert(budget.reserve(cost).map_err(AssetPixelsError::Budget)?);
            allocate(charge).map_err(AssetPixelsError::Budget)
        },
    )?;
    let Some(charge) = reserved else {
        return Err(AssetPixelsError::Decode(ImageAssetError::Allocation));
    };
    // No fallible work between the pixels and their carrier: a return from here
    // on drops the carrier in field order, pixels first.
    let retained = RetainedAsset {
        pixels,
        _charge: charge,
        snapshot,
        encoded,
    };
    if (request.cancel)() {
        return Err(AssetPixelsError::Cancelled);
    }
    wrap(retained)
}

#[cfg(test)]
#[path = "../tests/object_raster_asset_resources_cancel.rs"]
mod cancel;
#[cfg(test)]
#[path = "../tests/object_raster_asset_resources_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_raster_asset_resources_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/object_raster_asset_resources_refusals.rs"]
mod refusals;
#[cfg(test)]
#[path = "../tests/object_raster_asset_resources.rs"]
mod tests;
