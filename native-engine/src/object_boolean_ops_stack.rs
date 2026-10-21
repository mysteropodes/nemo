//! Frame stack, result records and identity for the boolean edits (G19 + G18, L02-I).
//!
//! The z-order of one frame of one layer is its stroke list bottom to top: the
//! composition plan's natural positions when the document carries one, else the
//! order of the records. An edit states its FINAL stack (survivors plus new
//! ids) and this module writes it back to both places, so a result lands where
//! the baseline's `insertBooleanResult` put it and one Undo restores the whole
//! previous document. Result records keep the style operand's family: a plain
//! fill record for the legacy family, a fill-only vector record (no stroke
//! unless the eraser redraws one) for the vector family.
use std::collections::HashSet;

use serde_json::Number;

use crate::object_document::object_group_kernels::object_boolean_ops::{Point, RingSeg};
use crate::object_document::object_group_kernels::object_boolean_topology::IslandBuild;
use crate::object_document::{
    AuthoredOrder, FrameScope, FrameScopeKind, ObjectDocument, ObjectGeometry, ObjectPoint,
    ObjectRecord, ObjectSegment, ObjectTarget, OrderElement, Presence, SolidFill, VectorBody,
    VectorExt, VECTOR_FAMILY,
};

pub(crate) const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;
/// Mirrors the storage admission bound of the cubic geometry (`object_vector_codec`).
pub(crate) const MAX_SEGMENTS: usize = 256;

/// Context, layer and authored frame an edit works on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) frame: u32,
}

impl Scope {
    pub(crate) fn target(&self, stroke_id: &str) -> ObjectTarget {
        ObjectTarget {
            context_id: self.context_id.clone(),
            frame_scope: FrameScope {
                kind: FrameScopeKind::Authored,
                frame: self.frame,
            },
            layer_uid: self.layer_uid.clone(),
            stroke_id: stroke_id.to_string(),
        }
    }
    pub(crate) fn owns(&self, target: &ObjectTarget) -> bool {
        target.context_id == self.context_id
            && target.layer_uid == self.layer_uid
            && target.frame_scope.kind == FrameScopeKind::Authored
            && target.frame_scope.frame == self.frame
    }
}

pub(crate) fn find_record(
    document: &ObjectDocument,
    scope: &Scope,
    stroke_id: &str,
) -> Option<usize> {
    document
        .objects
        .iter()
        .position(|record| scope.owns(&record.target) && record.target.stroke_id == stroke_id)
}

/// The frame's stroke ids, bottom to top.
pub(crate) fn stack_of(document: &ObjectDocument, scope: &Scope) -> Vec<String> {
    if let Presence::Present(plan) = &document.composition_order {
        if let Some(context) = plan
            .contexts
            .iter()
            .find(|c| c.context_id == scope.context_id)
        {
            if let Some(found) = context.scopes.iter().find(|s| {
                s.frame_scope.kind == FrameScopeKind::Authored && s.frame_scope.frame == scope.frame
            }) {
                let mut elements: Vec<&OrderElement> = found
                    .elements
                    .iter()
                    .filter(|e| e.layer_uid == scope.layer_uid)
                    .collect();
                elements.sort_by_key(|e| e.natural_position);
                return elements.iter().map(|e| e.stroke_id.clone()).collect();
            }
        }
    }
    document
        .objects
        .iter()
        .filter(|record| scope.owns(&record.target))
        .map(|record| record.target.stroke_id.clone())
        .collect()
}

/// `native-stroke:{next revision}:{n}` ids that collide with nothing in the document.
pub(crate) fn fresh_ids(
    document: &ObjectDocument,
    revision: u64,
    count: usize,
) -> Option<Vec<String>> {
    let next = revision
        .checked_add(1)
        .filter(|next| *next <= MAX_SAFE_REVISION)?;
    let taken: HashSet<&str> = document
        .objects
        .iter()
        .map(|record| record.target.stroke_id.as_str())
        .collect();
    let mut out: Vec<String> = Vec::with_capacity(count);
    let mut suffix = 0usize;
    while out.len() < count {
        let candidate = format!("native-stroke:{next}:{suffix}");
        if !taken.contains(candidate.as_str()) {
            out.push(candidate);
        }
        suffix += 1;
        if suffix > taken.len() + count + 1 {
            return None;
        }
    }
    Some(out)
}

fn number(value: f64) -> Option<Number> {
    Number::from_f64(value)
}

fn point_number(point: Point) -> Option<ObjectPoint> {
    Some(ObjectPoint {
        x: number(point[0])?,
        y: number(point[1])?,
    })
}

/// The redrawn outline of an erased shape.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutlineSpec {
    pub(crate) paint: SolidFill,
    pub(crate) width: Number,
    pub(crate) cap: String,
    pub(crate) join: String,
}

/// What a result record takes from its style source.
pub(crate) struct ResultStyle<'a> {
    pub(crate) source: &'a ObjectRecord,
    pub(crate) fill: SolidFill,
    pub(crate) opacity: Number,
    pub(crate) outline: Option<OutlineSpec>,
    pub(crate) box_angle: Option<Number>,
    /// `booleanOp` assigns the style source's stroke width / cap / join to the result BEFORE it is
    /// flattened, so a result that stays one plain path (`IslandBuild::Single`) keeps them even though
    /// its stroke colour is dropped; compound children and slit-merged islands are fresh paths with
    /// Paper's defaults (1 / butt / miter). Measured with `tools/probe_stroke_style.js`.
    pub(crate) keep_source_stroke_style: bool,
}

pub(crate) fn is_vector(record: &ObjectRecord) -> bool {
    record.family == VECTOR_FAMILY
}

/// A fill-only (or outlined) closed record for one island ring.
pub(crate) fn island_record(
    style: &ResultStyle<'_>,
    target: ObjectTarget,
    ring: &[RingSeg],
    build: IslandBuild,
) -> Option<ObjectRecord> {
    let mut segments: Vec<ObjectSegment> = Vec::with_capacity(ring.len());
    for seg in ring {
        segments.push(ObjectSegment {
            point: point_number(seg.point)?,
            handle_in: point_number(seg.handle_in)?,
            handle_out: point_number(seg.handle_out)?,
        });
    }
    let geometry = ObjectGeometry {
        kind: "cubic-path".to_string(),
        closed: true,
        coordinate_space: "document-world".to_string(),
        handle_space: "relative".to_string(),
        segments,
    };
    if !is_vector(style.source) {
        return Some(ObjectRecord {
            schema_version: 1,
            family: style.source.family.clone(),
            target,
            geometry,
            fill: Presence::Present(Some(style.fill.clone())),
            vector: Presence::Missing,
            vector_ext: Presence::Missing,
            effects: style.source.effects.clone(),
        });
    }
    let carried = if style.keep_source_stroke_style && build == IslandBuild::Single {
        style.source.vector()
    } else {
        None
    };
    let (stroke_width, stroke_cap, stroke_join) = match (&style.outline, carried) {
        (Some(outline), _) => (
            outline.width.clone(),
            outline.cap.clone(),
            outline.join.clone(),
        ),
        (None, Some(source)) => (
            source.stroke_width.clone(),
            source.stroke_cap.clone(),
            source.stroke_join.clone(),
        ),
        (None, None) => (number(1.0)?, "butt".to_string(), "miter".to_string()),
    };
    let body = VectorBody {
        stroke: Presence::Present(style.outline.as_ref().map(|o| o.paint.clone())),
        has_real_stroke: style.outline.is_some(),
        stroke_width,
        stroke_cap,
        stroke_join,
        miter_limit: Presence::Missing,
        dash_array: Presence::Missing,
        dash_offset: Presence::Missing,
        paint_order: Presence::Missing,
        opacity: style.opacity.clone(),
    };
    // Baseline BOOL_KEEP_DATA_ALL carries the style source's gradient through Boolean/erase/Flatten.
    let gradient = style.source.fill_gradient().cloned();
    let trace = style
        .source
        .vector_ext
        .value()
        .and_then(|ext| ext.fill_trace.value())
        .cloned();
    let ext = match (&style.box_angle, gradient) {
        (angle, gradient) if angle.is_some() || gradient.is_some() || trace.is_some() => {
            Presence::Present(VectorExt {
                pressure: Presence::Missing,
                param_shape: Presence::Missing,
                box_angle: angle.clone().map_or(Presence::Missing, Presence::Present),
                fill_gradient: gradient.map_or(Presence::Missing, Presence::Present),
                fill_trace: trace.map_or(Presence::Missing, Presence::Present),
                // Original BOOL_KEEP_DATA_ALL does not transfer mask tags.
                is_mask: Presence::Missing,
                mask_mode: Presence::Missing,
                mask_feather: Presence::Missing,
            })
        }
        _ => Presence::Missing,
    };
    Some(ObjectRecord {
        schema_version: 1,
        family: style.source.family.clone(),
        target,
        geometry,
        fill: Presence::Present(Some(style.fill.clone())),
        vector: Presence::Present(body),
        vector_ext: ext,
        effects: style.source.effects.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackError {
    /// The composition plan has no context for the scope.
    ContextAbsent,
}

/// Write a final stack back: records and (when present) the composition plan.
/// `removed` are the ids whose record at the frame goes; `created` are the new
/// records (their ids are in `stack`); a kept or re-created id keeps its authored order data.
pub(crate) fn apply_stack(
    document: &mut ObjectDocument,
    scope: &Scope,
    stack: &[String],
    removed: &[String],
    created: Vec<ObjectRecord>,
) -> Result<(), StackError> {
    document.objects.retain(|record| {
        !(scope.owns(&record.target) && removed.contains(&record.target.stroke_id))
    });
    for record in created {
        let id = record.target.stroke_id.clone();
        let at = stack
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap_or(stack.len());
        let above = stack[at.min(stack.len())..]
            .iter()
            .skip(1)
            .find_map(|next| find_record(document, scope, next));
        let index = match above {
            Some(index) => index,
            None => document
                .objects
                .iter()
                .rposition(|record| scope.owns(&record.target))
                .map_or(document.objects.len(), |last| last + 1),
        };
        document.objects.insert(index, record);
    }
    if let Presence::Present(plan) = &mut document.composition_order {
        let context = plan
            .contexts
            .iter_mut()
            .find(|c| c.context_id == scope.context_id)
            .ok_or(StackError::ContextAbsent)?;
        let found = context.scopes.iter_mut().find(|s| {
            s.frame_scope.kind == FrameScopeKind::Authored && s.frame_scope.frame == scope.frame
        });
        if let Some(found) = found {
            let old: Vec<OrderElement> = found
                .elements
                .iter()
                .filter(|e| e.layer_uid == scope.layer_uid)
                .cloned()
                .collect();
            found.elements.retain(|e| e.layer_uid != scope.layer_uid);
            for (position, id) in stack.iter().enumerate() {
                let order = old
                    .iter()
                    .find(|e| e.stroke_id == *id)
                    .map(|e| e.order.clone())
                    .unwrap_or(AuthoredOrder {
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    });
                found.elements.push(OrderElement {
                    layer_uid: scope.layer_uid.clone(),
                    stroke_id: id.clone(),
                    natural_position: u32::try_from(position).unwrap_or(u32::MAX),
                    order,
                });
            }
        } else {
            return Err(StackError::ContextAbsent);
        }
    }
    Ok(())
}

/// `colorHex8` of a solid paint (text is only an identity and truthiness carrier here).
pub(crate) fn hex8(paint: &SolidFill) -> String {
    let channel = |n: &Number| -> u32 {
        let v = n.as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
        (v * 255.0).round() as u32
    };
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(&paint.r),
        channel(&paint.g),
        channel(&paint.b),
        channel(&paint.a)
    )
}
