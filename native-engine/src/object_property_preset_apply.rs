//! G39 (#2376): a Motion preset applied to one layer as ONE candidate, hence one history entry.
//!
//! Baseline `motion-preset-picker.js` (@1065e9d8) replaces the target layer's whole `motion`/`motionStatic` with
//! the preset's copy. Natively each listed transform property is replaced in the layer's property holder: its
//! authored keys (frame, value, hold, lock, curve points, handles) and its static value. A property the preset
//! does not animate or fix is cleared, exactly as the replaced object would have dropped it. Expressions are
//! never touched (the baseline keeps them outside `motion`), and `order`/`timeRemap` have other owners.
//! The payload is the full authored track, so nothing is sampled, inferred or kept in a second store.
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::{
    object_deserialize, OrderKey, OrderTrack, Presence, PropertyHolder, PropertyHolderTarget,
    PropertyRecord,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

type R<T> = Result<T, DispatchErrorCode>;
/// Authored keys with curve points make a preset larger than a plain property edit.
const MAX_BYTES: usize = 65_536;
const MAX_KEYS: usize = 512;
/// The layer transform the baseline preset stores (`motion.js` `PROPS` without the `order` row).
const TRANSFORM: [&str; 5] = ["position", "anchor", "rotation", "scale", "opacity"];

#[derive(Debug)]
struct Entry {
    property: String,
    keys: Vec<OrderKey>,
    static_value: Presence<Vec<Number>>,
}
object_deserialize!(Entry {
    property: String,
    keys: Vec<OrderKey>,
    #[serde(default, rename = "static")] static_value: Presence<Vec<Number>>
});
#[derive(Debug)]
struct Payload {
    command: String,
    target: PropertyHolderTarget,
    properties: Vec<Entry>,
}
object_deserialize!(Payload {
    command: String,
    target: PropertyHolderTarget,
    properties: Vec<Entry>
});

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn finite(n: &Number) -> R<f64> {
    n.as_f64().filter(|v| v.is_finite()).ok_or_else(invalid)
}
/// Equal JavaScript numbers are equal values whatever their JSON spelling (`1` and `1.0`).
fn same_json(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value::{Array, Number as N, Object};
    match (a, b) {
        (N(a), N(b)) => a.as_f64() == b.as_f64(),
        (Array(a), Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_json(a, b)),
        (Object(a), Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| same_json(v, w)))
        }
        _ => a == b,
    }
}
fn same_record(a: &PropertyRecord, b: &PropertyRecord) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => same_json(&a, &b),
        _ => false,
    }
}
fn empty(record: &PropertyRecord) -> bool {
    record.static_value.missing()
        && record.expression.missing()
        && record.track.value().map_or(true, |t| t.keys.is_empty())
}
fn check_vector(values: &[Number], dimension: usize) -> R<()> {
    if values.len() != dimension {
        return Err(invalid());
    }
    values.iter().try_for_each(|n| finite(n).map(|_| ()))
}
fn check_keys(keys: &[OrderKey], dimension: usize) -> R<()> {
    if keys.len() > MAX_KEYS {
        return Err(invalid());
    }
    let mut previous = f64::NEG_INFINITY;
    for key in keys {
        let at = finite(&key.frame)?;
        if at <= previous {
            return Err(invalid()); // strictly increasing: one key per frame
        }
        previous = at;
        check_vector(&key.v, dimension)?;
        for handle in [&key.h_in, &key.h_out] {
            if let Presence::Present(h) = handle {
                check_vector(h, 2)?;
            }
        }
    }
    Ok(())
}

impl ObjectFillOwner {
    /// Prepare the whole preset against the current snapshot: every property resolves and validates before
    /// any record is replaced, and the sealed candidate carries all of them or none.
    pub(crate) fn prepare_property_preset_apply_json(
        &self,
        bytes: &[u8],
    ) -> R<PreparedPropertyEdit> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid());
        }
        let payload: Payload = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if payload.command != "object.property.preset.apply"
            || payload.properties.is_empty()
            || payload.properties.len() > TRANSFORM.len()
        {
            return Err(invalid());
        }
        let mut seen: Vec<&str> = Vec::new();
        for entry in &payload.properties {
            let name = entry.property.as_str();
            if !TRANSFORM.contains(&name) || seen.contains(&name) {
                return Err(invalid());
            }
            seen.push(name);
        }
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let original = document
            .property_holders
            .value()
            .cloned()
            .unwrap_or_default();
        let mut holders = original.clone();
        let index = holders.iter().position(|h| h.target() == &payload.target);
        let mut holder = index
            .map(|i| holders[i].clone())
            .unwrap_or_else(|| PropertyHolder {
                mesh_source: Presence::Missing,
                target: payload.target.clone(),
                controls: Presence::Missing,
                properties: vec![],
            });
        let was_empty = holder.properties.is_empty();
        // A layer that does not exist cannot gain a holder.
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
        for entry in payload.properties {
            let metadata = holder
                .metadata_in_document(&document, &entry.property)
                .ok_or(DispatchErrorCode::Unavailable)?;
            if !metadata.animatable() {
                return Err(DispatchErrorCode::Unavailable);
            }
            check_keys(&entry.keys, metadata.dimension())?;
            if let Presence::Present(fixed) = &entry.static_value {
                check_vector(fixed, metadata.dimension())?;
            }
            let at = holder
                .properties
                .iter()
                .position(|r| r.property == entry.property);
            let before = at.map(|i| holder.properties[i].clone());
            let mut record = before.clone().unwrap_or(PropertyRecord {
                property: entry.property.clone(),
                static_value: Presence::Missing,
                track: Presence::Missing,
                expression: Presence::Missing,
            });
            record.track = if entry.keys.is_empty() {
                Presence::Missing
            } else {
                Presence::Present(OrderTrack { keys: entry.keys })
            };
            record.static_value = entry.static_value;
            if before.as_ref().is_some_and(|b| same_record(b, &record)) {
                continue; // byte-preserving no-op: no revision, no history entry
            }
            match (at, empty(&record)) {
                (Some(i), true) => {
                    holder.properties.remove(i);
                }
                (Some(i), false) => holder.properties[i] = record,
                (None, true) => {}
                (None, false) => holder.properties.push(record),
            }
        }
        let vacated = !was_empty
            && holder.properties.is_empty()
            && holder.controls.missing()
            && holder.mesh_source.missing();
        match index {
            Some(i) if vacated => {
                holders.remove(i);
            }
            Some(i) => holders[i] = holder,
            None if !holder.properties.is_empty() => holders.push(holder),
            None => {}
        }
        if holders != original {
            document.property_holders = Presence::Present(holders);
        }
        if &document != origin.document() {
            ObjectSnapshot::validate_drawing(&document).map_err(|_| invalid())?;
        }
        Ok(PreparedPropertyEdit::sealed(origin, document))
    }
}
