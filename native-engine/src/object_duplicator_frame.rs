//! Real duplicator expansion retained by one immutable production frame.
//! Copies stay derived values; the authored source/order/selection tables do not change.
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::SpanBounds;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::{
    snapshot::SnapshotMotion, CopySet, Entry, Evaluation, Request,
};
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::object_document::RangeEvaluator;
use crate::object_frame_packet::{ObjectFrameError, ObjectFramePacket, ObjectFrameSelector};

#[derive(Debug, Clone)]
pub(crate) struct LayerCopies {
    pub(crate) layer_index: usize,
    pub(crate) output: CopySet,
    pub(crate) source_indices: Vec<usize>,
    // Admission fact of this successful output, never a document/global flag.
    evaluated_motion: bool,
}

impl ObjectFramePacket {
    pub(crate) fn duplicator_outputs(&self) -> &[LayerCopies] {
        &self.duplicators
    }
    /// Only this retained packet's successfully evaluated host owns the fact.
    /// Matching a layer index in another packet does not establish coverage.
    pub(crate) fn has_evaluated_duplicator_motion(&self, layer_index: usize) -> bool {
        self.duplicators
            .iter()
            .any(|copies| copies.layer_index == layer_index && copies.evaluated_motion)
    }
}

pub(super) fn prepare(
    packet: &ObjectFramePacket,
    exposure: bool,
    evaluated_motion: Option<&dyn MotionPort>,
) -> Result<Vec<LayerCopies>, ObjectFrameError> {
    let snapshot = packet.snapshot();
    let set = snapshot.document().duplicator_set();
    // FollowPath is consumed here only by an original non-content effector.
    // A role tag cannot claim FollowPath for another retained painter.
    if packet.source_layers().iter().any(|layer| {
        layer
            .follow_path
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|fp| !fp.target_layer_uid().is_empty())
            && !(set.is_some_and(|set| {
                set.effector_layers.iter().any(|effector| {
                    effector.context_id == packet.context_id()
                        && effector.layer_uid == layer.layer_uid()
                })
            }) && effector_motion_has_no_content(
                snapshot.document(),
                packet.context_id(),
                layer.layer_uid(),
            ))
    }) {
        return Err(ObjectFrameError::Unavailable);
    }
    let Some(set) = set else {
        return Ok(Vec::new());
    };
    let active = packet
        .source_layers()
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| {
            let descriptor = set.duplicator(packet.context_id(), layer.layer_uid())?;
            (layer.visible()
                && packet.source_frame_for_layer(index).is_some()
                && descriptor.duplicator.effective_clone_count() > 1.)
                .then_some((index, layer))
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(Vec::new());
    }
    // Entered/placed component expansion has its own original stage ordering;
    // never claim that join from a matching opaque layer ID in the root table.
    if packet.context_id() != "scene-root" {
        return Err(ObjectFrameError::Unavailable);
    }
    let (width, height) = snapshot
        .project()
        .ok_or(ObjectFrameError::Unavailable)?
        .project()
        .canvas_dimensions();
    let selector = ObjectFrameSelector {
        instance_id: packet.instance_id(),
        document_id: packet.document_id(),
        content_revision: packet.content_revision(),
        document_snapshot_id: packet.document_snapshot_id(),
        context_id: packet.context_id(),
        scope_kind: packet.scope_kind().clone(),
        frame: packet.frame(),
    };
    let sources = crate::object_frame_packet::duplicator_sources::SnapshotSources::new(
        snapshot, &selector, exposure,
    )?;
    let raw_motion = SnapshotMotion::new(snapshot, packet.context_id(), f64::from(packet.frame()));
    let motion = evaluated_motion.unwrap_or(&raw_motion);
    let ranges = RangeEvaluator::new(snapshot, packet.context_id(), packet.frame())
        .map_err(|_| ObjectFrameError::Unavailable)?;
    let mut output = Vec::with_capacity(active.len());
    let provenance = snapshot
        .document()
        .objects()
        .iter()
        .enumerate()
        .map(|(index, record)| (record.target(), index))
        .collect::<std::collections::HashMap<_, _>>();
    for (layer_index, layer) in active {
        if layer.rig().is_some() {
            return Err(ObjectFrameError::Unavailable);
        }
        let raw = sources.read(layer.layer_uid(), f64::from(packet.frame()))?;
        // Only the host's CURRENT base receives path effects before the clone
        // loop. The original temporal and Object-mode ports read raw drawings.
        let base = raw
            .iter()
            .map(|record| {
                crate::evaluation::object_vector_kernels::render_vector_eval::duplicator_base(
                    record,
                    layer,
                    packet.frame(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ObjectFrameError::Unavailable)?;
        let span = ranges
            .resolve(layer.layer_uid(), true)
            .map_err(|_| ObjectFrameError::Unavailable)?;
        let request = Request {
            context_id: packet.context_id(),
            host_uid: layer.layer_uid(),
            set,
            layers: packet.source_layers(),
            entry: Entry::Rendered { edit_source: false },
            is_3d_layer: layer.three_d.value() == Some(&true),
            base: &base,
            frame_idx: f64::from(packet.frame()),
            canvas_w: f64::from(width),
            canvas_h: f64::from(height),
            span: SpanBounds {
                start: span.in_point,
                len: span.out_point - span.in_point + 1.,
            },
        };
        if let Evaluation::Copies(copies) = sources.evaluate(&request, motion)? {
            let source_indices = copies
                .copies
                .iter()
                .map(|copy| {
                    provenance
                        .get(copy.record.target())
                        .copied()
                        .ok_or(ObjectFrameError::InvalidSource)
                })
                .collect::<Result<Vec<_>, _>>()?;
            output.push(LayerCopies {
                layer_index,
                output: copies,
                source_indices,
                evaluated_motion: evaluated_motion.is_some(),
            });
        }
    }
    Ok(output)
}

// Shared exact R2 non-content predicate; callers separately prove effector role.
fn effector_motion_has_no_content(
    document: &crate::object_document::ObjectDocument,
    context_id: &str,
    layer_uid: &str,
) -> bool {
    context_id == "scene-root"
        && !document.objects().iter().any(|record| {
            record.target().context_id() == context_id && record.target().layer_uid() == layer_uid
        })
        && document.item_records().is_none_or(|items| {
            !items
                .targets()
                .any(|item| item.context_id() == context_id && item.layer_uid() == layer_uid)
        })
        && document
            .component_set()
            .is_none_or(|set| set.instance(context_id, layer_uid).is_none())
}

/// Only declared duplicator/effector keys are newly owned by this producer.
/// Other fields still pass through the unchanged ordinary-holder admission.
pub(crate) fn remaining_holder(
    document: &crate::object_document::ObjectDocument,
    holder: &crate::object_document::PropertyHolder,
) -> Option<crate::object_document::PropertyHolder> {
    remaining_holder_with_motion(document, holder, false)
}

/// Only the actual owner-bound MotionPort admits evaluated duplicated fields.
/// Its effect sink refuses the whole frame when diagnostics/disables are pending.
pub(crate) fn remaining_holder_with_motion(
    document: &crate::object_document::ObjectDocument,
    holder: &crate::object_document::PropertyHolder,
    evaluated: bool,
) -> Option<crate::object_document::PropertyHolder> {
    let target = holder.target();
    if target.stroke_id().is_some() {
        return None;
    }
    let set = document.duplicator_set()?;
    let duplicator = set
        .duplicator(target.context_id(), target.layer_uid())
        .is_some();
    let effector = set.effector_layers.iter().any(|record| {
        record.context_id == target.context_id() && record.layer_uid == target.layer_uid()
    });
    // Effector layers are non-content roles in the original drawing source.
    // Do not steal a pose from another painter on a malformed mixed layer.
    let local_effector = evaluated
        && effector
        && effector_motion_has_no_content(document, target.context_id(), target.layer_uid());
    let owned = |key: &str| {
        (local_effector
            && matches!(
                key,
                "position" | "anchor" | "rotation" | "scale" | "opacity"
            ))
            || (duplicator
                && matches!(
                    key,
                    "dupOffsetPos"
                        | "dupOffsetRot"
                        | "dupOffsetScale"
                        | "dupOffsetOpacity"
                        | "dupOffsetHue"
                        | "dupOffsetPosZ"
                        | "dupOffsetRotX"
                        | "dupOffsetRotY"
                ))
            || (effector
                && matches!(
                    key,
                    "pathPercent"
                        | "pathInfluence"
                        | "effRadius"
                        | "effStrength"
                        | "effAngle"
                        | "effOffPosition"
                        | "effOffRotation"
                        | "effOffScale"
                        | "effOffOpacity"
                        | "effOffPositionZ"
                        | "effOffRotationX"
                        | "effOffRotationY"
                ))
    };
    if !holder
        .properties()
        .iter()
        .any(|property| owned(property.property()))
    {
        return None;
    }
    let mut remaining = holder.clone();
    // Raw-only packets keep their existing strict expression refusal. The
    // owner route actually supplies evaluated values for these exact keys.
    remaining.properties.retain(|property| {
        !owned(property.property())
            || (!evaluated
                && property
                    .expression
                    .value()
                    .is_some_and(|expression| expression.enabled))
    });
    Some(remaining)
}

/// Opacity and stroke hue are fields explicitly left for the real paint
/// consumer by the original placement kernel. Fill hue is already applied.
pub(crate) fn paint_style(
    packet: &ObjectFramePacket,
    record: &crate::object_frame_packet::ObjectRecord,
    factor: f64,
    hue: f64,
) -> Result<
    crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle,
    &'static str,
> {
    use crate::evaluation::object_component_kernels::duplicator_placement_color::rotate_fill_hue;
    use crate::evaluation::object_scene_style_kernels::object_scene_style_item::style_of_record_at_frame;
    use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
    let mut style = style_of_record_at_frame(record, factor, packet).map_err(|e| e.message())?;
    if hue != 0. {
        if let Some(stroke) = style.stroke.as_mut() {
            let [r, g, b, a] = stroke.paint.channels();
            let color = crate::object_document::SolidFill {
                kind: "solid".to_owned(),
                r: r.clone(),
                g: g.clone(),
                b: b.clone(),
                a: a.clone(),
            };
            let rotated =
                rotate_fill_hue(&color, hue).map_err(|_| "duplicator stroke hue is unavailable")?;
            stroke.paint = SceneSolid::from_fill(&rotated).map_err(|e| e.message())?;
        }
    }
    Ok(style)
}

pub(crate) fn paint_order(
    packet: &ObjectFramePacket,
    source: &crate::object_frame_packet::ObjectTarget,
    host_index: usize,
) -> Result<f64, &'static str> {
    let host = &packet.source_layers()[host_index];
    let source_frame = if source.layer_uid() == host.layer_uid() {
        source.frame()
    } else {
        let selected = packet.source_frame_for_layer(host_index);
        let found = packet
            .snapshot()
            .document()
            .objects()
            .iter()
            .filter(|record| {
                let target = record.target();
                target.context_id() == packet.context_id()
                    && target.layer_uid() == host.layer_uid()
                    && target.stroke_id() == source.stroke_id()
                    && target.scope_kind() == packet.scope_kind()
            })
            .map(|record| record.target().frame())
            .collect::<Vec<_>>();
        match selected.filter(|frame| found.contains(frame)) {
            Some(frame) => frame,
            None if found.is_empty() => return Ok(0.), // Empty Object-mode host has no element entry.
            None => return Err("duplicator pool Order has no admitted host-frame owner"),
        }
    };
    let order = crate::object_frame_packet::raw_expression_order(
        packet.snapshot(),
        packet.context_id(),
        packet.scope_kind(),
        source_frame,
        host.layer_uid(),
        Some(source.stroke_id()),
    )
    .map_err(|_| "duplicator element Order is unavailable")?;
    crate::object_frame_packet::raw_expression_numeric(order, packet.frame())
        .map_err(|_| "duplicator element Order is unavailable")
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_duplicator_frame.rs"]
pub(crate) mod tests;

/// Existing frame construction, with the actual original Motion lookup port.
/// Application owns the guest and every effect; evaluation never imports it.
pub(crate) fn prepare_with_motion(
    snapshot: &crate::revision::ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    motion: &dyn MotionPort,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    crate::object_frame_packet::prepare_frame_with_order(
        snapshot,
        selector,
        true,
        crate::object_frame_packet::order::prepare_context,
        Some(motion),
    )
}
