//! Conversion followed by the one strict native admission: the converted bytes must be a project the codec accepts.
use super::{convert_omitting, Converted, LegacyImportError, OPTIONAL_GROUPS};
use crate::history::NativeObjectHistory;

fn admitted(bytes: &[u8]) -> bool {
    NativeObjectHistory::from_project_file("legacy-import-probe", bytes).is_ok()
}

/// Convert and prove the result admissible. When the codec refuses the file, optional metadata groups (palettes, guides,
/// comments, camera and similar editor data) are dropped one at a time, each with a warning, until it admits; drawing
/// content is never dropped, so a refusal that survives that names the whole project.
pub fn convert_admitted(bytes: &[u8]) -> Result<Converted, LegacyImportError> {
    let first = convert_omitting(bytes, &[])?;
    if admitted(&first.project_file) {
        return Ok(first);
    }
    for group in OPTIONAL_GROUPS {
        let alone = convert_omitting(bytes, &[*group])?;
        if admitted(&alone.project_file) {
            return Ok(alone);
        }
    }
    let mut dropped = Vec::new();
    for group in OPTIONAL_GROUPS {
        dropped.push(*group);
        let next = convert_omitting(bytes, &dropped)?;
        if admitted(&next.project_file) {
            return Ok(next);
        }
    }
    Err(LegacyImportError::unsupported(
        "$",
        "the native engine refused the converted drawing content",
    ))
}
