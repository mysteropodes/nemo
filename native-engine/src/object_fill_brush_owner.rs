//! Bounded public Fill Brush payload -> the existing sealed boolean/history candidate (#2361).
use crate::commands::object_fill_owner::group_edit::boolean_edit::fill_trace::{
    paint_of, promoted,
};
use crate::commands::object_fill_owner::group_edit::boolean_edit::{
    BooleanEditKind, PreparedBooleanEdit,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::check_scope;
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    apply_stack, find_record, fresh_ids, stack_of, Scope,
};
use crate::object_document::ObjectDocument;
use crate::object_document::object_group_kernels::object_boolean_ops::RingSeg;
use crate::object_document::object_group_kernels::object_fill_trace_ops::brush::{
    plan_brush, Placement,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Segment {
    point: [f64; 2],
    handle_in: [f64; 2],
    handle_out: [f64; 2],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Payload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    /// The closed stroke outline (draw-bridge's ribbon of the Fill Brush samples), relative handles.
    outline: Vec<Segment>,
    color: String,
    opacity: f64,
    placement: String,
    /// Mask mode's `maskModeType` (tagOwner), stored verbatim like any mask tag, or `false` when Mask mode is off.
    mask: serde_json::Value,
    /// The symmetry copies of the committed path (Extend mirror or radial), affine `[a, b, c, d, e, f]`, in stack order.
    copies: Vec<[f64; 6]>,
    /// Labs multi-frame draw: the other selected frames of the layer the committed path is also stamped on.
    stamps: Vec<u32>,
}

/// multiframe-draw.js keys each stamped cell first (ensureKeyframe): a held or unexposed cell becomes a key holding what
/// it showed. The hook runs before the drawn frame is saved, so this reads the document as it was before the stroke.
fn key_stamps(document: &ObjectDocument, scope: &Scope, frames: &[u32]) -> Result<Option<ObjectDocument>, DispatchErrorCode> {
    let held: Vec<u32> = document
        .layers
        .iter()
        .find(|l| l.layer_uid == scope.layer_uid)
        .and_then(|l| l.timing.value())
        .map(|t| frames.iter().copied().filter(|&f| matches!(t.exposure(f).0, "held" | "unexposed")).collect())
        .unwrap_or_default();
    if held.is_empty() {
        return Ok(None);
    }
    match promote_cells(document, &scope.layer_uid, &held).map_err(|_| DispatchErrorCode::Unavailable)? {
        DocumentEdit::Changed(next) => Ok(Some(next)),
        _ => Ok(None),
    }
}

/// multiframe-draw.js onStroke over the committed Fill Brush path (draw-bridge.js runs tagOwner, then the Labs hooks,
/// then the symmetry copies @1065e9d8): a copy of `shown` goes on top of each listed cell, in the commit's one revision.
fn stamp(document: &mut ObjectDocument, scope: &Scope, shown: &str, frames: &[u32], revision: u64) -> Result<Vec<String>, DispatchErrorCode> {
    if frames.is_empty() {
        return Ok(Vec::new());
    }
    let source = document.objects[find_record(document, scope, shown).ok_or(DispatchErrorCode::Unavailable)?].clone();
    let ids = fresh_ids(document, revision, frames.len()).ok_or(DispatchErrorCode::Unavailable)?;
    for (&frame, id) in frames.iter().zip(&ids) {
        let cell = Scope { context_id: scope.context_id.clone(), layer_uid: scope.layer_uid.clone(), frame };
        let mut copy = source.clone();
        copy.target = cell.target(id);
        let mut stack = stack_of(document, &cell);
        stack.push(id.clone());
        apply_stack(document, &cell, &stack, &[], vec![copy]).map_err(|_| DispatchErrorCode::Unavailable)?;
    }
    Ok(ids)
}

impl ObjectFillOwner {
    pub(crate) fn prepare_fill_brush_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        let p: Payload =
            crate::commands::object_fill_owner::group_edit::boolean_edit::wire::parse(bytes)?;
        let finite = |v: &[f64; 2]| v.iter().all(|n| n.is_finite() && n.abs() <= 1.0e12);
        let placement = match p.placement.as_str() {
            "above" => Placement::Above,
            "below" => Placement::Below,
            "merge" => Placement::Merge,
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        if p.command != "object.fill.brush"
            || !crate::commands::object_fill_owner::group_edit::boolean_edit::target_ok(
                &p.layer_uid,
                &p.context_id,
            )
            || !(3..=256).contains(&p.outline.len())
            || !p
                .outline
                .iter()
                .all(|s| finite(&s.point) && finite(&s.handle_in) && finite(&s.handle_out))
            || !p.opacity.is_finite()
            || !(0.0..=1.0).contains(&p.opacity)
            || p.copies.len() > 23
            || p.stamps.len() > 256
            || p.stamps.iter().enumerate().any(|(i, f)| *f == p.frame || p.stamps[..i].contains(f))
            || !p.copies.iter().all(|m| m.iter().all(|n| n.is_finite() && n.abs() <= 1.0e12))
            || !(p.mask == serde_json::Value::Bool(false)
                || p.mask.as_str().is_some_and(|m| !m.is_empty() && m.len() <= 64 && !m.chars().any(char::is_control)))
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let paint = paint_of(&p.color).ok_or(DispatchErrorCode::InvalidRequest)?;
        let outline: Vec<RingSeg> = p
            .outline
            .iter()
            .map(|s| RingSeg {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
            })
            .collect();
        let scope = Scope {
            context_id: p.context_id,
            layer_uid: p.layer_uid,
            frame: p.frame,
        };
        let origin = self.current_snapshot();
        check_scope(origin.document(), &scope).map_err(|error| Self::fill_plan_error(&error))?;
        let promoted = promoted(origin.document(), &scope)?;
        if p.stamps.iter().any(|f| *f >= origin.document().total_frames) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let base = promoted.as_ref().unwrap_or_else(|| origin.document());
        let keyed = key_stamps(base, &scope, &p.stamps)?;
        let (mut plan, shown) = plan_brush(
            keyed.as_ref().unwrap_or(base),
            &scope,
            &outline,
            &paint,
            p.opacity,
            placement,
            p.mask.as_str(),
            &p.copies,
            origin.content_revision(),
        )
        .map_err(|error| Self::fill_plan_error(&error))?;
        plan.changed |= promoted.is_some() || keyed.is_some();
        let stamped = stamp(&mut plan.document, &scope, &shown, &p.stamps, origin.content_revision())?;
        plan.changed |= !stamped.is_empty();
        plan.created.extend(stamped);
        ObjectSnapshot::validate_drawing(&plan.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedBooleanEdit {
            origin,
            plan,
            kind: BooleanEditKind::FillBrush,
        })
    }
}
