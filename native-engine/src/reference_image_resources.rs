//! Actual immutable snapshot -> retained CPU pixels. No cache or GPU acceptance.
use crate::object_codec::{
    decode_png_pixels_with_buffers, DecodedPngPixels, ImageAssetError, ImageAssetLimits,
    PngWorkspace, ProjectReferenceState,
};
use crate::object_snapshot::charged_png_workspace as charged_workspace;
use crate::revision::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceCharge, ImageResourceError,
    ObjectSnapshot,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceResourceError {
    BareDrawing,
    NoMedia,
    Budget(ImageResourceError),
    Decode(ImageAssetError),
}
impl From<ImageAssetError> for ReferenceResourceError {
    fn from(error: ImageAssetError) -> Self {
        Self::Decode(error)
    }
}

/// Fields drop in declaration order: pixels BEFORE quota, then the original pins.
#[derive(Debug)]
struct RetainedPixels {
    pixels: DecodedPngPixels,
    _charge: ImageResourceCharge,
    snapshot: ObjectSnapshot,
    encoded: Arc<Vec<u8>>,
}
/// Aliases own the same allocation AND charge. No detached Vec/Arc<Vec> accessor.
#[derive(Debug, Clone)]
pub(crate) struct ReferencePixels(Arc<RetainedPixels>);
impl ReferencePixels {
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
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.0.snapshot
    }
    pub(crate) fn same_allocation_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn matches_image(&self, snapshot: &ObjectSnapshot) -> bool {
        snapshot.project().is_some_and(|project| {
            matches!(project.reference_state(), ProjectReferenceState::StillImage(image)
                if Arc::ptr_eq(&self.0.encoded, image.encoded_allocation()))
        })
    }
}
impl AsRef<[u8]> for ReferencePixels {
    fn as_ref(&self) -> &[u8] {
        self.rgba8()
    }
}

pub(crate) fn decode_snapshot_reference(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_managed(
        snapshot,
        limits,
        shared_image_resource_budget(),
        || Ok(PngWorkspace::new()),
        |charge| charge.allocate(),
    )
}

fn decode_managed(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    initialize: impl FnOnce() -> Result<PngWorkspace, ImageResourceError>,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_using_buffers(
        snapshot,
        limits,
        budget,
        allocate,
        || charged_workspace(budget, initialize).map_err(ReferenceResourceError::Budget),
        |n| {
            budget
                .allocate_bytes(n)
                .map_err(ReferenceResourceError::Budget)
        },
        |pixels| Ok(ReferencePixels(Arc::new(pixels))),
    )
}

// Explicit test-only uncharged workspace factory preserves the RGBA-only controls.
#[cfg(test)]
fn decode_with_budget(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_with_factory(snapshot, limits, budget, allocate, |pixels| {
        Ok(ReferencePixels(Arc::new(pixels)))
    })
}
#[cfg(test)]
fn decode_with_factory(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
    retain: impl FnOnce(RetainedPixels) -> Result<ReferencePixels, ReferenceResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_using_buffers(
        snapshot,
        limits,
        budget,
        allocate,
        || Ok(Box::new(PngWorkspace::new())),
        |n| {
            let mut row = Vec::new();
            row.try_reserve_exact(n)
                .map_err(|_| ReferenceResourceError::Decode(ImageAssetError::Allocation))?;
            row.resize(n, 0);
            Ok(row)
        },
        retain,
    )
}
fn decode_using_buffers<W: AsMut<PngWorkspace>, R: AsMut<[u8]>>(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
    workspace: impl FnOnce() -> Result<W, ReferenceResourceError>,
    row: impl FnOnce(usize) -> Result<R, ReferenceResourceError>,
    retain: impl FnOnce(RetainedPixels) -> Result<ReferencePixels, ReferenceResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    let project = snapshot
        .project()
        .ok_or(ReferenceResourceError::BareDrawing)?;
    let ProjectReferenceState::StillImage(image) = project.reference_state() else {
        return Err(ReferenceResourceError::NoMedia);
    };
    // Pin the original encoded allocation and immutable document before decoding.
    let snapshot = snapshot.clone();
    let encoded = Arc::clone(image.encoded_allocation());
    // Declared before the decoder's Vec: errors/unwind destroy bytes before charge.
    let mut charge = None;
    let pixels = decode_png_pixels_with_buffers(&encoded, limits, workspace, row, |cost| {
        charge = Some(
            budget
                .reserve(cost)
                .map_err(ReferenceResourceError::Budget)?,
        );
        allocate(charge.as_mut().expect("reserved before allocation"))
            .map_err(ReferenceResourceError::Budget)
    })?;
    // No fallible work after moving pixels and charge into their lifetime carrier.
    retain(RetainedPixels {
        pixels,
        _charge: charge.expect("positive admitted PNG has a pixel allocation"),
        snapshot,
        encoded,
    })
}

#[cfg(test)]
#[path = "../tests/reference_image_resources.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/reference_png_rows.rs"]
mod workspace_tests;
