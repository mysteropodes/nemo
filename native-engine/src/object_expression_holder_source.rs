//! Actual immutable expression source lookup; no evaluator, owner or shadow holder.
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_document::{ExpressionHolder, PropertyRecord};
use crate::object_frame_packet::{sample_property_raw, ObjectFrameError};
use crate::revision::ObjectSnapshot;
pub(super) fn animator<'a>(
    snapshot: &'a ObjectSnapshot,
    target: &ExpressionHolder,
) -> Option<&'a crate::object_document::object_text_layout_kernels::animator::AnimatorRecord> {
    let ExpressionHolder::TextAnimator {
        context_id,
        layer_uid,
        animator_id,
    } = target
    else {
        return None;
    };
    ContextSource::of_document(snapshot.document())
        .context(context_id)
        .ok()?
        .layers()
        .iter()
        .find(|l| l.layer_uid() == layer_uid)?
        .text_animators
        .value()?
        .0
        .iter()
        .find(|a| a.id == *animator_id)
}
pub(crate) fn record<'a>(
    snapshot: &'a ObjectSnapshot,
    target: &ExpressionHolder,
    property: &str,
) -> Option<&'a PropertyRecord> {
    match target {
        ExpressionHolder::Property(target) => snapshot
            .document()
            .property_holders()?
            .iter()
            .find(|h| h.target() == target)?
            .properties()
            .iter()
            .find(|p| p.property() == property),
        ExpressionHolder::TextAnimator { .. } => animator(snapshot, target)?
            .properties
            .value()?
            .iter()
            .find(|p| p.property() == property),
    }
}
pub(crate) fn code<'a>(
    snapshot: &'a ObjectSnapshot,
    target: &ExpressionHolder,
    property: &str,
) -> Option<&'a str> {
    if let ExpressionHolder::Property(target) = target {
        if property == "order" {
            return snapshot
                .document()
                .property_order(target)?
                .expression
                .value()
                .map(|e| e.code.as_str());
        }
    }
    record(snapshot, target, property)?
        .expression
        .value()
        .map(|e| e.code.as_str())
}
pub(crate) fn enabled(
    snapshot: &ObjectSnapshot,
    target: &ExpressionHolder,
    property: &str,
) -> bool {
    if let ExpressionHolder::Property(target) = target {
        if property == "order" {
            return snapshot
                .document()
                .property_order(target)
                .and_then(|o| o.expression.value())
                .is_some_and(|e| e.enabled);
        }
    }
    record(snapshot, target, property)
        .and_then(|p| p.expression.value())
        .is_some_and(|e| e.enabled)
}
pub(crate) fn raw(
    snapshot: &ObjectSnapshot,
    target: &ExpressionHolder,
    property: &str,
    frame: f64,
) -> Result<Vec<f64>, ObjectFrameError> {
    match target {
        ExpressionHolder::Property(target) => {
            sample_property_raw(snapshot, target, property, frame)
                .map_err(|_| ObjectFrameError::Unavailable)
        }
        ExpressionHolder::TextAnimator { .. } => animator(snapshot, target)
            .ok_or(ObjectFrameError::InvalidSource)?
            .raw_selector_property(property, frame)
            .map(|v| vec![v])
            .map_err(|_| ObjectFrameError::Unavailable),
    }
}
pub(crate) fn diagnostic_target(target: &ExpressionHolder) -> serde_json::Value {
    match target {
        ExpressionHolder::Property(t) => serde_json::to_value(t).expect("typed target serializes"),
        ExpressionHolder::TextAnimator {
            context_id,
            layer_uid,
            animator_id,
        } => {
            serde_json::json!({"contextId":context_id,"layerUid":layer_uid,"animatorId":animator_id})
        }
    }
}
