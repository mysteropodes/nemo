//! Asset-bearing clipboard packets resolve against the existing retained revision store.
//! No client-supplied identity is resource proof, and no bytes are decoded or charged here.
use crate::object_document::object_group_kernels::object_clipboard_plan::{
    copy_selection, ClipPayload, ClipSource,
};
use crate::object_document::Presence;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectRevisionOwner;
use std::sync::Arc;

pub(super) fn stamp(
    clip: &mut Option<ClipPayload>,
    origin: &ObjectSnapshot,
    stroke_ids: &[String],
) {
    if let Some(clip) = clip {
        if !clip.rasters.missing() {
            clip.source = Presence::Present(ClipSource {
                instance_id: origin.instance_id().to_owned(),
                document_id: origin.document_id().to_owned(),
                content_revision: origin.content_revision(),
                stroke_ids: stroke_ids.to_vec(),
            });
        }
    }
}

pub(super) fn verify(
    clip: &ClipPayload,
    current: &ObjectSnapshot,
    revisions: &ObjectRevisionOwner,
) -> Result<(), DispatchErrorCode> {
    let invalid = DispatchErrorCode::InvalidRequest;
    let Some(rasters) = clip.rasters.value() else {
        return if clip.source.missing() {
            Ok(())
        } else {
            Err(invalid)
        };
    };
    if rasters.is_empty() {
        return Err(invalid);
    }
    let source = clip.source.value().ok_or(invalid)?;
    if source.instance_id != current.instance_id() {
        return Err(DispatchErrorCode::WrongInstance);
    }
    if source.document_id != current.document_id() {
        return Err(DispatchErrorCode::WrongDocument);
    }
    let retained = revisions
        .acquire(source.content_revision)
        .ok_or(DispatchErrorCode::StaleRevision)?;
    let mut expected = copy_selection(
        retained.document(),
        &clip.context_id,
        &clip.layer_uid,
        clip.frame,
        &source.stroke_ids,
    )
    .map_err(|_| invalid)?;
    stamp(&mut expected, &retained, &source.stroke_ids);
    // Includes exact authored geometry, effects, group table and every companion.
    if expected.as_ref() != Some(clip) {
        return Err(invalid);
    }
    let before = retained
        .project()
        .and_then(|p| p.asset_inventory())
        .ok_or(invalid)?;
    let after = current
        .project()
        .and_then(|p| p.asset_inventory())
        .ok_or(invalid)?;
    for entry in rasters {
        let old = before.get(entry.item.asset_id()).ok_or(invalid)?;
        let next = after.get(entry.item.asset_id()).ok_or(invalid)?;
        let old_bytes = old.source().encoded_allocation().ok_or(invalid)?;
        let next_bytes = next.source().encoded_allocation().ok_or(invalid)?;
        if old.kind() != next.kind()
            || old.source().media_type() != next.source().media_type()
            || !Arc::ptr_eq(old_bytes, next_bytes)
        {
            return Err(invalid);
        }
    }
    Ok(())
}
