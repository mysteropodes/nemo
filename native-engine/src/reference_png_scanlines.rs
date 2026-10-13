//! Strict filtered-stream length only. The existing PNG reader owns CRC/pixels.
//! Caller scratch is fixed; inflater/backend workspace is not a quota claim.
use crate::reference_image_asset::ImageAssetError;
use flate2::{Decompress, FlushDecompress, Status};

const SCRATCH: usize = 8192;
struct Scanlines {
    inflater: Decompress,
    expected: usize,
    done: bool,
}
impl Scanlines {
    fn new(row_bytes: usize, height: usize) -> Result<Self, ImageAssetError> {
        let expected = row_bytes
            .checked_add(1)
            .and_then(|row| row.checked_mul(height))
            .filter(|_| row_bytes > 0 && height > 0)
            .ok_or(ImageAssetError::DecodedBudget)?;
        Ok(Self {
            inflater: Decompress::new(true),
            expected,
            done: false,
        })
    }
    fn feed(
        &mut self,
        mut input: &[u8],
        scratch: &mut [u8; SCRATCH],
    ) -> Result<(), ImageAssetError> {
        if self.done {
            return if input.is_empty() {
                Ok(())
            } else {
                Err(ImageAssetError::Malformed)
            };
        }
        loop {
            let before_in = self.inflater.total_in();
            let before_out = self.inflater.total_out();
            let emitted = usize::try_from(before_out).map_err(|_| ImageAssetError::Malformed)?;
            let remaining = self
                .expected
                .checked_sub(emitted)
                .ok_or(ImageAssetError::Malformed)?;
            // One sentinel byte detects surplus even exactly at the declared boundary.
            let capacity = if remaining >= SCRATCH {
                SCRATCH
            } else {
                remaining + 1
            };
            let status = self
                .inflater
                .decompress(input, &mut scratch[..capacity], FlushDecompress::None)
                .map_err(|_| ImageAssetError::Malformed)?;
            let consumed = usize::try_from(
                self.inflater
                    .total_in()
                    .checked_sub(before_in)
                    .ok_or(ImageAssetError::Malformed)?,
            )
            .map_err(|_| ImageAssetError::Malformed)?;
            let produced = usize::try_from(
                self.inflater
                    .total_out()
                    .checked_sub(before_out)
                    .ok_or(ImageAssetError::Malformed)?,
            )
            .map_err(|_| ImageAssetError::Malformed)?;
            input = input.get(consumed..).ok_or(ImageAssetError::Malformed)?;
            if produced > capacity || self.inflater.total_out() > self.expected as u64 {
                return Err(ImageAssetError::Malformed);
            }
            if status == Status::StreamEnd {
                if !input.is_empty() || self.inflater.total_out() != self.expected as u64 {
                    return Err(ImageAssetError::Malformed);
                }
                self.done = true;
                return Ok(());
            }
            if consumed == 0 && produced == 0 {
                // Empty input may await the next IDAT. Nonempty stalled input refuses.
                return if input.is_empty() {
                    Ok(())
                } else {
                    Err(ImageAssetError::Malformed)
                };
            }
            if input.is_empty() && produced < capacity {
                return Ok(());
            }
        }
    }
    fn finish(mut self, scratch: &mut [u8; SCRATCH]) -> Result<(), ImageAssetError> {
        self.feed(&[], scratch)?;
        if self.done {
            Ok(())
        } else {
            Err(ImageAssetError::Malformed)
        }
    }
}

/// Rewalk borrowed chunks without concatenating IDAT or retaining inflated data.
/// The parent already admitted dimensions, subtype, encoded size and container.
pub(super) fn verify(
    source: &[u8],
    row_bytes: usize,
    height: usize,
) -> Result<(), ImageAssetError> {
    if !source.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ImageAssetError::Malformed);
    }
    let mut stream = Scanlines::new(row_bytes, height)?;
    let mut scratch = [0; SCRATCH];
    let mut offset = 8usize;
    let mut idat_phase = 0;
    loop {
        let header_end = offset.checked_add(8).ok_or(ImageAssetError::Malformed)?;
        let header = source
            .get(offset..header_end)
            .ok_or(ImageAssetError::Malformed)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let data_end = header_end
            .checked_add(length)
            .ok_or(ImageAssetError::Malformed)?;
        let chunk_end = data_end.checked_add(4).ok_or(ImageAssetError::Malformed)?;
        if chunk_end > source.len() {
            return Err(ImageAssetError::Malformed);
        }
        let kind = &header[4..];
        if kind == b"IDAT" {
            if idat_phase == 2 {
                return Err(ImageAssetError::Malformed);
            }
            idat_phase = 1;
            stream.feed(&source[header_end..data_end], &mut scratch)?;
        } else {
            if idat_phase == 1 {
                idat_phase = 2;
            }
            if kind == b"IEND" {
                if length != 0 || chunk_end != source.len() || idat_phase == 0 {
                    return Err(ImageAssetError::Malformed);
                }
                return stream.finish(&mut scratch);
            }
        }
        offset = chunk_end;
    }
}

#[cfg(test)]
#[path = "../tests/reference_png_scanlines.rs"]
mod tests;
