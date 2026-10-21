//! PHASE TWO PRIVATE ONLY: do not relax owner guard before working font wire.
//! pending tickets from the SAME existing owner ledger.
use crate::object_document::media_timing::object_media_link_edit::{
    check_inventory, PlacementRefusal,
};
use crate::object_document::{AssetKind, ProjectEnvelope, ProjectReferenceState};
use crate::revision::object_revision::encoded_retention_budget::{
    Admission, EncodedRetentionBudget,
};

pub(super) fn pending_charges(
    project: &ProjectEnvelope,
    budget: &EncodedRetentionBudget,
) -> Result<Vec<Admission>, &'static str> {
    if project
        .asset_inventory()
        .is_some_and(|assets| assets.records().len() > 1024)
    {
        return Err("asset placement or catalog unavailable");
    }
    // Placements enter only against the document that owns their targets, and
    // BEFORE any retention ticket exists, so a refusal charges nothing.
    if let Some(assets) = project.asset_inventory() {
        check_inventory(project.drawing(), assets).map_err(PlacementRefusal::message)?;
    } else if project
        .drawing()
        .audio_tracks
        .value()
        .is_some_and(|tracks| !tracks.records().is_empty())
    {
        // No catalog is not an escape from the persistent AudioTrack source join.
        return Err(PlacementRefusal::Asset.message());
    }
    // A source's codec precondition does not establish its record's kind.
    // Keep typed entry as strict as wire admission BEFORE charging/identity.
    if project.asset_inventory().is_some_and(|assets| {
        assets.records().iter().any(|record| {
            record.source().encoded_allocation().is_some()
                && !((record.kind() == AssetKind::Image
                    && record.source().media_type() == Some("image/png")
                    && !record.source().has_font_admission())
                    || (record.kind() == AssetKind::Audio
                        && record.source().has_audio_admission()
                        && record.source().media_type() == Some("audio/wav"))
                    || (record.kind() == AssetKind::Font
                        && record.source().has_font_admission()
                        && record.source().media_type() == Some("font/ttf")))
        })
    }) {
        return Err("asset media codec unavailable");
    }
    let reference = project.reference_state();
    let mut count = usize::from(matches!(&reference, ProjectReferenceState::StillImage(_)));
    if let Some(assets) = project.asset_inventory() {
        count = count
            .checked_add(assets.encoded_allocations().count())
            .ok_or("encoded retention unavailable")?;
    }
    let mut tickets = Vec::new();
    tickets
        .try_reserve_exact(count)
        .map_err(|_| "encoded retention unavailable")?;
    if let ProjectReferenceState::StillImage(data) = &reference {
        tickets.push(
            budget
                .admit(data.encoded_allocation())
                .map_err(|_| "encoded retention unavailable")?,
        );
    }
    if let Some(assets) = project.asset_inventory() {
        for allocation in assets.encoded_allocations() {
            tickets.push(
                budget
                    .admit(allocation)
                    .map_err(|_| "encoded retention unavailable")?,
            );
        }
    }
    Ok(tickets)
}

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/project_asset_join.rs"]
mod tests;

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/project_audio_retention.rs"]
mod audio_retention_tests;

// Immutable bitmap source consistency belongs to the same asset admission
// boundary. Drawing-only owners cannot publish one half of a compound edit.
const BRUSH_EDIT_UNAVAILABLE: &str = "bitmap brush compound edit unavailable";
type BrushKey<'a> = (&'a str, &'a str, &'a str, u32, bool);
fn brush_key(target: &crate::object_document::ObjectTarget) -> BrushKey<'_> {
    (target.context_id(), target.layer_uid(), target.stroke_id(), target.frame(),
        target.frame_scope.kind == crate::object_document::FrameScopeKind::Reference)
}
fn brush_fields(record: &crate::object_document::ObjectRecord)
    -> Option<&crate::object_document::BitmapBrushFields> {
    record.vector_ext.value().and_then(|ext| ext.bitmap_brush.value())
}
fn equal_source<T: serde::Serialize>(before: &T, after: &T) -> bool {
    match (serde_json::to_vec(before), serde_json::to_vec(after)) {
        (Ok(before), Ok(after)) => before == after,
        _ => false,
    }
}
fn brush_anchors(document: &crate::object_document::ObjectDocument)
    -> Result<std::collections::BTreeMap<BrushKey<'_>, &crate::object_document::ObjectRecord>, &'static str> {
    let mut out = std::collections::BTreeMap::new();
    for record in document.objects() {
        if brush_fields(record).is_some()
            && out.insert(brush_key(record.target()), record).is_some() {
            return Err(BRUSH_EDIT_UNAVAILABLE);
        }
    }
    Ok(out)
}
fn brush_companions<'a>(
    document: &'a crate::object_document::ObjectDocument,
    groups: &std::collections::BTreeSet<BrushKey<'_>>,
) -> Result<std::collections::BTreeMap<BrushKey<'a>, serde_json::Value>, &'static str> {
    let mut out = std::collections::BTreeMap::new();
    for entry in document.item_records().map_or(&[][..], |items| items.raster_entries()) {
        let key = brush_key(&entry.target);
        let is_companion = entry.item.brush_group_id()
            .is_some_and(|group| groups.contains(&(key.0, key.1, group, key.3, key.4)));
        // A tagged standalone raster without an anchor remains an ordinary
        // raster. Matching an anchor protects even removed/false bitmap tags.
        if !is_companion { continue; }
        let mut value = serde_json::to_value(entry).map_err(|_| BRUSH_EDIT_UNAVAILABLE)?;
        // Authored ink opacity is an existing independent presentation value;
        // it does not alter PNG bytes or require a geometry/spec rebake.
        value.get_mut("item").and_then(serde_json::Value::as_object_mut)
            .ok_or(BRUSH_EDIT_UNAVAILABLE)?.remove("opacity");
        if out.insert(key, value).is_some() { return Err(BRUSH_EDIT_UNAVAILABLE); }
    }
    Ok(out)
}
/// Shared by EVERY drawing-only publication, not just the vector UI route.
/// Geometry/spec/new/delete/retarget and companion-resource edits require the
/// existing sealed project/asset transaction. No rebake, hash authority, asset
/// store or state writer is introduced here. Retained restore/undo is separate.
pub(super) fn validate_drawing_only(
    before: &crate::object_document::ObjectDocument,
    after: &crate::object_document::ObjectDocument,
) -> Result<(), &'static str> {
    let old = brush_anchors(before)?;
    let next = brush_anchors(after)?;
    if old.len() != next.len() { return Err(BRUSH_EDIT_UNAVAILABLE); }
    for (key, record) in &old {
        let Some(replacement) = next.get(key) else { return Err(BRUSH_EDIT_UNAVAILABLE); };
        if !equal_source(&record.geometry, &replacement.geometry)
            || !equal_source(&brush_fields(record), &brush_fields(replacement)) {
            return Err(BRUSH_EDIT_UNAVAILABLE);
        }
    }
    // Use BOTH snapshots' groups: removing a flag or changing a group cannot
    // disguise an old companion as an independent ordinary raster.
    let mut groups = std::collections::BTreeSet::new();
    for document in [before, after] {
        for record in document.objects() {
            if let Some(fields) = brush_fields(record) {
                let key = brush_key(record.target());
                groups.insert((key.0, key.1, fields.brush_group_id.as_str(), key.3, key.4));
            }
        }
    }
    if brush_companions(before, &groups)? != brush_companions(after, &groups)? {
        return Err(BRUSH_EDIT_UNAVAILABLE);
    }
    Ok(())
}

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_bitmap_brush_transition.rs"]
mod brush_transition_tests;
