//! The default PNG validity port of the admission kernel (L06-D): the existing
//! `reference_image_asset::validate_png` under the accepted admission bounds.
//!
//! It validates borrowed bytes and returns two integers; it decodes no pixels
//! and retains neither the bytes nor the validator's scratch. The scratch of
//! `validate_png` is NOT charged to the shared CPU image budget here: the owner
//! transaction (L06-H) supplies a charged port in the same shape, this default
//! exists for the kernel's own acceptance and for callers outside the budget.
use crate::object_document::object_raster_asset_kernels::object_raster_asset_admission::{
    PngCheck, MAX_DIMENSION, MAX_ENCODED_BYTES, MAX_RGBA_BYTES,
};
use crate::reference_image_asset::{validate_png, ImageAssetError, ImageAssetLimits};

/// Decoder scratch bound accepted by `object_project_admission.rs` (DECODER_BYTES).
pub(crate) const MAX_DECODER_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn map_error(error: ImageAssetError) -> PngCheck {
    match error {
        ImageAssetError::Malformed => PngCheck::Corrupt,
        ImageAssetError::Unsupported => PngCheck::Unsupported,
        ImageAssetError::EncodedBudget
        | ImageAssetError::DecodedBudget
        | ImageAssetError::DecoderBudget
        | ImageAssetError::DimensionBudget => PngCheck::TooLarge,
        ImageAssetError::InvalidLimits | ImageAssetError::Allocation => PngCheck::Unavailable,
    }
}

pub(crate) fn check_png(bytes: &[u8]) -> PngCheck {
    let Ok(limits) = ImageAssetLimits::new(
        MAX_ENCODED_BYTES,
        MAX_RGBA_BYTES,
        MAX_DECODER_BYTES,
        MAX_DIMENSION,
        MAX_DIMENSION,
    ) else {
        return PngCheck::Unavailable;
    };
    match validate_png(bytes, limits) {
        Ok((width, height)) => PngCheck::Valid { width, height },
        Err(error) => map_error(error),
    }
}
