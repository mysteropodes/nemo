//! Pure edit rules of the image-mesh transaction (F17/F18): `setOffset`, `resetOffsets` and the
//! carry-over of a retopology result into the stored record. No owner, snapshot, history or
//! receipt is read here, and nothing mutates its input: every rule returns a new value, so a
//! refusal can never leave a half-edited mesh behind.
//!
//! Mirrors `NS.setOffset` / `NS.resetOffsets` of `src/js/image-mesh.js` at fbe4b8a3 and the
//! `serialize()` form of the result. Semantics kept on purpose:
//! - `setOffset` WRITES `[du, dv]` into slot `index` (it is not an increment) and touches no other
//!   slot; an index past the last vertex is refused (the baseline returns `false`);
//! - an offset is a separate array: the rest positions `verts` and the `outline` prefix are never
//!   read or written by an offset edit, so moving an outline vertex deforms the mask without
//!   changing the stored outline;
//! - `resetOffsets` writes one zero pair per vertex.
//!
//! Authored state is kept distinct, as in the record: a missing `offsets` field evaluates as the
//! zeros `load()` would fill in, an edit whose result is still all zero leaves it missing, equal
//! numbers keep their authored JSON spelling (so an edit that changes nothing cannot rewrite
//! bytes), and written numbers are spelled like `JSON.stringify` (`-0` is `0`, integral values
//! have no fraction).
use crate::object_document::{ImageMesh, Presence};
use crate::object_snapshot::object_image_mesh_edit_read::MeshState;
use serde_json::Number;

/// A computed value spelled like `JSON.stringify` would: integral values without a fraction, `-0` as `0`.
pub(super) fn js_number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Number::from(0i64));
    }
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Some(Number::from(value as i64));
    }
    Number::from_f64(value)
}

/// Equal values (the JavaScript numeric domain) keep their authored spelling.
pub(super) fn keep_equal(before: &Number, after: Number) -> Number {
    if before.as_f64() == after.as_f64() {
        before.clone()
    } else {
        after
    }
}

fn is_zero(number: &Number) -> bool {
    number.as_f64() == Some(0.0)
}

fn zero() -> Number {
    Number::from(0i64)
}

/// `setOffset(meshId, i, du, dv)` over the effective offsets: slot `index` becomes `[du, dv]`.
/// `None` is the baseline `false` (index outside the offsets).
pub(super) fn set_offset(
    offsets: &[[Number; 2]],
    index: usize,
    du: &Number,
    dv: &Number,
) -> Option<Vec<[Number; 2]>> {
    let old = offsets.get(index)?;
    let slot = [
        keep_equal(&old[0], du.clone()),
        keep_equal(&old[1], dv.clone()),
    ];
    let mut next = offsets.to_vec();
    next[index] = slot;
    Some(next)
}

/// `resetOffsets(meshId)`: one zero pair per vertex; a slot that is already zero keeps its spelling.
pub(super) fn reset_offsets(offsets: &[[Number; 2]]) -> Vec<[Number; 2]> {
    offsets
        .iter()
        .map(|slot| [keep_equal(&slot[0], zero()), keep_equal(&slot[1], zero())])
        .collect()
}

/// The offsets `load()` would hold: the authored array, or one zero pair per vertex.
pub(super) fn effective_offsets(mesh: &ImageMesh) -> Vec<[Number; 2]> {
    match mesh.offsets.value() {
        Some(offsets) => offsets.clone(),
        None => mesh.verts.iter().map(|_| [zero(), zero()]).collect(),
    }
}

/// Stores `offsets` in a copy of `old`. An all-zero result of a mesh that never authored offsets
/// stays missing; anything else is authored.
pub(super) fn with_offsets(old: &ImageMesh, offsets: Vec<[Number; 2]>) -> ImageMesh {
    let all_zero = offsets
        .iter()
        .all(|slot| is_zero(&slot[0]) && is_zero(&slot[1]));
    let mut next = old.clone();
    next.offsets = if old.offsets.missing() && all_zero {
        Presence::Missing
    } else {
        Presence::Present(offsets)
    };
    next
}

/// The mesh as plain numbers, the form the retopology kernel facade takes.
pub(super) fn state_of(mesh: &ImageMesh) -> MeshState {
    MeshState {
        outline: mesh.outline_points(),
        verts: mesh.rest_points(),
        tris: mesh.triangles().to_vec(),
        offsets: mesh.effective_offsets(),
        cols: mesh.effective_cols(),
        rows: mesh.effective_rows(),
    }
}

fn spelled(next: &[[f64; 2]], old: &[[Number; 2]]) -> Option<Vec<[Number; 2]>> {
    let mut out = Vec::with_capacity(next.len());
    for (index, point) in next.iter().enumerate() {
        let x = js_number(point[0])?;
        let y = js_number(point[1])?;
        out.push(match old.get(index) {
            Some(before) => [keep_equal(&before[0], x), keep_equal(&before[1], y)],
            None => [x, y],
        });
    }
    Some(out)
}

/// The stored record after a retopology: new outline, rest positions, triangles and offsets. Absent
/// `offsets`, `cols` and `rows` stay absent while their new value equals what absence evaluates
/// as. `None` when a computed number is not finite.
pub(super) fn merged(old: &ImageMesh, next: &MeshState) -> Option<ImageMesh> {
    let no_numbers: &[[Number; 2]] = &[];
    let outline = spelled(&next.outline, &old.outline)?;
    let verts = spelled(&next.verts, &old.verts)?;
    let offsets = spelled(
        &next.offsets,
        old.offsets.value().map_or(no_numbers, Vec::as_slice),
    )?;
    let draft = ImageMesh {
        outline,
        verts,
        tris: next.tris.clone(),
        offsets: old.offsets.clone(),
        cols: if old.cols.missing() && next.cols == old.effective_cols() {
            Presence::Missing
        } else {
            Presence::Present(next.cols)
        },
        rows: if old.rows.missing() && next.rows == old.effective_rows() {
            Presence::Missing
        } else {
            Presence::Present(next.rows)
        },
    };
    Some(with_offsets(&draft, offsets))
}
