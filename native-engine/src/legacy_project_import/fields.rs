//! The single answer to "what happens to this baseline member": every member the baseline `exportJSON` / `serP` / `serR`
//! writes has exactly one disposition here, derived from the same tables the converter reads, so a member can never be
//! added to the baseline reader without a stated outcome. The field-catalogue test enumerates the baseline members
//! against this function.
use super::{classify, components, dup, holders, kinds, media, stroke, stroke_ext, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Project,
    Layer,
    Stroke,
    Raster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Disposition {
    /// Converted onto a native record or document member.
    Mapped,
    /// Derived or editor-only: recorded in the report, never a loss of drawing content.
    Ignored,
    /// Content with no native owner: any value refuses with `legacy_unsupported_field` naming the path.
    Refused,
}

pub(crate) fn disposition(scope: Scope, key: &str) -> Option<Disposition> {
    let any = |tables: &[&[&str]]| tables.iter().any(|t| t.contains(&key));
    match scope {
        Scope::Project => {
            if any(&[classify::PROJECT_MAPPED]) {
                Some(Disposition::Mapped)
            } else if any(&[classify::PROJECT_PREFERENCES]) {
                Some(Disposition::Ignored)
            } else if any(&[classify::PROJECT_LATER]) {
                Some(Disposition::Refused)
            } else {
                None
            }
        }
        Scope::Layer => {
            if key == "isTextLayer" || key == "footage" {
                Some(Disposition::Ignored)
            } else if any(&[
                classify::LAYER_MAPPED,
                classify::LAYER_PASS,
                kinds::MAPPED,
                holders::MAPPED,
                dup::MAPPED,
                components::LAYER_MAPPED,
            ]) || key == "groups"
            {
                Some(Disposition::Mapped)
            } else if any(&[classify::LAYER_LATER]) {
                Some(Disposition::Refused)
            } else {
                None
            }
        }
        Scope::Stroke => {
            if any(&[
                stroke::MAPPED,
                stroke::BRUSH,
                stroke_ext::MAPPED,
                text::MAPPED,
            ]) || key == "groupId"
            {
                Some(Disposition::Mapped)
            } else if stroke::WARNED.iter().any(|w| w.0 == key) {
                Some(Disposition::Ignored)
            } else if any(&[stroke::LATER]) {
                Some(Disposition::Refused)
            } else {
                None
            }
        }
        Scope::Raster => {
            if any(&[media::RASTER_MAPPED]) {
                Some(Disposition::Mapped)
            } else if media::RASTER_TEXT.contains(&key) {
                Some(Disposition::Ignored)
            } else if key == "linkedHandleId" || key == "groupId" {
                Some(Disposition::Refused)
            } else {
                None
            }
        }
    }
}
