//! Sealed widget transactions (F19) through the existing revision owner: creation, write-from-local and
//! settings. Nothing here is a second writer, a control catalog or a value store.
//!
//! - Creation reuses the one layer-create path (`prepare_layer_create_json`: unique monotonic layer id,
//!   composition-order insertion in every context) and then, in the SAME candidate, gives the new layer
//!   its widget record, the baseline colour and its layer-level `PropertyHolder`: one number
//!   `ExpressionControl` per axis (joystick `X`,`Y`; slider `Value`, defaults of `rig-widget.js`
//!   `DEFAULTS`) with a static rest value seeded, exactly what `addWidgetLayer` leaves behind. One
//!   commit, one history entry; undo removes the layer, the controls and the record together.
//! - Write-from-local asks the read-queries facade (`ObjectSnapshot::widget_write_values`, the L03-B
//!   pad kernel) for the control values of a drag, then issues each one through the existing property
//!   edit (`SetKey` at the playhead when the control track is keyed, `SetValue` otherwise), chained on
//!   one working document and published once. An axis whose control is missing, not a number control or
//!   unaddressable refuses the whole write: no partial publication.
//! - Settings edit the presentation fields (`pos`, `size`, axis `min` `max` `rest` `label`) with the
//!   baseline panel rules (size components never below 8); control values are never touched; an edit
//!   that changes nothing preserves bytes, history and redo.
//!
//! Deliberate differences from the baseline, each pinned by a test: the kind must be `joystick` or
//! `slider` (the baseline falls back to a joystick), the caller supplies the layer name and the world
//! anchor (the baseline reads them from UI state), numbers must be finite and at most 1e9 in magnitude
//! (the baseline has no bound), and `-0` is stored as `0` (the baseline JSON does the same).
//!
//! Not provided: hit-testing, the overlay, the locked-layer gesture escape (F06), duplicate and
//! transplant of the layer-level holder, the scene/export zero-pixel join and the JS adapter.
use crate::commands::{ObjectFillOwner, PreparedLayerCreate};
use crate::object_document::{
    object_deserialize, AxisBinding, ExpressionControl, ObjectDocument, Presence, PropertyHolder,
    PropertyHolderTarget, PropertyRecord, WidgetAxis, WidgetLayerKind, WidgetRecord,
};
use crate::object_snapshot::widget_read::WidgetIntent;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Number};

#[path = "object_image_mesh_edit.rs"]
mod object_image_mesh_edit;
pub(crate) use object_image_mesh_edit::PreparedImageMeshEdit;
#[path = "object_widget_settings.rs"]
mod settings;

const MAX_PAYLOAD_BYTES: usize = 4096;
const ROOT_CONTEXT: &str = "scene-root";
const WIDGET_COLOR: &str = "#ffb86c";
/// Leaf guard (not a baseline rule): keeps every product of the pad arithmetic finite.
const MAX_MAGNITUDE: f64 = 1.0e9;

#[derive(Debug)]
struct CreatePayload {
    command: String,
    kind: String,
    name: String,
    pos: [Number; 2],
}
object_deserialize!(CreatePayload {
    command: String,
    kind: String,
    name: String,
    pos: [Number; 2]
});

#[derive(Debug)]
struct WriteTo {
    kind: String,
    point: Presence<[Number; 2]>,
    anchor: Presence<[Number; 2]>,
    angle_degrees: Presence<Number>,
}
object_deserialize!(WriteTo {
    kind: String,
    #[serde(default)] point: Presence<[Number; 2]>,
    #[serde(default)] anchor: Presence<[Number; 2]>,
    #[serde(default)] angle_degrees: Presence<Number>
});

#[derive(Debug)]
struct WritePayload {
    command: String,
    layer_uid: String,
    frame: Number,
    to: WriteTo,
}
object_deserialize!(WritePayload {
    command: String,
    layer_uid: String,
    frame: Number,
    to: WriteTo
});

/// The origin and proposed document cannot be forged by callers.
#[derive(Debug, Clone)]
pub(crate) struct PreparedWidgetEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

fn bounded(number: &Number) -> Result<f64, DispatchErrorCode> {
    number
        .as_f64()
        .filter(|value| value.is_finite() && value.abs() <= MAX_MAGNITUDE)
        .ok_or(DispatchErrorCode::InvalidRequest)
}

/// A computed value spelled like `JSON.stringify` would: integral values without a fraction, `-0` as `0`.
fn js_number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Number::from(0i64));
    }
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Some(Number::from(value as i64));
    }
    Number::from_f64(value)
}

/// Equal values keep their authored spelling so a no-op cannot rewrite bytes.
fn keep_equal(before: &Number, after: Number) -> Number {
    if before.as_f64() == after.as_f64() {
        before.clone()
    } else {
        after
    }
}

fn parse_payload<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)
}

fn key_stem(layer_uid: &str) -> String {
    layer_uid
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The layer-level holder that declares `key` (first holder in order, like the axis binding).
fn holder_target(
    document: &ObjectDocument,
    layer_uid: &str,
    key: &str,
) -> Option<PropertyHolderTarget> {
    document
        .property_holders()
        .unwrap_or(&[])
        .iter()
        .filter(|holder| holder.target.layer_uid == layer_uid && holder.target.stroke_id.missing())
        .find(|holder| {
            holder
                .controls
                .value()
                .is_some_and(|controls| controls.iter().any(|control| control.key == key))
        })
        .map(|holder| holder.target.clone())
}

fn is_animated(document: &ObjectDocument, target: &PropertyHolderTarget, key: &str) -> bool {
    document
        .property_holders()
        .unwrap_or(&[])
        .iter()
        .filter(|holder| &holder.target == target)
        .flat_map(|holder| holder.properties.iter())
        .any(|record| {
            record.property == key
                && record
                    .track
                    .value()
                    .is_some_and(|track| !track.keys.is_empty())
        })
}

fn pair(value: &Presence<[Number; 2]>) -> Result<Option<[f64; 2]>, DispatchErrorCode> {
    match value.value() {
        None => Ok(None),
        Some(numbers) => Ok(Some([bounded(&numbers[0])?, bounded(&numbers[1])?])),
    }
}

fn intent_of(to: &WriteTo) -> Result<WidgetIntent, DispatchErrorCode> {
    let point = pair(&to.point)?;
    let anchor = pair(&to.anchor)?;
    let angle = match to.angle_degrees.value() {
        None => None,
        Some(number) => Some(bounded(number)?),
    };
    match (to.kind.as_str(), point, anchor, angle) {
        ("local", Some(point), None, None) => Ok(WidgetIntent::Local { point }),
        ("world", Some(point), Some(anchor), Some(angle_degrees)) => Ok(WidgetIntent::World {
            point,
            anchor,
            angle_degrees,
        }),
        ("rest", None, None, None) => Ok(WidgetIntent::Rest),
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

impl ObjectFillOwner {
    /// `object.widget.create`: the layer, its controls with seeded rest values and the widget record.
    pub(crate) fn prepare_widget_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        let payload: CreatePayload = parse_payload(bytes)?;
        if payload.command != "object.widget.create" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let kind = match payload.kind.as_str() {
            "joystick" => WidgetLayerKind::Joystick,
            "slider" => WidgetLayerKind::Slider,
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        for number in &payload.pos {
            bounded(number)?;
        }
        let layer_bytes =
            serde_json::to_vec(&json!({"command":"object.layer.create","name":payload.name}))
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let mut candidate = self.prepare_layer_create_json(&layer_bytes)?;
        if let Presence::Present(plan) = &candidate.document.composition_order {
            if !plan.contexts.iter().any(|c| c.context_id == ROOT_CONTEXT) {
                return Err(DispatchErrorCode::Unavailable);
            }
        }
        let uid = candidate.layer_uid().to_owned();
        let (names, min, max, size) = match kind {
            WidgetLayerKind::Joystick => (
                vec![("X", "x"), ("Y", "y")],
                -100i64,
                100i64,
                [160i64, 160i64],
            ),
            WidgetLayerKind::Slider => (vec![("Value", "value")], 0i64, 100i64, [200i64, 26i64]),
        };
        let stem = key_stem(&uid);
        let mut controls = Vec::new();
        let mut records = Vec::new();
        let mut axes = Vec::new();
        for (name, suffix) in &names {
            let key = format!("xc_{stem}_{suffix}");
            controls.push(ExpressionControl {
                key: key.clone(),
                name: (*name).to_owned(),
                kind: "number".to_owned(),
                default_value: Presence::Present(vec![Number::from(0i64)]),
            });
            records.push(PropertyRecord {
                property: key.clone(),
                static_value: Presence::Present(vec![Number::from(0i64)]),
                track: Presence::Missing,
                expression: Presence::Missing,
            });
            axes.push(WidgetAxis {
                key,
                min: Number::from(min),
                max: Number::from(max),
                rest: Presence::Present(Some(Number::from(0i64))),
                label: Presence::Present(Some((*name).to_owned())),
            });
        }
        let mut axes = axes.into_iter();
        let x = axes.next().ok_or(DispatchErrorCode::Internal)?;
        let y = axes.next().map_or(Presence::Missing, Presence::Present);
        let [pos_x, pos_y] = payload.pos;
        let widget = WidgetRecord {
            kind,
            pos: Presence::Present(Some([pos_x, pos_y])),
            size: Presence::Present(Some([Number::from(size[0]), Number::from(size[1])])),
            screen_space: Presence::Present(false),
            x,
            y,
        };
        let layer = candidate
            .document
            .layers
            .iter_mut()
            .find(|layer| layer.layer_uid == uid)
            .ok_or(DispatchErrorCode::Internal)?;
        layer.color = Presence::Present(WIDGET_COLOR.to_owned());
        layer.widget = Presence::Present(widget);
        let mut holders = candidate
            .document
            .property_holders
            .value()
            .cloned()
            .unwrap_or_default();
        holders.push(PropertyHolder {
            target: PropertyHolderTarget {
                context_id: ROOT_CONTEXT.to_owned(),
                layer_uid: uid,
                stroke_id: Presence::Missing,
            },
            controls: Presence::Present(controls),
            properties: records,
        });
        candidate.document.property_holders = Presence::Present(holders);
        ObjectSnapshot::validate_drawing(&candidate.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(candidate)
    }

    /// `object.widget.write`: the control values of one drag, published as one edit.
    pub(crate) fn prepare_widget_write_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedWidgetEdit, DispatchErrorCode> {
        let payload: WritePayload = parse_payload(bytes)?;
        if payload.command != "object.widget.write" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let frame = payload
            .frame
            .as_f64()
            .filter(|frame| frame.is_finite())
            .ok_or(DispatchErrorCode::InvalidRequest)?;
        let intent = intent_of(&payload.to)?;
        let origin = self.current_snapshot();
        let document = origin.document();
        let bindings = document
            .widget_bindings(&payload.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        if bindings.x != AxisBinding::Resolved
            || bindings.y.is_some_and(|y| y != AxisBinding::Resolved)
        {
            return Err(DispatchErrorCode::Unavailable);
        }
        let writes = origin
            .widget_write_values(&payload.layer_uid, frame, &intent)
            .map_err(|refusal| refusal.code())?;
        let mut working = document.clone();
        for write in &writes {
            let target = holder_target(document, &payload.layer_uid, &write.key)
                .ok_or(DispatchErrorCode::Unavailable)?;
            let value = js_number(write.value).ok_or(DispatchErrorCode::InvalidRequest)?;
            let kind = if is_animated(document, &target, &write.key) {
                "setKey"
            } else {
                "setValue"
            };
            let edit = json!({
                "command": "object.property.edit",
                "target": {"contextId": target.context_id, "layerUid": target.layer_uid},
                "property": write.key,
                "action": {"kind": kind, "frame": payload.frame, "values": [value]}
            });
            let edit = serde_json::to_vec(&edit).map_err(|_| DispatchErrorCode::InvalidRequest)?;
            working = ObjectFillOwner::apply_property_edit_json(origin.clone(), working, &edit)?
                .into_document();
        }
        let changed = &working != document;
        Ok(PreparedWidgetEdit {
            origin,
            document: working,
            changed,
        })
    }

    pub(crate) fn commit_widget_edit(
        &mut self,
        candidate: PreparedWidgetEdit,
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
