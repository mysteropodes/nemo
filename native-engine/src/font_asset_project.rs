//! PRIVATE PHASE TWO child of font_asset_admission, never document -> codec.
//! Requires paired wire, reservation and owner source window before applying.
use crate::object_codec::font_assets::{admit_reserved_font, FontAssetError, FontAssetLimits};
use crate::object_document::AssetSource;
use std::sync::Arc;

/// Existing aggregate permits precede all binary copying. The codec borrows the
/// resulting allocation, constructs its private proof, then the one-way bridge
/// retains the SAME Arc at the document floor. It cannot issue or publish IDs.
pub(crate) fn admitted_font_source(
    encoded: Arc<Vec<u8>>,
    limits: FontAssetLimits,
) -> Result<AssetSource, FontAssetError> {
    let admitted = admit_reserved_font(encoded, limits)?;
    let media_type = admitted.media_type();
    // The bridge is a child of the codec: private validated-proof and limit
    // fields need no new public type/visibility or reverse document dependency.
    AssetSource::admitted_font(media_type, admitted.into_encoded(), limits.encoded)
        .map_err(|_| FontAssetError::Invalid)
}
