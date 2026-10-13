//! Private CPU PNG admission; no document, file, decoder or GPU authority.
use png::{BitDepth, ColorType, DecodeOptions, Decoder, DecodingError, Limits};
use std::io::{BufReader, Cursor};
use std::sync::Arc;

const MAX_ENCODED: usize = 16 * 1024 * 1024;
const MAX_RGBA: usize = 256 * 1024 * 1024;
const MAX_DECODER: usize = 64 * 1024 * 1024;
const MAX_DIMENSION: u32 = 8192;

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
            || rgba_bytes > MAX_RGBA
            || decoder_bytes == 0
            || decoder_bytes > MAX_DECODER
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
    encoded: Arc<Vec<u8>>,
    rgba: Arc<Vec<u8>>,
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

#[derive(Clone, Copy)]
struct PngRows {
    width: u32,
    height: u32,
    color: ColorType,
    row_size: usize,
    stride: usize,
    rgba_size: usize,
}
// Both consumers use the existing header, admitted-row and terminal checks.
fn read_png_rows<E: From<ImageAssetError>>(
    source: &[u8],
    budget: ImageAssetLimits,
    mut consume: impl FnMut(PngRows, usize, &[u8]) -> Result<(), E>,
) -> Result<PngRows, E> {
    if source.len() > budget.encoded_bytes {
        return Err(ImageAssetError::EncodedBudget.into());
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
        return Err(ImageAssetError::DimensionBudget.into());
    }
    if info.interlaced
        || info.bit_depth != BitDepth::Eight
        || !matches!(color, ColorType::Rgb | ColorType::Rgba)
    {
        return Err(ImageAssetError::Unsupported.into());
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ImageAssetError::DecodedBudget)?;
    let rgba_size = pixels
        .checked_mul(4)
        .ok_or(ImageAssetError::DecodedBudget)?;
    if rgba_size > budget.rgba_bytes {
        return Err(ImageAssetError::DecodedBudget.into());
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
        return Err(ImageAssetError::Unsupported.into());
    }
    if reader.output_buffer_size() != Some(raw_size) {
        return Err(ImageAssetError::Malformed.into());
    }
    let row_size = (width as usize)
        .checked_mul(channels)
        .ok_or(ImageAssetError::DecodedBudget)?;
    let stride = (width as usize)
        .checked_mul(4)
        .ok_or(ImageAssetError::DecodedBudget)?;
    let rows = PngRows {
        width,
        height,
        color,
        row_size,
        stride,
        rgba_size,
    };
    for index in 0..height as usize {
        let row = reader
            .next_row()
            .map_err(decoder_error)?
            .ok_or(ImageAssetError::Malformed)?;
        let data = row.data();
        if data.len() != rows.row_size {
            return Err(ImageAssetError::Malformed.into());
        }
        consume(rows, index, data)?;
    }
    if reader.next_row().map_err(decoder_error)?.is_some()
        || reader.info().width != width
        || reader.info().height != height
        || reader.info().color_type != color
        || reader.info().bit_depth != BitDepth::Eight
    {
        return Err(ImageAssetError::Malformed.into());
    }
    reader.finish().map_err(decoder_error)?;
    Ok(rows)
}

/// Borrowed decoder-check parity, without final pixel/source buffers.
/// Known decoder gap: excess inflated pixels after declared rows can be ignored.
/// Decoder-internal row/workspace allocations still obey PNG Limits.
pub(crate) fn validate_png(
    source: &[u8],
    budget: ImageAssetLimits,
) -> Result<(u32, u32), ImageAssetError> {
    let rows = read_png_rows(source, budget, |_, _, _| Ok::<(), ImageAssetError>(()))?;
    Ok((rows.width, rows.height))
}

pub(crate) fn decode_png(
    source: &[u8],
    budget: ImageAssetLimits,
) -> Result<ReferenceImageAsset, ImageAssetError> {
    let pixels = decode_png_pixels(source, budget, buffer)?;
    let mut encoded = buffer(source.len())?;
    encoded.copy_from_slice(source);
    Ok(ReferenceImageAsset {
        width: pixels.width,
        height: pixels.height,
        encoded: Arc::new(encoded),
        rgba: Arc::new(pixels.rgba),
    })
}

/// Private borrowed producer: no encoded copy and no raw-pixel ownership port.
#[derive(Debug)]
pub(crate) struct DecodedPngPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    #[cfg(test)]
    release_probe: Option<PixelReleaseProbe>,
}
#[cfg(test)]
struct PixelReleaseProbe(Option<Box<dyn FnOnce() + Send + Sync>>);
#[cfg(test)]
impl std::fmt::Debug for PixelReleaseProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("release probe")
    }
}
#[cfg(test)]
impl Drop for PixelReleaseProbe {
    fn drop(&mut self) {
        if let Some(probe) = self.0.take() {
            probe();
        }
    }
}
impl DecodedPngPixels {
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub(crate) fn row_stride(&self) -> usize {
        self.width as usize * 4
    }
    pub(crate) fn rgba8(&self) -> &[u8] {
        &self.rgba
    }
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.rgba.capacity()
    }
    // Runs after the actual Vec field is destroyed, before the carrier's charge.
    #[cfg(test)]
    pub(crate) fn observe_release(&mut self, probe: impl FnOnce() + Send + Sync + 'static) {
        self.release_probe = Some(PixelReleaseProbe(Some(Box::new(probe))));
    }
}

pub(crate) fn decode_png_pixels<E: From<ImageAssetError>>(
    source: &[u8],
    budget: ImageAssetLimits,
    allocate: impl FnOnce(usize) -> Result<Vec<u8>, E>,
) -> Result<DecodedPngPixels, E> {
    let mut allocate = Some(allocate);
    let mut rgba = Vec::new();
    let rows = read_png_rows(source, budget, |rows, index, data| {
        if let Some(allocate) = allocate.take() {
            rgba = allocate(rows.rgba_size)?;
            if rgba.len() != rows.rgba_size {
                return Err(ImageAssetError::Allocation.into());
            }
        }
        let start = index * rows.stride;
        let target = &mut rgba[start..start + rows.stride];
        if rows.color == ColorType::Rgb {
            for (rgb, pixel) in data.chunks_exact(3).zip(target.chunks_exact_mut(4)) {
                pixel[..3].copy_from_slice(rgb);
                pixel[3] = 255;
            }
        } else {
            target.copy_from_slice(data);
        }
        Ok::<(), E>(())
    })?;
    Ok(DecodedPngPixels {
        width: rows.width,
        height: rows.height,
        rgba,
        #[cfg(test)]
        release_probe: None,
    })
}
