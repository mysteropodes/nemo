//! Sealed generic property edits through the existing revision owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, ObjectDocument, OrderCurvePoint, OrderExpression, OrderKey, OrderTrack,
    Presence, PropertyHolder, PropertyHolderTarget, PropertyRecord,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[path = "object_property_bake_edit.rs"]
mod bake_edit;
#[path = "object_property_curve_edits.rs"]
mod curves;
#[path = "object_property_preset_apply.rs"]
mod preset_apply;
#[path = "object_property_track_edit.rs"]
mod track_edit;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum PropertyAction {
    // Private producer only. Public property intents keep refusing expression edits.
    #[serde(skip_deserializing)]
    SetExpression {
        expression: Option<OrderExpression>,
    },
    SetValue {
        frame: Number,
        values: Vec<Number>,
    },
    SetKey {
        frame: Number,
        values: Vec<Number>,
        #[serde(default, rename = "curvePoints")]
        curve_points: Presence<Option<Vec<OrderCurvePoint>>>,
    },
    RemoveKey {
        frame: Number,
    },
    SetAnimated {
        frame: Number,
        animated: bool,
    },
}
#[derive(Debug)]
struct PropertyPayload {
    command: String,
    target: PropertyHolderTarget,
    property: String,
    action: PropertyAction,
}
object_deserialize!(PropertyPayload {
    command: String,
    target: PropertyHolderTarget,
    property: String,
    #[serde(deserialize_with = "deserialize_action")]
    action: PropertyAction
});

#[derive(Debug)]
struct ExpressionPayload {
    target: PropertyHolderTarget,
    property: String,
    expression: Presence<Option<OrderExpression>>,
}
object_deserialize!(ExpressionPayload {
    target: PropertyHolderTarget,
    property: String,
    #[serde(default)] expression: Presence<Option<OrderExpression>>
});

// Internally tagged enum derives also admit positional sequences. Require a
// map before the derive sees original fields, preserving duplicate rejection.
fn deserialize_action<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct MapOnly<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for MapOnly<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a property action object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(MapOnly(std::marker::PhantomData))
}

/// The origin and proposed document cannot be forged by callers.
#[derive(Debug, Clone)]
pub(crate) struct PreparedPropertyEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}
impl PreparedPropertyEdit {
    /// Seal a document built from `origin` by a sibling preparer; `changed` is derived, never supplied.
    pub(super) fn sealed(origin: ObjectSnapshot, document: ObjectDocument) -> Self {
        let changed = &document != origin.document();
        Self {
            origin,
            document,
            changed,
        }
    }
    /// The proposed document, for a caller that chains further edits before sealing its own candidate.
    pub(super) fn into_document(self) -> ObjectDocument {
        self.document
    }
}
fn finite(n: &Number) -> Result<f64, DispatchErrorCode> {
    n.as_f64()
        .filter(|v| v.is_finite())
        .ok_or(DispatchErrorCode::InvalidRequest)
}
// Motion compares numbers in the JavaScript numeric domain. Preserve the original
// JSON spelling for equal values so a no-op cannot allocate history or erase redo.
fn preserve_number(before: &Number, after: &mut Number) {
    if before.as_f64() == after.as_f64() {
        *after = before.clone();
    }
}
fn preserve_values(before: &[Number], after: &mut [Number]) {
    for (before, after) in before.iter().zip(after) {
        preserve_number(before, after);
    }
}
fn preserve_curve(before: &[OrderCurvePoint], after: &mut [OrderCurvePoint]) {
    for (before, after) in before.iter().zip(after) {
        preserve_number(&before.x, &mut after.x);
        preserve_number(&before.y, &mut after.y);
        if let (Presence::Present(before), Presence::Present(after)) = (&before.tx, &mut after.tx) {
            preserve_number(before, after);
        }
        if let (Presence::Present(before), Presence::Present(after)) = (&before.ty, &mut after.ty) {
            preserve_number(before, after);
        }
    }
}
fn default_curve() -> Vec<OrderCurvePoint> {
    [(0., 0.), (0.25, 0.156), (0.5, 0.5), (0.75, 0.844), (1., 1.)]
        .into_iter()
        .map(|(x, y)| OrderCurvePoint {
            x: Number::from_f64(x).unwrap(),
            y: Number::from_f64(y).unwrap(),
            tx: Presence::Missing,
            ty: Presence::Missing,
        })
        .collect()
}
fn keyed(record: &PropertyRecord) -> bool {
    record.track.value().is_some_and(|t| !t.keys.is_empty())
}
fn track(record: &mut PropertyRecord) -> &mut OrderTrack {
    if record.track.missing() {
        record.track = Presence::Present(OrderTrack { keys: vec![] });
    }
    match &mut record.track {
        Presence::Present(t) => t,
        Presence::Missing => unreachable!(),
    }
}
fn set_key(
    record: &mut PropertyRecord,
    frame: Number,
    values: Vec<Number>,
    curve: Option<Vec<OrderCurvePoint>>,
) {
    track(record).scalar_intent_key(frame, values, curve);
}
impl OrderTrack {
    pub(crate) fn scalar_intent_key(
        &mut self,
        frame: Number,
        mut values: Vec<Number>,
        curve: Option<Vec<OrderCurvePoint>>,
    ) {
        let keys = &mut self.keys;
        let at = frame.as_f64().unwrap();
        if let Some(key) = keys.iter_mut().find(|key| key.frame.as_f64() == Some(at)) {
            preserve_values(&key.v, &mut values);
            key.v = values;
            if let Some(mut curve) = curve {
                if let Some(Some(before)) = key.curve_points.value() {
                    preserve_curve(before, &mut curve);
                }
                key.curve_points = Presence::Present(Some(curve));
            }
        } else {
            keys.push(OrderKey {
                frame,
                v: values,
                hold: Presence::Missing,
                lock_to: Presence::Missing,
                curve_points: Presence::Present(Some(curve.unwrap_or_else(default_curve))),
                h_in: Presence::Present(vec![0.into(), 0.into()]),
                h_out: Presence::Present(vec![0.into(), 0.into()]),
            });
            keys.sort_by(|a, b| {
                a.frame
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b.frame.as_f64().unwrap())
            });
        }
    }
}
impl ObjectFillOwner {
    /// Private authoring producer over a retained origin; the existing history
    /// owner commits the sealed candidate. No public expression writer is admitted.
    /// Null removes the expression; an empty string preserves its authored presence.
    #[allow(dead_code)]
    pub(crate) fn prepare_property_expression_edit_json(
        origin: ObjectSnapshot,
        bytes: &[u8],
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        // The private envelope can carry every admitted 4096-byte code string,
        // even when JSON escapes it. Public transport keeps its own 4096-byte cap.
        if bytes.len() > 32768 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: ExpressionPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let Presence::Present(expression) = payload.expression else {
            return Err(DispatchErrorCode::InvalidRequest);
        };
        let document = origin.document().clone();
        Self::apply_property_edit(
            origin,
            document,
            PropertyPayload {
                command: "object.property.edit".into(),
                target: payload.target,
                property: payload.property,
                action: PropertyAction::SetExpression { expression },
            },
        )
    }
    pub(crate) fn prepare_property_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let document = origin.document().clone();
        Self::apply_property_edit_json(origin, document, bytes)
    }
    /// The same strict edit applied to `document`, a working copy of `origin` (the snapshot the
    /// candidate is sealed to), so a chain of edits stays one atomic candidate.
    pub(super) fn apply_property_edit_json(
        origin: ObjectSnapshot,
        document: ObjectDocument,
        bytes: &[u8],
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: PropertyPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Self::apply_property_edit(origin, document, payload)
    }
    fn apply_property_edit(
        origin: ObjectSnapshot,
        mut document: ObjectDocument,
        payload: PropertyPayload,
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        if payload.command != "object.property.edit" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if matches!(payload.property.as_str(), "order" | "timeRemap") {
            return Err(DispatchErrorCode::Unavailable);
        }
        let mut holders = document
            .property_holders
            .value()
            .cloned()
            .unwrap_or_default();
        let index = holders.iter().position(|h| h.target() == &payload.target);
        let mut holder = index
            .map(|i| holders[i].clone())
            .unwrap_or_else(|| PropertyHolder {
                mesh_source: Presence::Missing,
                target: payload.target.clone(),
                controls: Presence::Missing,
                properties: vec![],
            });
        // Validate even a no-op selector; absent targets cannot invent a holder.
        PropertyHolder::validate_with_items(
            &Presence::Present(vec![holder.clone()]),
            &document.layers,
            &document.objects,
            document.composition_order.value(),
            document.component_tables(),
            document.items.value(),
            document.image_meshes.value(),
        )
        .map_err(|_| DispatchErrorCode::NotFound)?;
        let metadata = holder
            .metadata_in_document(&document, &payload.property)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let record_index = holder
            .properties
            .iter()
            .position(|r| r.property == payload.property);
        if record_index.is_none() && PropertyHolder::mesh_is_unbound(&document, &payload.target) {
            return Err(DispatchErrorCode::Unavailable);
        }
        let mut record = record_index
            .map(|i| holder.properties[i].clone())
            .unwrap_or_else(|| PropertyRecord {
                property: payload.property.clone(),
                static_value: Presence::Missing,
                track: Presence::Missing,
                expression: Presence::Missing,
            });
        let before = record.clone();
        match payload.action {
            PropertyAction::SetExpression { expression } => {
                record.expression = expression.map_or(Presence::Missing, Presence::Present);
            }
            PropertyAction::SetValue { frame, mut values } => {
                finite(&frame)?;
                if values.len() != metadata.dimension() {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                for value in &values {
                    finite(value)?;
                }
                if keyed(&record) {
                    set_key(&mut record, frame, values, None);
                } else {
                    if let Some(before) = record.static_value.value() {
                        preserve_values(before, &mut values);
                    }
                    record.static_value = Presence::Present(values);
                }
            }
            PropertyAction::SetKey {
                frame,
                values,
                curve_points,
            } => {
                finite(&frame)?;
                if !metadata.animatable() {
                    return Err(DispatchErrorCode::Unavailable);
                }
                if values.len() != metadata.dimension() {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                for value in &values {
                    finite(value)?;
                }
                set_key(
                    &mut record,
                    frame,
                    values,
                    curve_points.value().cloned().flatten(),
                );
            }
            PropertyAction::RemoveKey { frame } => {
                let at = finite(&frame)?;
                if let Presence::Present(t) = &mut record.track {
                    t.keys.retain(|key| key.frame.as_f64() != Some(at));
                }
            }
            PropertyAction::SetAnimated { frame, animated } => {
                let at = finite(&frame)?;
                if !metadata.animatable() {
                    return Err(DispatchErrorCode::Unavailable);
                }
                if keyed(&record) != animated {
                    // Active expression sampling must use the separately admitted C02 evaluator.
                    // Never accept a UI-supplied effective value as a second state authority.
                    if record
                        .expression
                        .value()
                        .is_some_and(|e| e.enabled && !e.code.is_empty())
                    {
                        return Err(DispatchErrorCode::Unavailable);
                    }
                    let values = origin
                        .sample_property_raw(&payload.target, &payload.property, at)?
                        .into_iter()
                        .map(|v| Number::from_f64(v).ok_or(DispatchErrorCode::Unavailable))
                        .collect::<Result<Vec<_>, _>>()?;
                    if animated {
                        set_key(&mut record, frame, values, None);
                    } else {
                        track(&mut record).keys.clear();
                        record.static_value = Presence::Present(values);
                    }
                }
            }
        }
        if before != record {
            if let Some(i) = record_index {
                holder.properties[i] = record;
            } else {
                holder.properties.push(record);
            }
            if let Some(i) = index {
                holders[i] = holder;
            } else {
                holders.push(holder);
            }
            document.property_holders = Presence::Present(holders);
            ObjectSnapshot::validate_drawing(&document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        let changed = &document != origin.document();
        Ok(PreparedPropertyEdit {
            origin,
            document,
            changed,
        })
    }
    pub(crate) fn commit_property_edit(
        &mut self,
        candidate: PreparedPropertyEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed {
            return Ok(current.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}

// Text selector holders share the exact existing key mutation/number preservation.
impl PropertyRecord {
    pub(crate) fn scalar_intent_keyed(&self) -> bool {
        keyed(self)
    }
    pub(crate) fn scalar_intent_value(
        &mut self,
        frame: Number,
        value: Number,
        force_key: bool,
        curve: Option<Vec<OrderCurvePoint>>,
    ) {
        let mut values = vec![value];
        if force_key || keyed(self) {
            set_key(self, frame, values, curve);
        } else {
            if let Some(before) = self.static_value.value() {
                preserve_values(before, &mut values);
            }
            self.static_value = Presence::Present(values);
        }
    }
    pub(crate) fn scalar_intent_remove(&mut self, at: f64) {
        if let Presence::Present(t) = &mut self.track {
            t.keys.retain(|k| k.frame.as_f64() != Some(at));
        }
    }
    pub(crate) fn scalar_intent_freeze(&mut self, value: Number) {
        track(self).keys.clear();
        let mut values = vec![value];
        if let Some(before) = self.static_value.value() {
            preserve_values(before, &mut values);
        }
        self.static_value = Presence::Present(values);
    }
}
