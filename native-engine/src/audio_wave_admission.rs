//! Bounded RIFF/WAVE encoded-source admission; no decoder, player or budget.
//!
//! Subset: little-endian RIFF/WAVE, one fmt before one data, PCM 8/16/24/32
//! or IEEE float 32, mono/stereo, 16-byte legacy or 18-byte zero-extension fmt.
//! Other format tags, float64, multichannel layouts and extended fmt are
//! explicitly unsupported. Metadata chunks are skipped as opaque RIFF chunks.
//! A caller's existing retention owner must reserve capacity before allocation;
//! AdmittedWave only retains the exact validated allocation, without copying.
//!
//! Contracts: IBM/Microsoft Multimedia Programming Interface and Data
//! Specifications 1.0 (1991), sections Chunks and Waveform Audio File Format;
//! Microsoft WAVEFORMATEX and microsoft/DirectXTK Wave-Formats (see packet sources).
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaveError {
    Invalid,
    Unsupported,
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WaveMetadata {
    pub(crate) channels: u16,
    pub(crate) sample_rate: u32,
    pub(crate) bits_per_sample: u16,
    pub(crate) block_align: u16,
    pub(crate) frame_count: u64,
    pub(crate) duration_seconds: f64,
}

/// Private fields seal allocation and metadata together after validation.
#[derive(Debug, Clone)]
pub(crate) struct AdmittedWave {
    allocation: Arc<Vec<u8>>,
    metadata: WaveMetadata,
}

impl AdmittedWave {
    pub(crate) fn allocation(&self) -> &Arc<Vec<u8>> {
        &self.allocation
    }

    pub(crate) fn metadata(&self) -> WaveMetadata {
        self.metadata
    }
}

/// Capacity, not just used length, is the retained charge's admission bound.
/// This value neither reserves nor releases another owner's resource budget.
pub(crate) fn admit_wave(
    allocation: Arc<Vec<u8>>,
    capacity_limit: usize,
) -> Result<AdmittedWave, WaveError> {
    if capacity_limit == 0 || allocation.capacity() > capacity_limit {
        return Err(WaveError::Limit);
    }
    let metadata = validate_wave(allocation.as_slice(), capacity_limit)?;
    Ok(AdmittedWave {
        allocation,
        metadata,
    })
}

fn word(bytes: &[u8], at: usize) -> Result<u16, WaveError> {
    let end = at.checked_add(2).ok_or(WaveError::Invalid)?;
    let value = bytes.get(at..end).ok_or(WaveError::Invalid)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn dword(bytes: &[u8], at: usize) -> Result<u32, WaveError> {
    let end = at.checked_add(4).ok_or(WaveError::Invalid)?;
    let value = bytes.get(at..end).ok_or(WaveError::Invalid)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[derive(Clone, Copy)]
struct Format {
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    block_align: u16,
}

fn format(bytes: &[u8]) -> Result<Format, WaveError> {
    if bytes.len() < 16 {
        return Err(WaveError::Invalid);
    }
    let tag = word(bytes, 0)?;
    if !matches!(tag, 1 | 3) {
        return Err(WaveError::Unsupported);
    }
    if bytes.len() != 16 {
        let extension = usize::from(word(bytes, 16)?);
        if extension.checked_add(18) != Some(bytes.len()) {
            return Err(WaveError::Invalid);
        }
        if extension != 0 {
            return Err(WaveError::Unsupported);
        }
    }
    let channels = word(bytes, 2)?;
    let sample_rate = dword(bytes, 4)?;
    let byte_rate = dword(bytes, 8)?;
    let block_align = word(bytes, 12)?;
    let bits_per_sample = word(bytes, 14)?;
    if channels == 0 || sample_rate == 0 {
        return Err(WaveError::Invalid);
    }
    // Legacy tags do not carry a channel mask. Do not invent its layout.
    if channels > 2 {
        return Err(WaveError::Unsupported);
    }
    if !matches!((tag, bits_per_sample), (1, 8 | 16 | 24 | 32) | (3, 32)) {
        return Err(WaveError::Unsupported);
    }
    let expected_align = channels
        .checked_mul(bits_per_sample / 8)
        .ok_or(WaveError::Invalid)?;
    if block_align == 0 || block_align != expected_align {
        return Err(WaveError::Invalid);
    }
    let expected_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or(WaveError::Invalid)?;
    if byte_rate != expected_rate {
        return Err(WaveError::Invalid);
    }
    Ok(Format {
        channels,
        sample_rate,
        bits_per_sample,
        block_align,
    })
}

/// Validates the complete supplied container, including every top-level chunk
/// boundary and zero odd-byte pad. It never guesses a format from MIME/magic.
/// Zero-frame data is structurally valid (duration zero); this does not prove
/// that a later AudioTrack trim or playback operation accepts an empty window.
pub(crate) fn validate_wave(bytes: &[u8], byte_limit: usize) -> Result<WaveMetadata, WaveError> {
    if byte_limit == 0 || bytes.len() > byte_limit {
        return Err(WaveError::Limit);
    }
    if bytes.len() < 12 {
        return Err(WaveError::Invalid);
    }
    match &bytes[..4] {
        b"RIFF" => {}
        b"RIFX" | b"RF64" | b"BW64" => return Err(WaveError::Unsupported),
        _ => return Err(WaveError::Invalid),
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WaveError::Unsupported);
    }
    let declared = usize::try_from(dword(bytes, 4)?).map_err(|_| WaveError::Limit)?;
    if declared.checked_add(8) != Some(bytes.len()) {
        return Err(WaveError::Invalid);
    }
    let mut at = 12usize;
    let mut parsed = None;
    let mut data_length = None;
    while at < bytes.len() {
        let body_start = at.checked_add(8).ok_or(WaveError::Invalid)?;
        let header = bytes.get(at..body_start).ok_or(WaveError::Invalid)?;
        let length = usize::try_from(dword(header, 4)?).map_err(|_| WaveError::Limit)?;
        let body_end = body_start.checked_add(length).ok_or(WaveError::Invalid)?;
        let body = bytes.get(body_start..body_end).ok_or(WaveError::Invalid)?;
        let next = body_end.checked_add(length & 1).ok_or(WaveError::Invalid)?;
        if next > bytes.len() || (length & 1 == 1 && bytes[body_end] != 0) {
            return Err(WaveError::Invalid);
        }
        match &header[..4] {
            b"fmt " => {
                if parsed.is_some() {
                    return Err(WaveError::Invalid);
                }
                parsed = Some(format(body)?);
            }
            b"data" => {
                if parsed.is_none() || data_length.is_some() {
                    return Err(WaveError::Invalid);
                }
                data_length = Some(length);
            }
            // A segmented wavl source is a different timing contract. Opaque
            // ancillary LIST payloads otherwise remain retained but uninterpreted.
            b"LIST" if body.starts_with(b"wavl") => return Err(WaveError::Unsupported),
            _ => {}
        }
        at = next;
    }
    let fmt = parsed.ok_or(WaveError::Invalid)?;
    let data_length = data_length.ok_or(WaveError::Invalid)?;
    if data_length % usize::from(fmt.block_align) != 0 {
        return Err(WaveError::Invalid);
    }
    let frame_count =
        u64::try_from(data_length / usize::from(fmt.block_align)).map_err(|_| WaveError::Limit)?;
    let duration_seconds = frame_count as f64 / f64::from(fmt.sample_rate);
    if !duration_seconds.is_finite() {
        return Err(WaveError::Invalid);
    }
    Ok(WaveMetadata {
        channels: fmt.channels,
        sample_rate: fmt.sample_rate,
        bits_per_sample: fmt.bits_per_sample,
        block_align: fmt.block_align,
        frame_count,
        duration_seconds,
    })
}

#[cfg(test)]
#[path = "../tests/audio_wave_admission.rs"]
mod tests;
