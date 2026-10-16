//! PRIVATE PHASE TWO: bounded encoded payload parsing/copy, not a quota owner.
use crate::object_codec::project::{text, ProjectWireError};
use serde_json::value::RawValue;

pub(super) fn prepare(raw: &RawValue, cap: usize) -> Result<(String, usize), ProjectWireError> {
    let chars = base64::encoded_len(cap, true).ok_or(ProjectWireError::Budget)?;
    let data = text(raw, chars)?;
    if data.is_empty() || data.len() % 4 != 0 {
        return Err(ProjectWireError::Invalid);
    }
    let padding = data.bytes().rev().take_while(|b| *b == b'=').count();
    if padding > 2 {
        return Err(ProjectWireError::Invalid);
    }
    let length = (data.len() / 4)
        .checked_mul(3)
        .and_then(|n| n.checked_sub(padding))
        .ok_or(ProjectWireError::Budget)?;
    if length == 0 || length > cap {
        return Err(ProjectWireError::Budget);
    }
    Ok((data, length))
}

/// Caller has already reserved ALL unique payload capacities in the existing
/// ledger. This callback consumes exactly one permit and makes one binary copy.
pub(super) fn decode(
    data: &str,
    length: usize,
    allocate: &mut impl FnMut(usize) -> Result<Vec<u8>, ProjectWireError>,
) -> Result<Vec<u8>, ProjectWireError> {
    let mut encoded = allocate(length)?;
    if encoded.len() != length || encoded.capacity() != length {
        return Err(ProjectWireError::Budget);
    }
    let mut reader = base64::read::DecoderReader::new(
        data.as_bytes(),
        &base64::engine::general_purpose::STANDARD,
    );
    std::io::Read::read_exact(&mut reader, &mut encoded).map_err(|_| ProjectWireError::Invalid)?;
    if std::io::Read::read(&mut reader, &mut [0]).map_err(|_| ProjectWireError::Invalid)? != 0 {
        return Err(ProjectWireError::Invalid);
    }
    Ok(encoded)
}
