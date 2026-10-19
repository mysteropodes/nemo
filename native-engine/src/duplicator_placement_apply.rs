//! Application of one duplicator placement to one native record (L04-D, I08).
//!
//! Mirrors the per-stroke body of `applyLayerDuplicator` (`src/js/app.js` at
//! baseline `fbe4b8a3`): the placement descriptor of clone `k` becomes ONE
//! matrix around the pivot of the clone's source strokes, the matrix moves the
//! geometry, the hue shift rotates the fill, and the copy is stamped with its
//! per-copy identity: `dupIndex = k` and
//! `dupCloneId = 'dup' + (dup.seed || 0) + '_' + k` (deterministic, so it is
//! stable across re-materializations without any persisted counter).
//!
//! A copy is a DERIVED value. [`DuplicatorCopy`] is deliberately not an
//! [`ObjectRecord`]: it owns a detached record whose `target` is the SOURCE's
//! stable identity (the baseline lets every clone inherit the seed shape's
//! `strokeId`, so selection and grouping resolve back to the source), while
//! the copy's own identity is `clone_id` + `dup_index`. It never enters
//! persisted frame data and the source record is only read. The stroke paint
//! and the opacity live in the vector body, which a copy carries VERBATIM:
//! what the baseline would do to them (hue rotation, opacity factor) is NOT
//! defaulted or silently dropped, it is reported in `unavailable` (typed), and
//! the join that owns those fields (L01 G14) must supply it.

use crate::evaluation::object_component_kernels::duplicator_placement_color::{
    rotate_fill_hue, FillHueError,
};
use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::{
    number_to_string, truthy,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    matrix_from_descriptor, transform_geometry, Descriptor, MatrixError, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::object_document::{ObjectRecord, Presence};

/// `place.dup3D`: the per-clone 3D delta, only present when an axis moved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dup3d {
    pub dz: f64,
    pub drx: f64,
    pub dry: f64,
}

/// The placement descriptor of clone `k` (`_duplicatorClonePlacement`'s result).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacementApply {
    pub matrix: Descriptor,
    pub opacity_factor: f64,
    pub hue_deg: f64,
    pub dup_3d: Option<Dup3d>,
}

/// A baseline effect on a field the native record format does not carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnavailableField {
    /// `sd.opacity * opacityFactor` (any factor but exactly 1).
    Opacity,
    /// `strokeColor` hue rotation (any truthy hue).
    StrokePaint,
}

/// Why a copy could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApplyError {
    Geometry(MatrixError),
    Fill(FillHueError),
}

/// One synthetic copy, detached from the document.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DuplicatorCopy {
    /// `'dup' + (dup.seed || 0) + '_' + k`.
    pub clone_id: String,
    /// `k`.
    pub dup_index: u32,
    /// The moved geometry and rotated fill; `target` is the source's.
    pub record: ObjectRecord,
    pub dup_3d: Option<Dup3d>,
    /// What the baseline applied that this record format cannot hold.
    pub unavailable: Vec<UnavailableField>,
}

/// `'dup' + (dup.seed || 0) + '_' + k`.
pub(crate) fn dup_clone_id(seed: JsNumber, k: u32) -> String {
    format!("dup{}_{}", number_to_string(seed.or_default(0.0)), k)
}

/// The copy of `source` at clone index `k` under `place`, around `pivot`.
pub(crate) fn apply_placement(
    source: &ObjectRecord,
    k: u32,
    seed: JsNumber,
    place: &PlacementApply,
    pivot: Pivot,
) -> Result<DuplicatorCopy, ApplyError> {
    let matrix = matrix_from_descriptor(&place.matrix, pivot);
    let geometry = transform_geometry(&matrix, &source.geometry).map_err(ApplyError::Geometry)?;
    // Only a solid fill is rotated; an explicit null (no fill) or an absent slot passes through
    // exactly as the source holds it (the baseline's `hexHueRotate` leaves null/undefined alone).
    let fill = match source.solid_fill() {
        Some(solid) => Presence::Present(Some(
            rotate_fill_hue(solid, place.hue_deg).map_err(ApplyError::Fill)?,
        )),
        None => source.fill.clone(),
    };
    let mut unavailable = Vec::new();
    if place.opacity_factor != 1.0 {
        unavailable.push(UnavailableField::Opacity);
    }
    if truthy(place.hue_deg) {
        unavailable.push(UnavailableField::StrokePaint);
    }
    Ok(DuplicatorCopy {
        clone_id: dup_clone_id(seed, k),
        dup_index: k,
        record: ObjectRecord {
            schema_version: source.schema_version,
            family: source.family.clone(),
            target: source.target.clone(),
            geometry,
            fill,
            // The vector body and its extension ride along verbatim, so a copy of an
            // `editable-vector-path` source stays a valid record of that family; for the legacy
            // family both are absent on the source and so on the copy. Applying the baseline's
            // stroke-paint hue and opacity factor to them is still reported in `unavailable`.
            vector: source.vector.clone(),
            vector_ext: source.vector_ext.clone(),
            effects: source.effects.clone(),
        },
        dup_3d: place.dup_3d,
        unavailable,
    })
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_apply_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_apply.rs"]
mod tests;
