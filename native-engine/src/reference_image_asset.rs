//! Private CPU PNG admission; no document, file, decoder or GPU authority.
use png::{BitDepth, ColorType, DecodeOptions, Decoder, DecodingError, Limits};
use std::io::{BufReader, Cursor};

const MAX_ENCODED: usize = 16 * 1024 * 1024;
const MAX_DECODED: usize = 64 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageAssetError {
    InvalidLimits,
    EncodedBudget,
    DecodedBudget,
    DecoderBudget,
    DimensionBudget,
    Unsupported,
    Malformed,
    Allocation,
}

/// Separate bounds for retained source, normalized output and decoder scratch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImageAssetLimits {
    encoded_bytes: usize,
    rgba_bytes: usize,
    decoder_bytes: usize,
    width: u32,
    height: u32,
}
impl ImageAssetLimits {
    pub(crate) fn new(
        encoded_bytes: usize,
        rgba_bytes: usize,
        decoder_bytes: usize,
        width: u32,
        height: u32,
    ) -> Result<Self, ImageAssetError> {
        if encoded_bytes == 0
            || encoded_bytes > MAX_ENCODED
            || rgba_bytes == 0
            || rgba_bytes > MAX_DECODED
            || decoder_bytes == 0
            || decoder_bytes > MAX_DECODED
            || width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
        {
            return Err(ImageAssetError::InvalidLimits);
        }
        Ok(Self {
            encoded_bytes,
            rgba_bytes,
            decoder_bytes,
            width,
            height,
        })
    }
}

/// Immutable owned source and straight-alpha RGBA8. No caller/decoder borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReferenceImageAsset {
    width: u32,
    height: u32,
    encoded: Box<[u8]>,
    rgba: Box<[u8]>,
}
impl ReferenceImageAsset {
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub(crate) fn row_stride(&self) -> usize {
        self.width as usize * 4
    }
    pub(crate) fn encoded_png(&self) -> &[u8] {
        &self.encoded
    }
    pub(crate) fn rgba8(&self) -> &[u8] {
        &self.rgba
    }
}

fn decoder_error(error: DecodingError) -> ImageAssetError {
    if matches!(error, DecodingError::LimitsExceeded) {
        ImageAssetError::DecoderBudget
    } else {
        ImageAssetError::Malformed
    }
}
fn buffer(size: usize) -> Result<Vec<u8>, ImageAssetError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| ImageAssetError::Allocation)?;
    bytes.resize(size, 0);
    Ok(bytes)
}

/// No ignored trailing data or late animation chunks, before decoder allocation.
/// The png decoder verifies critical AND ancillary CRCs and zlib checksums.
fn container(bytes: &[u8]) -> Result<(), ImageAssetError> {
    if !bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        return Err(ImageAssetError::Malformed);
    }
    let mut offset = 8usize;
    let mut first = true;
    loop {
        let header_end = offset.checked_add(8).ok_or(ImageAssetError::Malformed)?;
        let header = bytes
            .get(offset..header_end)
            .ok_or(ImageAssetError::Malformed)?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind = &header[4..];
        let end = header_end
            .checked_add(size)
            .and_then(|n| n.checked_add(4))
            .ok_or(ImageAssetError::Malformed)?;
        if end > bytes.len()
            || !kind.iter().all(u8::is_ascii_alphabetic)
            || !kind[2].is_ascii_uppercase()
            || (first && (kind != b"IHDR" || size != 13))
            || (!first && kind == b"IHDR")
        {
            return Err(ImageAssetError::Malformed);
        }
        if [b"acTL", b"fcTL", b"fdAT"].iter().any(|k| kind == *k) {
            return Err(ImageAssetError::Unsupported);
        }
        if kind == b"IEND" {
            return if size == 0 && end == bytes.len() {
                Ok(())
            } else {
                Err(ImageAssetError::Malformed)
            };
        }
        first = false;
        offset = end;
    }
}

pub(crate) fn decode_png(
    source: &[u8],
    budget: ImageAssetLimits,
) -> Result<ReferenceImageAsset, ImageAssetError> {
    if source.len() > budget.encoded_bytes {
        return Err(ImageAssetError::EncodedBudget);
    }
    container(source)?;
    let mut options = DecodeOptions::default();
    options.set_ignore_checksums(false);
    options.set_skip_ancillary_crc_failures(false);
    let mut decoder = Decoder::new_with_options(BufReader::new(Cursor::new(source)), options);
    decoder.set_limits(Limits {
        bytes: budget.decoder_bytes,
    });
    let info = decoder.read_header_info().map_err(decoder_error)?;
    let (width, height, color) = (info.width, info.height, info.color_type);
    if width == 0 || height == 0 || width > budget.width || height > budget.height {
        return Err(ImageAssetError::DimensionBudget);
    }
    if info.interlaced
        || info.bit_depth != BitDepth::Eight
        || !matches!(color, ColorType::Rgb | ColorType::Rgba)
    {
        return Err(ImageAssetError::Unsupported);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ImageAssetError::DecodedBudget)?;
    let rgba_size = pixels
        .checked_mul(4)
        .ok_or(ImageAssetError::DecodedBudget)?;
    if rgba_size > budget.rgba_bytes {
        return Err(ImageAssetError::DecodedBudget);
    }
    let channels = if color == ColorType::Rgb { 3 } else { 4 };
    let raw_size = pixels
        .checked_mul(channels)
        .ok_or(ImageAssetError::DecodedBudget)?;
    let mut reader = decoder.read_info().map_err(decoder_error)?;
    if reader.info().animation_control.is_some()
        || reader.info().frame_control.is_some()
        || reader.info().trns.is_some()
    {
        return Err(ImageAssetError::Unsupported);
    }
    if reader.output_buffer_size() != Some(raw_size) {
        return Err(ImageAssetError::Malformed);
    }
    let mut raw = buffer(raw_size)?;
    let frame = reader.next_frame(&mut raw).map_err(decoder_error)?;
    if frame.width != width
        || frame.height != height
        || frame.color_type != color
        || frame.bit_depth != BitDepth::Eight
        || frame.buffer_size() != raw_size
    {
        return Err(ImageAssetError::Malformed);
    }
    reader.finish().map_err(decoder_error)?;
    let rgba = if color == ColorType::Rgb {
        let mut normalized = buffer(rgba_size)?;
        for (rgb, rgba) in raw.chunks_exact(3).zip(normalized.chunks_exact_mut(4)) {
            rgba[..3].copy_from_slice(rgb);
            rgba[3] = 255;
        }
        normalized
    } else {
        raw
    };
    let mut encoded = buffer(source.len())?;
    encoded.copy_from_slice(source);
    Ok(ReferenceImageAsset {
        width,
        height,
        encoded: encoded.into_boxed_slice(),
        rgba: rgba.into_boxed_slice(),
    })
}
