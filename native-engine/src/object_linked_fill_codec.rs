//! Admission of original two-geometry pressure-primary/backdrop pairs; never manufacture a path.
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, VECTOR_FAMILY};
fn invalid() -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, "invalid linked fill pair")
}
pub(super) fn validate_record(record: &ObjectRecord) -> Result<(), ObjectCodecError> {
    let Some(ext) = record.vector_ext.value() else {
        return Ok(());
    };
    let companion = ext.is_linked_fill_companion.value() == Some(&true);
    let Some(link) = ext.linked_fill_id.value() else {
        return if companion { Err(invalid()) } else { Ok(()) };
    };
    if link.is_empty()
        || link.len() > 128
        || link.chars().any(char::is_control)
        || record.target.frame_scope.kind != FrameScopeKind::Authored
        || record.family != VECTOR_FAMILY
        || !ext.bitmap_brush.missing()
    {
        return Err(invalid());
    }
    if companion {
        if !ext.pressure.missing() || record.solid_fill().is_none() {
            return Err(invalid());
        }
    } else if ext
        .pressure
        .value()
        .and_then(|p| p.center_segments.value())
        .is_none_or(|p| p.len() < 2)
    {
        return Err(invalid());
    }
    Ok(())
}
fn validate_list(records: &[ObjectRecord]) -> Result<(), ObjectCodecError> {
    for record in records {
        validate_record(record)?;
        let Some(ext) = record.vector_ext.value() else {
            continue;
        };
        let Some(link) = ext.linked_fill_id.value() else {
            continue;
        };
        let same_cell = |other: &&ObjectRecord| {
            other.target.context_id == record.target.context_id
                && other.target.layer_uid == record.target.layer_uid
                && other.target.frame_scope == record.target.frame_scope
        };
        let members: Vec<_> = records
            .iter()
            .enumerate()
            .filter(|(_, other)| same_cell(other))
            .filter(|(_, other)| {
                other
                    .vector_ext
                    .value()
                    .and_then(|x| x.linked_fill_id.value())
                    == Some(link)
            })
            .collect();
        if members.len() != 2 {
            return Err(invalid());
        }
        let companions: Vec<_> = members
            .iter()
            .filter(|(_, other)| {
                other
                    .vector_ext
                    .value()
                    .and_then(|x| x.is_linked_fill_companion.value())
                    == Some(&true)
            })
            .collect();
        if companions.len() != 1 {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(crate) fn validate_linked_pairs(document: &ObjectDocument) -> Result<(), ObjectCodecError> {
    validate_list(&document.objects)?;
    if let Some(set) = document.component_set() {
        for component in &set.components {
            validate_list(&component.objects)?;
        }
    }
    Ok(())
}
