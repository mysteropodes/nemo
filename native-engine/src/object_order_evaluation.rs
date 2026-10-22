//! Partial numeric Order producer; no expression interpreter or renderer policy.
use crate::animation_curve::{evaluate_curve, CurveSample};
use crate::evaluation::object_component_kernels::object_context_resolver::{
    ContextKind, ContextSource,
};
use crate::object_document::{
    AuthoredOrder, FrameScope, FrameScopeKind, OrderContext, OrderCurvePoint,
};
use crate::object_frame_packet::ObjectFrameError;
use crate::revision::ObjectSnapshot;
use serde_json::Number;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct NumericElementOrder {
    source_object_index: usize,
    value: f64,
}
impl NumericElementOrder {
    pub fn source_object_index(&self) -> usize {
        self.source_object_index
    }
    pub fn value(&self) -> f64 {
        self.value
    }
}
#[derive(Debug, Clone)]
pub struct NumericLayerOrder {
    source_layer_index: usize,
    natural_position: u32,
    value: f64,
    explicit: bool,
    elements: Vec<NumericElementOrder>,
}
impl NumericLayerOrder {
    pub fn source_layer_index(&self) -> usize {
        self.source_layer_index
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn explicit(&self) -> bool {
        self.explicit
    }
    pub fn elements(&self) -> &[NumericElementOrder] {
        &self.elements
    }
}
/// Back-to-front sequence pinned by its containing immutable frame packet.
#[derive(Debug, Clone)]
pub struct NumericOrderPlan {
    layers: Vec<NumericLayerOrder>,
}
impl NumericOrderPlan {
    pub fn layers(&self) -> &[NumericLayerOrder] {
        &self.layers
    }
}

fn number(value: &Number) -> Result<f64, ObjectFrameError> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(ObjectFrameError::Unavailable)
}
fn active_expression(order: &AuthoredOrder) -> bool {
    order
        .expression
        .value()
        .is_some_and(|e| e.enabled && !e.code.is_empty())
}
fn has_keys(order: &AuthoredOrder) -> bool {
    order
        .track
        .value()
        .is_some_and(|track| !track.keys.is_empty())
}
fn routing_presence(order: &AuthoredOrder) -> bool {
    !order.static_value.missing() || !order.track.missing()
}
fn samples(points: &[OrderCurvePoint]) -> Result<Vec<CurveSample>, ObjectFrameError> {
    points
        .iter()
        .map(|p| {
            Ok(CurveSample {
                x: number(&p.x)?,
                y: number(&p.y)?,
                tx: p.tx.value().map(number).transpose()?,
                ty: p.ty.value().map(number).transpose()?,
            })
        })
        .collect()
}
fn default_curve() -> Vec<CurveSample> {
    [
        (0.0, 0.0),
        (0.25, 0.156),
        (0.5, 0.5),
        (0.75, 0.844),
        (1.0, 1.0),
    ]
    .into_iter()
    .map(|(x, y)| CurveSample {
        x,
        y,
        tx: None,
        ty: None,
    })
    .collect()
}
pub(super) fn numeric(order: &AuthoredOrder, frame: u32) -> Result<f64, ObjectFrameError> {
    numeric_at(order, f64::from(frame))
}
pub(super) fn numeric_at(order: &AuthoredOrder, frame: f64) -> Result<f64, ObjectFrameError> {
    if !frame.is_finite() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    let Some(track) = order.track.value().filter(|t| !t.keys.is_empty()) else {
        return order
            .static_value
            .value()
            .map_or(Ok(0.0), |v| number(&v[0]));
    };
    let first = &track.keys[0];
    let last = &track.keys[track.keys.len() - 1];
    if frame <= number(&first.frame)? {
        return number(&first.v[0]);
    }
    if frame >= number(&last.frame)? {
        return number(&last.v[0]);
    }
    for pair in track.keys.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let start = number(&a.frame)?;
        let end = number(&b.frame)?;
        if start <= frame && frame < end {
            let value = number(&a.v[0])?;
            if a.hold.value() == Some(&true) {
                return Ok(value);
            }
            let t = (frame - start) / (end - start);
            let curve = match a.curve_points.value() {
                Some(Some(points)) => samples(points)?,
                _ => default_curve(),
            };
            let y = evaluate_curve(&curve, t);
            let result = value + (number(&b.v[0])? - value) * y;
            if result.is_finite() {
                return Ok(result);
            }
            return Err(ObjectFrameError::Unavailable);
        }
    }
    Err(ObjectFrameError::InvalidSource)
}
type SourceIndices<'a> = HashMap<(&'a str, &'a FrameScope, &'a str, &'a str), usize>;

fn elements<E, F>(
    sources: &SourceIndices<'_>,
    context: &OrderContext,
    frame: u32,
    source_frame: Option<u32>,
    exposure: bool,
    layer: &str,
    resolver: &mut F,
) -> Result<Vec<NumericElementOrder>, E>
where
    E: From<ObjectFrameError>,
    F: FnMut(u32, &str, Option<&str>) -> Result<f64, E>,
{
    let Some(source_frame) = source_frame else {
        return Ok(Vec::new());
    };
    let scope = context
        .scopes
        .iter()
        .find(|scope| scope.frame_scope.frame == source_frame);
    let scope = match scope {
        Some(scope) => scope,
        None if exposure
            && !sources.keys().any(|(ctx, scope, uid, _)| {
                *ctx == context.context_id
                    && scope.kind == FrameScopeKind::Authored
                    && scope.frame == source_frame
                    && *uid == layer
            }) =>
        {
            return Ok(Vec::new())
        } // Explicit empty key, never an inferred missing scope.
        None => return Err(ObjectFrameError::Unavailable.into()),
    };
    let mut entries: Vec<_> = scope
        .elements
        .iter()
        .filter(|e| e.layer_uid == layer)
        .collect();
    entries.sort_by_key(|e| e.natural_position);
    let routed = entries.iter().any(|e| routing_presence(&e.order));
    let mut result = Vec::new();
    for entry in entries {
        let value = if routed && active_expression(&entry.order) {
            resolver(source_frame, layer, Some(&entry.stroke_id))?
        } else if routed {
            numeric(&entry.order, frame)?
        } else {
            0.0
        };
        if !value.is_finite() {
            return Err(ObjectFrameError::Unavailable.into());
        }
        let index = sources
            .get(&(
                context.context_id.as_str(),
                &scope.frame_scope,
                entry.layer_uid.as_str(),
                entry.stroke_id.as_str(),
            ))
            .copied()
            .ok_or(ObjectFrameError::InvalidSource)?;
        result.push(NumericElementOrder {
            source_object_index: index,
            value,
        });
    }
    if routed {
        result.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap());
    }
    Ok(result)
}

pub(crate) fn prepare(
    snapshot: &ObjectSnapshot,
    frame: u32,
    sources: &[Option<u32>],
    exposure: bool,
) -> Result<NumericOrderPlan, ObjectFrameError> {
    prepare_context(snapshot, "scene-root", frame, sources, exposure)
}

pub(super) fn prepare_context(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    frame: u32,
    sources: &[Option<u32>],
    exposure: bool,
) -> Result<NumericOrderPlan, ObjectFrameError> {
    prepare_context_with_resolver(snapshot, context_id, frame, sources, exposure, |_, _, _| {
        Err(ObjectFrameError::Unavailable)
    })
}

/// Private native application seam; evaluation never imports a runtime/owner.
/// A resolver may only supply a finite scalar for an already routed holder.
pub(super) fn prepare_with_resolver<E, F>(
    snapshot: &ObjectSnapshot,
    frame: u32,
    source_frames: &[Option<u32>],
    exposure: bool,
    resolver: F,
) -> Result<NumericOrderPlan, E>
where
    E: From<ObjectFrameError>,
    F: FnMut(u32, &str, Option<&str>) -> Result<f64, E>,
{
    prepare_context_with_resolver(
        snapshot,
        "scene-root",
        frame,
        source_frames,
        exposure,
        resolver,
    )
}

pub(super) fn prepare_context_with_resolver<E, F>(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    frame: u32,
    source_frames: &[Option<u32>],
    exposure: bool,
    mut resolver: F,
) -> Result<NumericOrderPlan, E>
where
    E: From<ObjectFrameError>,
    F: FnMut(u32, &str, Option<&str>) -> Result<f64, E>,
{
    let doc = snapshot.document();
    let selected = ContextSource::of_document(doc)
        .context(context_id)
        .map_err(|_| ObjectFrameError::InvalidContext)?;
    let context = doc.composition_order.value().and_then(|plan| {
        plan.contexts
            .iter()
            .find(|context| context.context_id == context_id)
    });
    let Some(context) = context else {
        if selected.kind() == ContextKind::Component {
            return natural_component(&selected, source_frames).map_err(Into::into);
        }
        return Err(ObjectFrameError::Unavailable.into());
    };
    let source: HashMap<_, _> = selected
        .layers()
        .iter()
        .enumerate()
        .map(|(i, l)| (l.layer_uid(), i))
        .collect();
    // Snapshot admission already rejects duplicate full scoped targets. Borrow
    // their immutable identities once; per-element lookup never scans geometry.
    let records = selected.objects();
    let mut sources = SourceIndices::new();
    for (index, record) in records.iter().enumerate() {
        let target = &record.target;
        let key = (
            target.context_id.as_str(),
            &target.frame_scope,
            target.layer_uid.as_str(),
            target.stroke_id.as_str(),
        );
        if sources.insert(key, index).is_some() {
            return Err(ObjectFrameError::InvalidSource.into());
        }
    }
    if let Some(items) = doc.item_records() {
        for (index, target) in items.targets().enumerate() {
            let key = (
                target.context_id.as_str(),
                &target.frame_scope,
                target.layer_uid.as_str(),
                target.stroke_id.as_str(),
            );
            if sources.insert(key, records.len() + index).is_some() {
                return Err(ObjectFrameError::InvalidSource.into());
            }
        }
    }

    let mut entries: Vec<_> = context.layers.iter().collect();
    entries.sort_by_key(|l| l.natural_position);
    let mut layers = Vec::new();
    for layer in entries {
        let source_layer_index = *source
            .get(layer.layer_uid.as_str())
            .ok_or(ObjectFrameError::InvalidSource)?;
        let source_frame = *source_frames
            .get(source_layer_index)
            .ok_or(ObjectFrameError::InvalidSource)?;
        let explicit = has_keys(&layer.order) || !layer.order.static_value.missing();
        let value = if explicit && active_expression(&layer.order) {
            resolver(source_frame.unwrap_or(frame), &layer.layer_uid, None)?
        } else if explicit {
            numeric(&layer.order, frame)?
        } else {
            (context.layers.len() - layer.natural_position as usize) as f64
        };
        if !value.is_finite() {
            return Err(ObjectFrameError::Unavailable.into());
        }
        layers.push(NumericLayerOrder {
            source_layer_index,
            natural_position: layer.natural_position,
            value,
            explicit,
            elements: elements(
                &sources,
                context,
                frame,
                source_frame,
                exposure,
                &layer.layer_uid,
                &mut resolver,
            )?,
        });
    }
    let mut front: Vec<_> = layers
        .iter()
        .filter(|l| !l.explicit)
        .rev()
        .cloned()
        .collect();
    let mut explicit: Vec<_> = layers.into_iter().filter(|l| l.explicit).collect();
    explicit.sort_by(|a, b| {
        a.value
            .partial_cmp(&b.value)
            .unwrap()
            .then(a.natural_position.cmp(&b.natural_position))
    });
    for layer in explicit {
        // Negative ranks all clamp to front. Positive round matches JS Math.round.
        let pos = (layer.value.round() - 1.0).clamp(0.0, front.len() as f64) as usize;
        front.insert(pos, layer);
    }
    front.reverse();
    Ok(NumericOrderPlan { layers: front })
}

// The existing entered-component producer uses its own native layer/stroke
// encounter order when no explicit context OrderPlan exists. This is a read
// projection of that admitted owner, never synthesized persisted order data.
fn natural_component(
    context: &crate::evaluation::object_component_kernels::object_context_resolver::ResolvedContext<
        '_,
    >,
    sources: &[Option<u32>],
) -> Result<NumericOrderPlan, ObjectFrameError> {
    if sources.len() != context.layers().len() {
        return Err(ObjectFrameError::InvalidSource);
    }
    let layer_indices: HashMap<_, _> = context
        .layers()
        .iter()
        .enumerate()
        .map(|(index, layer)| (layer.layer_uid(), index))
        .collect();
    let mut layer_elements: Vec<Vec<NumericElementOrder>> =
        context.layers().iter().map(|_| Vec::new()).collect();
    // Traverse retained geometry once, preserving each layer's encounter order.
    for (source_object_index, record) in context.objects().iter().enumerate() {
        let Some(index) = layer_indices.get(record.target().layer_uid()).copied() else {
            continue;
        };
        if record.target().scope_kind() == &FrameScopeKind::Authored
            && sources[index] == Some(record.target().frame())
        {
            layer_elements[index].push(NumericElementOrder {
                source_object_index,
                value: 0.0,
            });
        }
    }
    let layers = context
        .layers()
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let elements = std::mem::take(&mut layer_elements[index]);
            Ok(NumericLayerOrder {
                source_layer_index: index,
                natural_position: index
                    .try_into()
                    .map_err(|_| ObjectFrameError::InvalidSource)?,
                value: (context.layers().len() - index) as f64,
                explicit: false,
                elements,
            })
        })
        .collect::<Result<Vec<_>, ObjectFrameError>>()?;
    Ok(NumericOrderPlan { layers })
}
