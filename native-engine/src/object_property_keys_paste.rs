//! #2524 / #2377: paste copied Motion keys at the playhead as ONE candidate, hence one history entry.
//!
//! Baseline `motion.js` `copySelectedKeys` / `pasteKeys` (@1065e9d8): the copy records, per selected key, its property, its offset
//! from the earliest copied key (`dt`), its value, its hold flag, its curve points and the holder it came from; the paste puts
//! each at `playhead + dt` on that holder and overwrites the value, hold and curve of a key already there (its handles stay),
//! while a new key gets zero handles. A key whose landing frame leaves `0..totalFrames - 1` is skipped without a message.
//! The packet is built by the adapter from the ordinary native `authoredKeys` read; nothing is sampled or inferred here, and
//! every track resolves and validates before any record is written, so a refusal publishes nothing.
use crate::commands::object_fill_owner::property_edit::default_curve;
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::{
    object_deserialize, OrderCurvePoint, OrderKey, OrderTrack, Presence, PropertyHolder,
    PropertyHolderTarget, PropertyRecord,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

type R<T> = Result<T, DispatchErrorCode>;
/// Authored keys with curve points make a packet larger than a plain property edit.
const MAX_BYTES: usize = 65_536;
const MAX_TRACKS: usize = 128;
const MAX_KEYS: usize = 512;

#[derive(Debug)]
struct PasteKey {
    dt: Number,
    v: Vec<Number>,
    hold: Presence<bool>,
    curve_points: Presence<Option<Vec<OrderCurvePoint>>>,
}
object_deserialize!(PasteKey {
    dt: Number,
    v: Vec<Number>,
    #[serde(default)] hold: Presence<bool>,
    #[serde(default)] curve_points: Presence<Option<Vec<OrderCurvePoint>>>
});
#[derive(Debug)]
struct PasteTrack {
    target: PropertyHolderTarget,
    property: String,
    keys: Vec<PasteKey>,
}
object_deserialize!(PasteTrack {
    target: PropertyHolderTarget,
    property: String,
    keys: Vec<PasteKey>
});
#[derive(Debug)]
struct Payload {
    command: String,
    frame: Number,
    tracks: Vec<PasteTrack>,
}
object_deserialize!(Payload {
    command: String,
    frame: Number,
    tracks: Vec<PasteTrack>
});

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn finite(n: &Number) -> R<f64> {
    n.as_f64().filter(|v| v.is_finite()).ok_or_else(invalid)
}
/// The frame a copied key lands on: the playhead plus its offset, an integer spelling when the sum is whole.
fn landing(at: f64, dt: f64) -> Number {
    let sum = at + dt;
    if sum.fract() == 0.0 && sum.abs() <= 9_007_199_254_740_991.0 {
        Number::from(sum as i64)
    } else {
        Number::from_f64(sum).expect("a finite sum")
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_property_keys_paste_json(&self, bytes: &[u8]) -> R<PreparedPropertyEdit> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid());
        }
        let payload: Payload = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        let at = finite(&payload.frame)?;
        if payload.command != "object.property.keys.paste"
            || at < 0.0
            || payload.tracks.is_empty()
            || payload.tracks.len() > MAX_TRACKS
            || payload.tracks.iter().any(|t| t.keys.is_empty())
            || payload.tracks.iter().map(|t| t.keys.len()).sum::<usize>() > MAX_KEYS
        {
            return Err(invalid());
        }
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let last = f64::from(document.total_frames()) - 1.0;
        let original = document
            .property_holders
            .value()
            .cloned()
            .unwrap_or_default();
        let mut holders = original.clone();
        for track in &payload.tracks {
            // `order` and `timeRemap` have other owners, exactly as in the track edit.
            if matches!(track.property.as_str(), "order" | "timeRemap") {
                return Err(DispatchErrorCode::Unavailable);
            }
            let index = holders.iter().position(|h| h.target() == &track.target);
            let mut holder = index
                .map(|i| holders[i].clone())
                .unwrap_or_else(|| PropertyHolder {
                    mesh_source: Presence::Missing,
                    target: track.target.clone(),
                    controls: Presence::Missing,
                    properties: vec![],
                });
            // A layer or element that does not exist cannot gain a holder.
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
                .metadata_in_document(&document, &track.property)
                .ok_or(DispatchErrorCode::Unavailable)?;
            if !metadata.animatable() {
                return Err(DispatchErrorCode::Unavailable);
            }
            let dimension = metadata.dimension();
            let slot = match holder
                .properties
                .iter()
                .position(|r| r.property == track.property)
            {
                Some(i) => i,
                None => {
                    holder.properties.push(PropertyRecord {
                        property: track.property.clone(),
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    });
                    holder.properties.len() - 1
                }
            };
            let mut keys: Vec<OrderKey> = holder.properties[slot]
                .track
                .value()
                .map(|t| t.keys.clone())
                .unwrap_or_default();
            let mut landed = false;
            for key in &track.keys {
                let dt = finite(&key.dt)?;
                if key.v.len() != dimension || key.v.iter().any(|n| finite(n).is_err()) {
                    return Err(invalid());
                }
                let to = at + dt;
                if to < 0.0 || to > last {
                    continue; // outside the composition: skipped, as the baseline did
                }
                let frame = landing(at, dt);
                let curve = match &key.curve_points {
                    Presence::Present(Some(points)) => Presence::Present(Some(points.clone())),
                    _ => Presence::Present(Some(default_curve())),
                };
                if let Some(existing) = keys.iter_mut().find(|k| k.frame.as_f64() == Some(to)) {
                    existing.v = key.v.clone();
                    existing.hold = match (key.hold.value(), existing.hold.missing()) {
                        (Some(true), _) => Presence::Present(true),
                        (_, false) => Presence::Present(false),
                        (_, true) => Presence::Missing,
                    };
                    existing.curve_points = curve;
                } else {
                    keys.push(OrderKey {
                        frame,
                        v: key.v.clone(),
                        hold: match key.hold.value() {
                            Some(true) => Presence::Present(true),
                            _ => Presence::Missing,
                        },
                        lock_to: Presence::Missing,
                        curve_points: curve,
                        h_in: Presence::Present(vec![0.into(), 0.into()]),
                        h_out: Presence::Present(vec![0.into(), 0.into()]),
                        color: Presence::Missing,
                    });
                }
                landed = true;
            }
            if !landed {
                continue; // every key fell outside the composition: this track changes nothing
            }
            keys.sort_by(|a, b| {
                a.frame
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b.frame.as_f64().unwrap())
            });
            holder.properties[slot].track = Presence::Present(OrderTrack { keys });
            match index {
                Some(i) => holders[i] = holder,
                None => holders.push(holder),
            }
        }
        if holders != original {
            document.property_holders = Presence::Present(holders);
            ObjectSnapshot::validate_drawing(&document).map_err(|_| invalid())?;
        }
        Ok(PreparedPropertyEdit::sealed(origin, document))
    }
}
