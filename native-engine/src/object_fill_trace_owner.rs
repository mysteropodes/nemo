//! Bounded public paint-bucket payload -> the existing sealed boolean/history candidate.
use crate::commands::object_fill_owner::group_edit::boolean_edit::{
    BooleanEditKind, PreparedBooleanEdit,
};
use crate::commands::object_fill_owner::object_frame_edit_document::{
    promote_cells, DocumentEdit, FrameEditError,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    check_scope, PlanError,
};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::object_group_kernels::object_fill_trace_ops::document::plan_trace;
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, SolidFill};
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
        crate::commands::object_fill_owner::group_edit::boolean_edit::refusal_code(error)
    }
    pub(crate) fn prepare_fill_trace_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        let p: Payload =
            crate::commands::object_fill_owner::group_edit::boolean_edit::wire::parse(bytes)?;
        if p.command != "object.fill.trace"
            || !crate::commands::object_fill_owner::group_edit::boolean_edit::target_ok(
                &p.layer_uid,
                &p.context_id,
            )
            || !p.point.iter().all(|n| n.is_finite() && n.abs() <= 1.0e12)
            || !p.opacity.is_finite()
            || !(0.0..=1.0).contains(&p.opacity)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        self.prepare_fill_trace_at(
            Scope {
                context_id: p.context_id,
                layer_uid: p.layer_uid,
                frame: p.frame,
            },
            p.point,
            &p.color,
            p.opacity,
            p.remove,
            false,
        )
    }

    /// The ordinary bucket fill over an admitted scope. With `reference`, every other effectively visible layer's
    /// shown drawing at the frame joins the walls (U36 reference fill, `src/js/labs/reference-fill.js`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_fill_trace_at(
        &self,
        scope: Scope,
        point: [f64; 2],
        color: &str,
        opacity: f64,
        remove: bool,
        reference: bool,
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        if color.len() != 9
            || !color.starts_with('#')
            || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let channel = |i| {
            Number::from_f64(f64::from(u8::from_str_radix(&color[i..i + 2], 16).unwrap()) / 255.0)
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
        check_scope(origin.document(), &scope).map_err(|error| Self::fill_plan_error(&error))?;
        let held = scope.context_id == "scene-root"
            && origin
                .document()
                .layers
                .iter()
                .find(|layer| layer.layer_uid == scope.layer_uid)
                .and_then(|layer| layer.timing.value())
                .is_some_and(|timing| {
                    matches!(timing.exposure(scope.frame).0, "held" | "unexposed")
                });
        // Ordinary Fill's ensureKeyframe is part of the same undo operation.
        // Reuse the real detached frame kernel before tracing the visible copy;
        // a later geometry/provider refusal still publishes neither change.
        let promoted = if held {
            match promote_cells(origin.document(), &scope.layer_uid, &[scope.frame]) {
                Ok(DocumentEdit::Changed(document)) => Some(document),
                Ok(DocumentEdit::NoChange) => None,
                Err(FrameEditError::Unsupported | FrameEditError::MissingTiming) => {
                    return Err(DispatchErrorCode::Unavailable)
                }
                Err(FrameEditError::UnknownLayer) => return Err(DispatchErrorCode::NotFound),
                Err(_) => return Err(DispatchErrorCode::InvalidRequest),
            }
        } else {
            None
        };
        let walls = reference.then(|| reference_walls(origin.document(), &scope));
        let mut plan = plan_trace(
            promoted.as_ref().unwrap_or_else(|| origin.document()),
            &scope,
            point,
            &paint,
            opacity,
            remove,
            origin.content_revision(),
            walls.as_deref(),
        )
        .map_err(|error| Self::fill_plan_error(&error))?;
        plan.changed |= promoted.is_some();
        ObjectSnapshot::validate_drawing(&plan.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedBooleanEdit {
            origin,
            plan,
            kind: BooleanEditKind::FillTrace,
        })
    }
}

/// The shown drawing of every other effectively visible scene-root layer at the scope's frame, by the baseline filter:
/// hidden layers and hidden folders, component instances, linked-fill companions, brush-texture dabs and paths with
/// fewer than two anchors are not walls. The native document stores no solo, so none applies.
fn reference_walls<'a>(document: &'a ObjectDocument, scope: &Scope) -> Vec<&'a ObjectRecord> {
    let visible = |uid: &str| {
        document
            .layers
            .iter()
            .find(|l| l.layer_uid == uid)
            .is_some_and(|l| l.visible())
    };
    let mut walls = Vec::new();
    for layer in &document.layers {
        let folder_hidden = layer
            .parent_layer_uid
            .value()
            .and_then(Option::as_deref)
            .is_some_and(|parent| {
                document
                    .layers
                    .iter()
                    .any(|l| l.layer_uid == parent && l.is_folder_layer.value() == Some(&true))
                    && !visible(parent)
            });
        let instance = document
            .component_set()
            .is_some_and(|set| set.instance(&scope.context_id, &layer.layer_uid).is_some());
        if layer.layer_uid == scope.layer_uid || !layer.visible() || folder_hidden || instance {
            continue;
        }
        // An untimed layer has no held cells: the frame is its own key.
        let source = match layer.timing.value() {
            Some(timing) => timing.exposure(scope.frame).1,
            None => Some(scope.frame),
        };
        let Some(source) = source else {
            continue;
        };
        walls.extend(document.objects.iter().filter(|r| {
            let ext = r.vector_ext.value();
            r.target.context_id == scope.context_id
                && r.target.layer_uid == layer.layer_uid
                && r.target.frame_scope.kind == FrameScopeKind::Authored
                && r.target.frame_scope.frame == source
                && r.geometry.segments.len() >= 2
                && !ext
                    .and_then(|e| e.is_linked_fill_companion.value())
                    .is_some_and(|v| *v)
                && !ext
                    .and_then(|e| e.brush_texture.value())
                    .and_then(|b| b.is_brush_texture_copy.value())
                    .is_some_and(|v| *v)
        }));
    }
    walls
}
