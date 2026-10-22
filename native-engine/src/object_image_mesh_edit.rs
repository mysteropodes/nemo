//! Sealed image-mesh edit transaction (F17/F18) through the existing revision owner: `setOffset`,
//! `resetOffsets`, `setOutline` and the density control (`setDensity`) of `src/js/image-mesh.js` at
//! fbe4b8a3. Nothing here is a second writer, a mesh store or a topology implementation.
//!
//! One typed preparation reads the pinned current snapshot, validates the complete request and the
//! target mesh, builds the proposed document on a detached copy and seals it to its origin
//! (instance, document, snapshot, content revision). The commit is the existing sealed owner
//! publish, atomic with history, request replay and cancellation in the shared dispatcher. A
//! refusal, a stale or foreign pin, a malformed request or a cancelled one publishes nothing.
//!
//! - `setOffset {index, du, dv}` writes the pair into one slot of the separate `offsets` array
//!   (rest `verts` and the `outline` prefix are never touched); an index past the last vertex is a
//!   typed refusal. An offset equal to the stored one, and a `resetOffsets` over offsets that are
//!   already zero, change nothing: no revision, no history entry, redo kept, bytes kept.
//! - `setOutline {outline, cols?, rows?}` and `setDensity {cols, rows}` retopologize through the
//!   topology kernel (reached only via the read-queries facade; without the `evaluation` feature the
//!   request is a typed `unavailable`): the outline stays the stable vertex prefix and the sculpt is
//!   carried by `resampleOffsets`.
//!
//! The mesh is addressed by its store id. A mesh that no raster references is a normal store entry and
//! stays editable; the raster binding, `releaseIfUnused` and the animated per-vertex rows belong to
//! later leaves.
//!
//! Deliberate differences from the baseline, each pinned by a test: every number must be finite (an
//! offset at most 1e9 in magnitude, an outline coordinate at most 4), the outline has 3..=512 points
//! and a perimeter of at most 16 normalized units (the new-point count of a retopology is bounded by
//! the request and a density change also by the stored outline; the stored vertex and triangle
//! counts are not capped here, and the baseline has no bound at all), `cols`/`rows` must be integers in 1..=64 because that is what a record can
//! hold (the baseline ignores a falsy value and keeps a fraction), an index must be a non-negative
//! integer, and fewer than three outline points is `invalid_request` instead of `null`.
use crate::commands::{ObjectFillOwner, MAX_VECTOR_REQUEST_BYTES};
use crate::object_document::{
    object_deserialize, ImageMeshStore, MeshIdSequence, ObjectDocument, Presence,
};
use crate::object_snapshot::object_image_mesh_edit_read::{
    retopologize, Retopology, RetopologyRefusal,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[path = "object_image_mesh_binding_edit.rs"]
mod object_image_mesh_binding_edit;
#[path = "object_image_mesh_edit_rules.rs"]
mod rules;

/// An offset component beyond this keeps every product of the scene mapping finite.
const MAX_OFFSET_MAGNITUDE: f64 = 1.0e9;
/// Outline input bounds: points, absolute coordinate and closed perimeter (normalized units).
const MAX_OUTLINE_POINTS: usize = 512;
const MAX_OUTLINE_COORDINATE: f64 = 4.0;
const MAX_OUTLINE_PERIMETER: f64 = 16.0;
/// A stored outline longer than this is refused for a density retopology (cost bound).
const MAX_STORED_OUTLINE_POINTS: usize = 4096;
const MAX_MESH_ID_BYTES: usize = 128;
const MAX_DENSITY: f64 = 64.0;

#[derive(Debug)]
struct ActionPayload {
    kind: String,
    index: Presence<Number>,
    du: Presence<Number>,
    dv: Presence<Number>,
    outline: Presence<Vec<[Number; 2]>>,
    cols: Presence<Number>,
    rows: Presence<Number>,
}
object_deserialize!(ActionPayload {
    kind: String,
    #[serde(default)] index: Presence<Number>,
    #[serde(default)] du: Presence<Number>,
    #[serde(default)] dv: Presence<Number>,
    #[serde(default)] outline: Presence<Vec<[Number; 2]>>,
    #[serde(default)] cols: Presence<Number>,
    #[serde(default)] rows: Presence<Number>
});

#[derive(Debug)]
struct EditPayload {
    command: String,
    mesh_id: String,
    action: ActionPayload,
}
object_deserialize!(EditPayload {
    command: String,
    mesh_id: String,
    action: ActionPayload
});

/// A validated action: nothing in it can fail again except the kernel and the target lookup.
#[derive(Debug)]
enum MeshAction {
    SetOffset {
        index: usize,
        du: Number,
        dv: Number,
    },
    ResetOffsets,
    Retopology(Retopology),
}

/// The origin and proposed document cannot be forged by callers.
#[derive(Debug, Clone)]
pub(crate) struct PreparedImageMeshEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
    binding_sequence: Option<(MeshIdSequence, MeshIdSequence)>,
}

fn bad<T>() -> Result<T, DispatchErrorCode> {
    Err(DispatchErrorCode::InvalidRequest)
}

fn finite(number: &Number, bound: f64) -> Result<f64, DispatchErrorCode> {
    match number.as_f64() {
        Some(value) if value.is_finite() && value.abs() <= bound => Ok(value),
        _ => bad(),
    }
}

/// An integer in `low..=high` (a JSON `4.0` is the same JavaScript number as `4`).
fn whole(number: &Number, low: f64, high: f64) -> Result<u32, DispatchErrorCode> {
    match number.as_f64() {
        Some(value) if value.fract() == 0.0 && (low..=high).contains(&value) => Ok(value as u32),
        _ => bad(),
    }
}

fn density(number: &Presence<Number>) -> Result<Option<u32>, DispatchErrorCode> {
    match number.value() {
        None => Ok(None),
        Some(number) => whole(number, 1.0, MAX_DENSITY).map(Some),
    }
}

/// Exactly the members `required` plus any of `optional`; any other member present is refused.
fn members(
    action: &ActionPayload,
    required: &[&str],
    optional: &[&str],
) -> Result<(), DispatchErrorCode> {
    let present = [
        ("index", !action.index.missing()),
        ("du", !action.du.missing()),
        ("dv", !action.dv.missing()),
        ("outline", !action.outline.missing()),
        ("cols", !action.cols.missing()),
        ("rows", !action.rows.missing()),
    ];
    for (name, there) in present {
        let wanted = required.contains(&name);
        if there != wanted && !(there && optional.contains(&name)) {
            return bad();
        }
    }
    Ok(())
}

fn outline_of(raw: &[[Number; 2]]) -> Result<Vec<[f64; 2]>, DispatchErrorCode> {
    if raw.len() < 3 || raw.len() > MAX_OUTLINE_POINTS {
        return bad();
    }
    let mut points = Vec::with_capacity(raw.len());
    for pair in raw {
        points.push([
            finite(&pair[0], MAX_OUTLINE_COORDINATE)?,
            finite(&pair[1], MAX_OUTLINE_COORDINATE)?,
        ]);
    }
    let mut perimeter = 0.0;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        perimeter += (next[0] - point[0]).hypot(next[1] - point[1]);
    }
    if perimeter > MAX_OUTLINE_PERIMETER {
        return bad();
    }
    Ok(points)
}

fn parse_action(action: &ActionPayload) -> Result<MeshAction, DispatchErrorCode> {
    match action.kind.as_str() {
        "setOffset" => {
            members(action, &["index", "du", "dv"], &[])?;
            let (Some(index), Some(du), Some(dv)) =
                (action.index.value(), action.du.value(), action.dv.value())
            else {
                return bad();
            };
            let du = finite(du, MAX_OFFSET_MAGNITUDE)?;
            let dv = finite(dv, MAX_OFFSET_MAGNITUDE)?;
            let spelled =
                |value: f64| rules::js_number(value).ok_or(DispatchErrorCode::InvalidRequest);
            Ok(MeshAction::SetOffset {
                index: whole(index, 0.0, f64::from(u32::MAX))? as usize,
                du: spelled(du)?,
                dv: spelled(dv)?,
            })
        }
        "resetOffsets" => {
            members(action, &[], &[])?;
            Ok(MeshAction::ResetOffsets)
        }
        "setOutline" => {
            members(action, &["outline"], &["cols", "rows"])?;
            let Some(raw) = action.outline.value() else {
                return bad();
            };
            Ok(MeshAction::Retopology(Retopology::Outline {
                poly: outline_of(raw)?,
                cols: density(&action.cols)?,
                rows: density(&action.rows)?,
            }))
        }
        "setDensity" => {
            members(action, &["cols", "rows"], &[])?;
            match (density(&action.cols)?, density(&action.rows)?) {
                (Some(cols), Some(rows)) => {
                    Ok(MeshAction::Retopology(Retopology::Density { cols, rows }))
                }
                _ => bad(),
            }
        }
        _ => bad(),
    }
}

fn refusal_code(refusal: RetopologyRefusal) -> DispatchErrorCode {
    match refusal {
        RetopologyRefusal::KernelUnavailable => DispatchErrorCode::Unavailable,
        RetopologyRefusal::TooFewPoints
        | RetopologyRefusal::Densify
        | RetopologyRefusal::Triangulation
        | RetopologyRefusal::DensityNotRecordable => DispatchErrorCode::InvalidRequest,
    }
}

impl ObjectFillOwner {
    /// Strict `object.image.mesh.edit` preparation from this owner's current immutable snapshot.
    pub(crate) fn prepare_image_mesh_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedImageMeshEdit, DispatchErrorCode> {
        if bytes.len() > MAX_VECTOR_REQUEST_BYTES {
            return bad();
        }
        let shape: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if shape
            .get("action")
            .and_then(|action| action.get("kind"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(object_image_mesh_binding_edit::binding_action_kind)
        {
            let origin = self.current_snapshot();
            let plan = object_image_mesh_binding_edit::plan_binding_edit_json(
                &origin,
                self.mesh_id_sequence,
                bytes,
            )?;
            let changed = plan.changed();
            let before_sequence = plan.origin_sequence();
            let (document, after_sequence, _) = plan.into_parts();
            return Ok(PreparedImageMeshEdit {
                origin,
                document,
                changed,
                binding_sequence: Some((before_sequence, after_sequence)),
            });
        }
        let payload: EditPayload = match serde_json::from_slice(bytes) {
            Ok(payload) => payload,
            Err(_) => return bad(),
        };
        if payload.command != "object.image.mesh.edit"
            || payload.mesh_id.is_empty()
            || payload.mesh_id.len() > MAX_MESH_ID_BYTES
        {
            return bad();
        }
        let action = parse_action(&payload.action)?;
        let origin = self.current_snapshot();
        let store = origin
            .document()
            .image_meshes()
            .ok_or(DispatchErrorCode::NotFound)?;
        let old = store
            .get(&payload.mesh_id)
            .ok_or(DispatchErrorCode::NotFound)?;
        let next = match action {
            MeshAction::SetOffset { index, du, dv } => {
                let offsets = rules::set_offset(&rules::effective_offsets(old), index, &du, &dv)
                    .ok_or(DispatchErrorCode::InvalidRequest)?;
                rules::with_offsets(old, offsets)
            }
            MeshAction::ResetOffsets => {
                rules::with_offsets(old, rules::reset_offsets(&rules::effective_offsets(old)))
            }
            MeshAction::Retopology(change) => {
                if matches!(change, Retopology::Density { .. })
                    && old.outline.len() > MAX_STORED_OUTLINE_POINTS
                {
                    return bad();
                }
                let rebuilt = retopologize(&rules::state_of(old), &change).map_err(refusal_code)?;
                rules::merged(old, &rebuilt).ok_or(DispatchErrorCode::InvalidRequest)?
            }
        };
        let mut document = origin.document().clone();
        if &next != old {
            let entries = store
                .entries()
                .iter()
                .map(|(id, mesh)| {
                    if *id == payload.mesh_id {
                        (id.clone(), next.clone())
                    } else {
                        (id.clone(), mesh.clone())
                    }
                })
                .collect();
            let replaced =
                ImageMeshStore::try_new(entries).map_err(|_| DispatchErrorCode::InvalidRequest)?;
            document.image_meshes = Presence::Present(replaced);
            ObjectSnapshot::validate_drawing(&document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        let changed = &document != origin.document();
        Ok(PreparedImageMeshEdit {
            origin,
            document,
            changed,
            binding_sequence: None,
        })
    }

    /// Publishes the candidate only on the owner, document, snapshot and revision it was prepared from.
    pub(crate) fn commit_image_mesh_edit(
        &mut self,
        candidate: PreparedImageMeshEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
            || candidate
                .binding_sequence
                .is_some_and(|(before, _)| before != self.mesh_id_sequence)
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let revision = if candidate.changed {
            self.revisions
                .publish(candidate.document)
                .map_err(|_| DispatchErrorCode::Unavailable)?
        } else {
            current.content_revision()
        };
        // Publish before advancing the lifetime counter. A refused/stale candidate spends no id.
        if let Some((_, after)) = candidate.binding_sequence {
            self.mesh_id_sequence = after;
        }
        Ok(revision)
    }
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit_oracle.rs"]
mod object_image_mesh_edit_oracle;

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit_controls.rs"]
mod object_image_mesh_edit_controls_tests;
