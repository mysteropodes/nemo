//! Pure raw Motion sampling from one retained native snapshot. No expression execution.
use crate::animation_curve::{evaluate_curve, CurveSample};
use crate::object_document::{
    ObjectDocument, OrderKey, Presence, PropertyHolder, PropertyHolderTarget,
};
use crate::revision::ObjectSnapshot;
use serde_json::Number;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertySampleError {
    InvalidFrame,
    MissingTarget,
    UnsupportedProperty,
    Unavailable,
}
fn numbers(values: &[Number]) -> Result<Vec<f64>, PropertySampleError> {
    values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or(PropertySampleError::Unavailable)
        })
        .collect()
}
fn frame(key: &OrderKey) -> f64 {
    key.frame.as_f64().expect("admitted key frame")
}
fn handle(value: &Presence<Vec<Number>>) -> [f64; 2] {
    value
        .value()
        .map(|v| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()])
        .unwrap_or([0., 0.])
}
fn target_exists(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
    if let Some(component) = document.component_set().and_then(|set| {
        set.components
            .iter()
            .find(|component| component.symbol_id() == target.context_id())
    }) {
        return component
            .layers()
            .iter()
            .any(|layer| layer.layer_uid() == target.layer_uid())
            && target.stroke_id().is_none_or(|stroke| {
                PropertyHolder::item_target_exists(document, target)
                    || component.objects().iter().any(|record| {
                        let item = record.target();
                        item.context_id() == target.context_id()
                            && item.layer_uid() == target.layer_uid()
                            && item.stroke_id() == stroke
                            && item.scope_kind()
                                == &crate::object_document::FrameScopeKind::Authored
                    })
            });
    }
    let context_exists =
        document
            .composition_order
            .value()
            .map_or(target.context_id() == "scene-root", |plan| {
                plan.contexts
                    .iter()
                    .any(|context| context.context_id == target.context_id())
            });
    context_exists
        && document
            .layers
            .iter()
            .any(|layer| layer.layer_uid() == target.layer_uid())
        && target.stroke_id().is_none_or(|stroke| {
            PropertyHolder::item_target_exists(document, target)
                || document.objects.iter().any(|object| {
                    object.target.context_id == target.context_id()
                        && object.target.layer_uid == target.layer_uid()
                        && object.target.stroke_id == stroke
                        && object.target.frame_scope.kind
                            == crate::object_document::FrameScopeKind::Authored
                })
        })
}
pub(crate) fn default_curve() -> Vec<CurveSample> {
    [(0., 0.), (0.25, 0.156), (0.5, 0.5), (0.75, 0.844), (1., 1.)]
        .into_iter()
        .map(|(x, y)| CurveSample {
            x,
            y,
            tx: None,
            ty: None,
        })
        .collect()
}
fn curve(key: &OrderKey) -> Vec<CurveSample> {
    match key.curve_points.value().and_then(Option::as_ref) {
        None => default_curve(),
        Some(points) => points
            .iter()
            .map(|point| CurveSample {
                x: point.x.as_f64().unwrap(),
                y: point.y.as_f64().unwrap(),
                tx: point.tx.value().and_then(Number::as_f64),
                ty: point.ty.value().and_then(Number::as_f64),
            })
            .collect(),
    }
}
/// RAW sampling deliberately ignores active expressions, including self.at recursion.
/// An inherited paint value without its bound native source is explicitly unavailable.
/// The caller retains and, before publication, revalidates the supplied snapshot.
pub fn sample_property_raw(
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    property: &str,
    at: f64,
) -> Result<Vec<f64>, PropertySampleError> {
    if !at.is_finite() {
        return Err(PropertySampleError::InvalidFrame);
    }
    let document = snapshot.document();
    if !target_exists(document, target) {
        return Err(PropertySampleError::MissingTarget);
    }
    if property == "order" {
        let order = document.property_order(target).ok_or(PropertySampleError::Unavailable)?;
        return super::order::numeric_at(order, at).map(|v| vec![v])
            .map_err(|_| PropertySampleError::Unavailable);
    }
    if property == "timeRemap" {
        return Err(PropertySampleError::UnsupportedProperty);
    }
    let empty = PropertyHolder {
        target: target.clone(),
        controls: Presence::Missing,
        properties: vec![],
    };
    let holder = document
        .property_holders()
        .unwrap_or(&[])
        .iter()
        .find(|holder| holder.target() == target)
        .unwrap_or(&empty);
    let metadata = holder
        .metadata_in_document(document, property)
        .ok_or(PropertySampleError::UnsupportedProperty)?;
    let record = holder
        .properties()
        .iter()
        .find(|record| record.property() == property);
    let keys = record
        .and_then(|record| record.track.value())
        .map(|track| track.keys.as_slice())
        .unwrap_or(&[]);
    if keys.is_empty() {
        return record
            .and_then(|record| record.static_value.value())
            .map(Vec::as_slice)
            .or(metadata.default_value())
            .ok_or(PropertySampleError::Unavailable)
            .and_then(numbers);
    }
    sample_keys(property, keys, at)
}
fn sample_keys(
    property: &str,
    keys: &[OrderKey],
    at: f64,
) -> Result<Vec<f64>, PropertySampleError> {
    let first = &keys[0];
    let last = &keys[keys.len() - 1];
    if at <= frame(first) {
        return numbers(&first.v);
    }
    if at >= frame(last) {
        return numbers(&last.v);
    }
    let right = keys.partition_point(|key| frame(key) <= at);
    let a = &keys[right - 1];
    let b = &keys[right];
    if a.hold.value().copied().unwrap_or(false) {
        return numbers(&a.v);
    }
    let eased = evaluate_curve(&curve(a), (at - frame(a)) / (frame(b) - frame(a)));
    let av = numbers(&a.v)?;
    let bv = numbers(&b.v)?;
    let outgoing = handle(&a.h_out);
    let incoming = handle(&b.h_in);
    let result = if property == "position" && (outgoing != [0., 0.] || incoming != [0., 0.]) {
        let inv = 1. - eased;
        (0..2)
            .map(|i| {
                inv * inv * inv * av[i]
                    + 3. * inv * inv * eased * (av[i] + outgoing[i])
                    + 3. * inv * eased * eased * (bv[i] + incoming[i])
                    + eased * eased * eased * bv[i]
            })
            .collect::<Vec<_>>()
    } else {
        av.iter()
            .zip(bv)
            .map(|(a, b)| a + (b - a) * eased)
            .collect::<Vec<_>>()
    };
    if result.iter().any(|value| !value.is_finite()) {
        return Err(PropertySampleError::Unavailable);
    }
    Ok(result)
}
