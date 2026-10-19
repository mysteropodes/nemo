//! Typed Motion retiming within the existing document; publication stays with the caller.
//! Real stored effect stacks join the whole-layer pass; render and public admission remain separate.
use crate::object_document::{
    object_deserialize, FrameScope, KeyLockMode, ObjectDocument, OrderTrack, Presence,
    PropertyHolderTarget,
};
use crate::request_receipts::DispatchErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[path = "object_effect_key_shift.rs"]
mod effects;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum ShiftTrackTarget {
    Property {
        target: PropertyHolderTarget,
        property: String,
    },
    LayerOrder {
        context_id: String,
        layer_uid: String,
    },
    ElementOrder {
        context_id: String,
        layer_uid: String,
        frame_scope: FrameScope,
        stroke_id: String,
    },
    TimeRemap {
        context_id: String,
        layer_uid: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SelectedMotionKey {
    pub(super) target: ShiftTrackTarget,
    pub(super) frame: Number,
}
object_deserialize!(SelectedMotionKey {
    #[serde(deserialize_with = "target_map")]
    target: ShiftTrackTarget,
    frame: Number
});
// Preserve original field/duplicate checks in the tagged derive, but never accept positional sequences.
fn target_map<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ShiftTrackTarget, D::Error> {
    struct MapOnly;
    impl<'de> serde::de::Visitor<'de> for MapOnly {
        type Value = ShiftTrackTarget;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a closed Motion key target object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            ShiftTrackTarget::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(MapOnly)
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn finite(frame: &Number) -> Result<f64, DispatchErrorCode> {
    frame.as_f64().filter(|n| n.is_finite()).ok_or_else(invalid)
}
fn delta(dx: i64) -> Result<(), DispatchErrorCode> {
    if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&dx) {
        return Err(invalid());
    }
    Ok(())
}
impl ShiftTrackTarget {
    fn scope(&self) -> (&str, &str) {
        match self {
            Self::Property { target, .. } => (&target.context_id, &target.layer_uid),
            Self::LayerOrder {
                context_id,
                layer_uid,
            }
            | Self::ElementOrder {
                context_id,
                layer_uid,
                ..
            }
            | Self::TimeRemap {
                context_id,
                layer_uid,
            } => (context_id, layer_uid),
        }
    }
}
fn present_mut<T>(slot: &mut Presence<T>) -> Option<&mut T> {
    match slot {
        Presence::Missing => None,
        Presence::Present(value) => Some(value),
    }
}
fn tracks(document: &ObjectDocument) -> Vec<(ShiftTrackTarget, &OrderTrack)> {
    let mut out = Vec::new();
    if let Some(holders) = document.property_holders.value() {
        for holder in holders {
            for property in &holder.properties {
                if let Some(track) = property.track.value() {
                    out.push((
                        ShiftTrackTarget::Property {
                            target: holder.target.clone(),
                            property: property.property.clone(),
                        },
                        track,
                    ));
                }
            }
        }
    }
    if let Some(plan) = document.composition_order.value() {
        for context in &plan.contexts {
            for layer in &context.layers {
                if let Some(track) = layer.order.track.value() {
                    out.push((
                        ShiftTrackTarget::LayerOrder {
                            context_id: context.context_id.clone(),
                            layer_uid: layer.layer_uid.clone(),
                        },
                        track,
                    ));
                }
            }
            for scope in &context.scopes {
                for element in &scope.elements {
                    if let Some(track) = element.order.track.value() {
                        out.push((
                            ShiftTrackTarget::ElementOrder {
                                context_id: context.context_id.clone(),
                                layer_uid: element.layer_uid.clone(),
                                frame_scope: scope.frame_scope.clone(),
                                stroke_id: element.stroke_id.clone(),
                            },
                            track,
                        ));
                    }
                }
            }
        }
    }
    if let Some(set) = document.component_set() {
        for instance in &set.instances {
            if let Some(Some(track)) = instance.time_remap.value() {
                out.push((
                    ShiftTrackTarget::TimeRemap {
                        context_id: instance.context_id.clone(),
                        layer_uid: instance.layer_uid.clone(),
                    },
                    track,
                ));
            }
        }
    }
    out
}
fn track_mut<'a>(
    document: &'a mut ObjectDocument,
    target: &ShiftTrackTarget,
) -> Option<&'a mut OrderTrack> {
    match target {
        ShiftTrackTarget::Property { target, property } => {
            let holder = present_mut(&mut document.property_holders)?
                .iter_mut()
                .find(|holder| &holder.target == target)?;
            present_mut(
                &mut holder
                    .properties
                    .iter_mut()
                    .find(|entry| &entry.property == property)?
                    .track,
            )
        }
        ShiftTrackTarget::LayerOrder {
            context_id,
            layer_uid,
        } => {
            let context = present_mut(&mut document.composition_order)?
                .contexts
                .iter_mut()
                .find(|context| &context.context_id == context_id)?;
            present_mut(
                &mut context
                    .layers
                    .iter_mut()
                    .find(|layer| &layer.layer_uid == layer_uid)?
                    .order
                    .track,
            )
        }
        ShiftTrackTarget::ElementOrder {
            context_id,
            layer_uid,
            frame_scope,
            stroke_id,
        } => {
            let context = present_mut(&mut document.composition_order)?
                .contexts
                .iter_mut()
                .find(|context| &context.context_id == context_id)?;
            let scope = context
                .scopes
                .iter_mut()
                .find(|scope| &scope.frame_scope == frame_scope)?;
            present_mut(
                &mut scope
                    .elements
                    .iter_mut()
                    .find(|element| {
                        &element.layer_uid == layer_uid && &element.stroke_id == stroke_id
                    })?
                    .order
                    .track,
            )
        }
        ShiftTrackTarget::TimeRemap {
            context_id,
            layer_uid,
        } => {
            let set = present_mut(&mut present_mut(&mut document.component_tables)?.components)?;
            present_mut(
                &mut set
                    .instances
                    .iter_mut()
                    .find(|instance| {
                        &instance.context_id == context_id && &instance.layer_uid == layer_uid
                    })?
                    .time_remap,
            )?
            .as_mut()
        }
    }
}
// Root document contexts use its declared layers. Real component contexts carry their own span.
fn extent(document: &ObjectDocument, context: &str, layer: &str) -> Result<u32, DispatchErrorCode> {
    if context.is_empty() || layer.is_empty() {
        return Err(invalid());
    }
    if let Some(component) = document
        .component_set()
        .and_then(|set| set.component(context))
    {
        if component
            .layers
            .iter()
            .any(|entry| entry.layer_uid == layer)
        {
            return (component.total_frames > 0)
                .then_some(component.total_frames)
                .ok_or_else(invalid);
        }
    }
    if !document.layers.iter().any(|entry| entry.layer_uid == layer) {
        return Err(DispatchErrorCode::NotFound);
    }
    let known = context == "scene-root"
        || document.composition_order.value().is_some_and(|plan| {
            plan.contexts
                .iter()
                .any(|entry| entry.context_id == context)
        })
        || document
            .objects
            .iter()
            .any(|record| record.target.context_id == context && record.target.layer_uid == layer)
        || document.property_holders.value().is_some_and(|holders| {
            holders.iter().any(|holder| {
                holder.target.context_id == context && holder.target.layer_uid == layer
            })
        });
    if !known {
        return Err(DispatchErrorCode::NotFound);
    }
    (document.total_frames > 0)
        .then_some(document.total_frames)
        .ok_or_else(invalid)
}
fn retimed(
    track: &OrderTrack,
    selected: Option<&[Number]>,
    dx: i64,
    total: u32,
) -> Result<OrderTrack, DispatchErrorCode> {
    let mut next = track.clone();
    for key in &mut next.keys {
        let before = finite(&key.frame)?;
        if selected.is_none_or(|frames| frames.iter().any(|frame| frame.as_f64() == Some(before))) {
            let after = (before + dx as f64).max(0.0).min(f64::from(total - 1));
            if after != before {
                key.frame = Number::from_f64(after).ok_or_else(invalid)?;
            }
        }
    }
    // Stable sort then exact-frame collapse retains the earliest ORIGINAL-listed key.
    next.keys.sort_by(|a, b| {
        a.frame
            .as_f64()
            .unwrap()
            .partial_cmp(&b.frame.as_f64().unwrap())
            .unwrap()
    });
    next.keys
        .dedup_by(|a, b| a.frame.as_f64() == b.frame.as_f64());
    Ok(next)
}
pub(super) fn validate_selected(
    document: &ObjectDocument,
    selection: &[SelectedMotionKey],
) -> Result<(), DispatchErrorCode> {
    let available = tracks(document);
    for (index, key) in selection.iter().enumerate() {
        let frame = finite(&key.frame)?;
        let (context, layer) = key.target.scope();
        extent(document, context, layer)?;
        if selection[..index]
            .iter()
            .any(|previous| previous.target == key.target && previous.frame.as_f64() == Some(frame))
        {
            return Err(invalid());
        }
        let track = available
            .iter()
            .find(|(target, _)| target == &key.target)
            .map(|(_, track)| *track)
            .ok_or(DispatchErrorCode::NotFound)?;
        if !track
            .keys
            .iter()
            .any(|entry| entry.frame.as_f64() == Some(frame))
        {
            return Err(DispatchErrorCode::NotFound);
        }
    }
    Ok(())
}
pub(super) fn shift_selected(
    document: &mut ObjectDocument,
    selection: &[SelectedMotionKey],
    dx: i64,
) -> Result<bool, DispatchErrorCode> {
    delta(dx)?;
    validate_selected(document, selection)?;
    if selection.is_empty() || dx == 0 {
        return Ok(false);
    }
    let mut replacements = Vec::new();
    for (target, track) in tracks(document) {
        let frames: Vec<Number> = selection
            .iter()
            .filter(|key| key.target == target)
            .map(|key| key.frame.clone())
            .collect();
        if frames.is_empty() {
            continue;
        }
        let (context, layer) = target.scope();
        let next = retimed(track, Some(&frames), dx, extent(document, context, layer)?)?;
        replacements.push((target, next));
    }
    let mut next = document.clone();
    for (target, track) in replacements {
        *track_mut(&mut next, &target).ok_or(DispatchErrorCode::NotFound)? = track;
    }
    *document = next;
    Ok(true)
}
pub(super) fn shift_layer(
    document: &mut ObjectDocument,
    context: &str,
    layer: &str,
    dx: i64,
) -> Result<bool, DispatchErrorCode> {
    delta(dx)?;
    let total = extent(document, context, layer)?;
    if dx == 0 {
        return Ok(false);
    }
    let mut replacements = Vec::new();
    for (target, track) in tracks(document) {
        if target.scope() == (context, layer) && !track.keys.is_empty() {
            replacements.push((target, retimed(track, None, dx, total)?));
        }
    }
    let motion_touched = !replacements.is_empty();
    let mut next = document.clone();
    for (target, track) in replacements {
        *track_mut(&mut next, &target).ok_or(DispatchErrorCode::NotFound)? = track;
    }
    let effects_touched = effects::shift_layer(&mut next, context, layer, dx, total)?;
    if !motion_touched && !effects_touched {
        return Ok(false);
    }
    *document = next;
    Ok(true)
}
pub(super) fn locked_keys(
    document: &ObjectDocument,
    context: &str,
    layer: &str,
    mode: KeyLockMode,
) -> Vec<SelectedMotionKey> {
    let mut out = Vec::new();
    for (target, track) in tracks(document) {
        if target.scope() != (context, layer)
            || matches!(&target, ShiftTrackTarget::TimeRemap { .. })
        {
            continue;
        }
        for key in &track.keys {
            if key
                .lock_to
                .value()
                .is_some_and(|value| *value == Some(mode))
            {
                out.push(SelectedMotionKey {
                    target: target.clone(),
                    frame: key.frame.clone(),
                });
            }
        }
    }
    out
}
#[cfg(test)]
#[path = "../tests/object_layer_shift_motion.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_effect_key_provider.rs"]
mod effect_provider_tests;
