//! Bounded public paint-bucket payload -> the existing sealed boolean/history candidate.
use super::{BooleanEditKind, PreparedBooleanEdit};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::PlanError;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::object_group_kernels::object_fill_trace_ops::document::plan_trace;
use crate::object_document::SolidFill;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Payload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    point: [f64; 2],
    color: String,
    opacity: f64,
    remove: bool,
}

impl ObjectFillOwner {
    pub(crate) fn fill_plan_error(error: &PlanError) -> DispatchErrorCode {
        super::refusal_code(error)
    }
    pub(crate) fn prepare_fill_trace_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        let p: Payload = super::wire::parse(bytes)?;
        if p.command != "object.fill.trace"
            || !super::target_ok(&p.layer_uid, &p.context_id)
            || !p.point.iter().all(|n| n.is_finite() && n.abs() <= 1.0e12)
            || !p.opacity.is_finite()
            || !(0.0..=1.0).contains(&p.opacity)
            || p.color.len() != 9
            || !p.color.starts_with('#')
            || !p.color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let channel = |i| {
            Number::from_f64(f64::from(u8::from_str_radix(&p.color[i..i + 2], 16).unwrap()) / 255.0)
                .unwrap()
        };
        let paint = SolidFill {
            kind: "solid".to_string(),
            r: channel(1),
            g: channel(3),
            b: channel(5),
            a: channel(7),
        };
        let origin = self.current_snapshot();
        let scope = Scope {
            context_id: p.context_id,
            layer_uid: p.layer_uid,
            frame: p.frame,
        };
        let plan = plan_trace(
            origin.document(),
            &scope,
            p.point,
            &paint,
            p.opacity,
            p.remove,
            origin.content_revision(),
        )
        .map_err(|error| Self::fill_plan_error(&error))?;
        ObjectSnapshot::validate_drawing(&plan.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedBooleanEdit {
            origin,
            plan,
            kind: BooleanEditKind::FillTrace,
        })
    }
}
