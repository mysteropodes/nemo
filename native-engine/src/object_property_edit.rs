//! Sealed generic property edits through the existing revision owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, ObjectDocument, OrderCurvePoint, OrderKey, OrderTrack, Presence,
    PropertyHolder, PropertyHolderTarget, PropertyRecord,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum PropertyAction {
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
    action: PropertyAction
});

/// The origin and proposed document cannot be forged by callers.
#[derive(Debug, Clone)]
pub(crate) struct PreparedPropertyEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}
fn finite(n: &Number) -> Result<f64, DispatchErrorCode> {
    n.as_f64()
        .filter(|v| v.is_finite())
        .ok_or(DispatchErrorCode::InvalidRequest)
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
    let keys = &mut track(record).keys;
    let at = frame.as_f64().unwrap();
    if let Some(key) = keys.iter_mut().find(|key| key.frame.as_f64() == Some(at)) {
        key.v = values;
        if let Some(curve) = curve {
            key.curve_points = Presence::Present(Some(curve));
        }
    } else {
        keys.push(OrderKey {
            frame,
            v: values,
            hold: Presence::Missing,
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
impl ObjectFillOwner {
    pub(crate) fn prepare_property_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: PropertyPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.property.edit" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if matches!(payload.property.as_str(), "order" | "timeRemap") {
            return Err(DispatchErrorCode::Unavailable);
        }
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let mut holders = document
            .property_holders
            .value()
            .cloned()
            .unwrap_or_default();
        let index = holders.iter().position(|h| h.target() == &payload.target);
        let mut holder = index
            .map(|i| holders[i].clone())
            .unwrap_or_else(|| PropertyHolder {
                target: payload.target.clone(),
                controls: Presence::Missing,
                properties: vec![],
            });
        // Validate even a no-op selector; absent targets cannot invent a holder.
        crate::object_document::validate_property_holders(
            &Presence::Present(vec![holder.clone()]),
            &document.layers,
            &document.objects,
            document.composition_order.value(),
        )
        .map_err(|_| DispatchErrorCode::NotFound)?;
        let metadata = holder
            .metadata(&payload.property)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let record_index = holder
            .properties
            .iter()
            .position(|r| r.property == payload.property);
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
            PropertyAction::SetValue { frame, values } => {
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
