//! PHASE TWO PRIVATE ONLY: do not relax owner guard before working font wire.
//! pending tickets from the SAME existing owner ledger.
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
        .is_some_and(|assets| !assets.placements().is_empty() || assets.records().len() > 1024)
    {
        return Err("asset placement or catalog unavailable");
    }
    // A source's codec precondition does not establish its record's kind.
    // Keep typed entry as strict as wire admission BEFORE charging/identity.
    if project.asset_inventory().is_some_and(|assets| {
        assets.records().iter().any(|record| {
            record.source().encoded_allocation().is_some()
                && !((record.kind() == AssetKind::Image
                    && record.source().media_type() == Some("image/png")
                    && !record.source().has_font_admission())
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
