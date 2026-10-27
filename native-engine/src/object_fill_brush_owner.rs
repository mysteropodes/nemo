//! Bounded public Fill Brush payload -> the existing sealed boolean/history candidate (#2361).
use crate::commands::object_fill_owner::group_edit::boolean_edit::fill_trace::{
    paint_of, promoted,
};
use crate::commands::object_fill_owner::group_edit::boolean_edit::{
    BooleanEditKind, PreparedBooleanEdit,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::check_scope;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
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
        let mut plan = plan_brush(
            promoted.as_ref().unwrap_or_else(|| origin.document()),
            &scope,
            &outline,
            &paint,
            p.opacity,
            placement,
            origin.content_revision(),
        )
        .map_err(|error| Self::fill_plan_error(&error))?;
        plan.changed |= promoted.is_some();
        ObjectSnapshot::validate_drawing(&plan.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedBooleanEdit {
            origin,
            plan,
            kind: BooleanEditKind::FillBrush,
        })
    }
}
