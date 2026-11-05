//! Null, Effect, Guide, Folder and Effector layer creation (#2371 U11 / D36): `addNullLayer`, `addEffectLayer`,
//! `addGuideLayer`, `addFolderLayer` of `src/js/timeline.js` and `addEffectorLayer` of `src/js/effector-layer.js` at
//! 1065e9d8, as ONE sealed layer-create candidate.
//!
//! The candidate is the ordinary layer-create one (unique monotonic id, order-plan insertion, appended last, as the
//! baseline's `createUserLayer` pushes) whose blank layer is replaced by the kind layer, so one gesture is one revision
//! and one history entry. A Null and a Folder also adopt the layers the user had selected (`children`): each is freed
//! with the Parent A planner (its world pose kept) and then attached, and without an explicit `pos` the marker sits at
//! the centre of the union of their bounds (the canvas centre when none has bounds), as the baseline does. An Effector
//! needs a layer that hosts a duplicator and starts with the visible Offset Position the baseline seeds.
use crate::commands::object_fill_owner::layer_lifecycle::parent::{plan_parent, set_static_value};
use crate::commands::object_fill_owner::layer_lifecycle::refusal_code;
use crate::commands::{ObjectFillOwner, PreparedLayerCreate};
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    EffectorLayerRecord, EffectorTarget,
};
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::index_of;
use crate::object_document::{object_deserialize, ObjectDocument, ObjectLayer, Presence};
use crate::object_snapshot::object_parent_pivot_read::layer_pivot_extents;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Number};
use std::collections::HashSet;

const ROOT: &str = "scene-root";
const MAX_PAYLOAD_BYTES: usize = 4096;
const MAX_NAME_BYTES: usize = 256;
const MAX_CHILDREN: usize = 128;
const MAX_ANCHOR: f64 = 1_000_000_000.0;
const NULL_COLOR: &str = "#ff2d78";
const GUIDE_COLOR: &str = "#00baff";
const EFFECTOR_COLOR: &str = "#59d38a";
/// The baseline `CHANNELS[0].key`: the first offset channel, seeded so a fresh effector visibly does something.
const EFFECTOR_SEED_KEY: &str = "effOffPosition";

#[derive(Debug)]
struct KindCreatePayload {
    command: String,
    kind: String,
    name: String,
    frame: u32,
    pos: Option<[Number; 2]>,
    children: Option<Vec<String>>,
    target: Option<String>,
}
object_deserialize!(KindCreatePayload {
    command: String,
    kind: String,
    name: String,
    frame: u32,
    pos: Option<[Number; 2]>,
    children: Option<Vec<String>>,
    target: Option<String>
});

fn anchor(pair: &[Number; 2]) -> Result<[f64; 2], DispatchErrorCode> {
    let mut out = [0.0; 2];
    for (slot, number) in out.iter_mut().zip(pair) {
        *slot = number
            .as_f64()
            .filter(|v| v.is_finite() && v.abs() <= MAX_ANCHOR)
            .ok_or(DispatchErrorCode::InvalidRequest)?;
    }
    Ok(out)
}

fn number(value: f64) -> Result<Number, DispatchErrorCode> {
    Number::from_f64(value).ok_or(DispatchErrorCode::InvalidRequest)
}

fn pair(point: [f64; 2]) -> Result<[Number; 2], DispatchErrorCode> {
    Ok([number(point[0])?, number(point[1])?])
}

/// Centre of the union of the children's own bounds; a child without evaluated bounds adds nothing (the baseline's
/// `layerWorldBoundsUnion` skips an empty layer), and no bounds at all means no centre.
fn union_centre(origin: &ObjectSnapshot, children: &[String], frame: u32) -> Option<[f64; 2]> {
    let mut union: Option<[f64; 4]> = None;
    for uid in children {
        let Ok(rects) = layer_pivot_extents(origin, ROOT, &[uid.as_str()], frame) else {
            continue;
        };
        let Some(rect) = rects.first().copied() else {
            continue;
        };
        union = Some(match union {
            None => rect,
            Some(u) => [
                u[0].min(rect[0]),
                u[1].min(rect[1]),
                u[2].max(rect[2]),
                u[3].max(rect[3]),
            ],
        });
    }
    union.map(|u| [(u[0] + u[2]) / 2.0, (u[1] + u[3]) / 2.0])
}

/// Copies the layer-level Motion holder of `uid` that the Parent A plan rewrote into the accumulating document.
fn adopt_holder(into: &mut ObjectDocument, from: &ObjectDocument, uid: &str) {
    let mine = |h: &crate::object_document::PropertyHolder| {
        h.target().layer_uid() == uid && h.target().stroke_id().is_none()
    };
    let Some(source) = from
        .property_holders()
        .and_then(|all| all.iter().find(|h| mine(h)))
    else {
        return;
    };
    let mut holders = into
        .property_holders()
        .map(<[_]>::to_vec)
        .unwrap_or_default();
    match holders.iter().position(mine) {
        Some(at) => holders[at] = source.clone(),
        None => holders.push(source.clone()),
    }
    into.property_holders = Presence::Present(holders);
}

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_kind_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: KindCreatePayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let children = payload.children.clone().unwrap_or_default();
        let grouping = matches!(payload.kind.as_str(), "null" | "folder");
        let valid_kind =
            grouping || matches!(payload.kind.as_str(), "effect" | "guide" | "effector");
        let distinct: HashSet<&str> = children.iter().map(String::as_str).collect();
        if payload.command != "object.layer.kind.create"
            || !valid_kind
            || payload.name.len() > MAX_NAME_BYTES
            || children.len() > MAX_CHILDREN
            || distinct.len() != children.len()
            || children.iter().any(String::is_empty)
            || (!children.is_empty() && !grouping)
            || payload.target.as_deref().is_some_and(str::is_empty)
            || (payload.kind == "effector") != payload.target.is_some()
            || (payload.pos.is_some() && payload.kind == "effect")
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let explicit = payload.pos.as_ref().map(anchor).transpose()?;
        let base =
            serde_json::to_vec(&json!({"command": "object.layer.create", "name": payload.name}))
                .map_err(|_| DispatchErrorCode::Internal)?;
        let mut candidate = self.prepare_layer_create_json(&base)?;
        let origin = candidate.origin.clone();
        let document = origin.document();
        for child in &children {
            index_of(document, child).map_err(|e| refusal_code(&e))?;
        }
        // Only an anchored kind has a centre: `pos`, else the bounds of the layers it adopts, else the canvas centre.
        let centre = |origin: &ObjectSnapshot| -> Result<[f64; 2], DispatchErrorCode> {
            if let Some(point) = explicit.or_else(|| union_centre(origin, &children, payload.frame))
            {
                return Ok(point);
            }
            let (w, h) = origin
                .project()
                .map(|p| p.project().canvas_dimensions())
                .ok_or(DispatchErrorCode::Unavailable)?;
            Ok([f64::from(w) / 2.0, f64::from(h) / 2.0])
        };
        let uid = candidate.layer_uid.clone();
        let slot = candidate.document.layers.len() - 1;
        match payload.kind.as_str() {
            "null" | "folder" => {
                let mut layer = if payload.kind == "folder" {
                    ObjectLayer::blank_folder_layer(uid.clone(), payload.name.clone(), slot)
                } else {
                    let mut plain = candidate.document.layers[slot].clone();
                    plain.color = Presence::Present(NULL_COLOR.into());
                    plain
                };
                // A folder is a Null that also groups (`addFolderLayer` marks both): the same pivot, no content.
                layer.is_null_layer = Presence::Present(Some(true));
                layer.null_pos = Presence::Present(Some(pair(centre(&origin)?)?));
                candidate.document.layers[slot] = layer;
                for child in &children {
                    let at = index_of(document, child).map_err(|e| refusal_code(&e))?;
                    let plan = plan_parent(&origin, ROOT, child, None, payload.frame)
                        .map_err(|e| refusal_code(&e))?;
                    if plan.changed {
                        adopt_holder(&mut candidate.document, &plan.document, child);
                    }
                    candidate.document.layers[at].parent_layer_uid =
                        Presence::Present(Some(uid.clone()));
                }
            }
            "effect" => {
                candidate.document.layers[slot] =
                    ObjectLayer::blank_effect_layer(uid.clone(), payload.name.clone(), slot);
            }
            "guide" => {
                let layer = &mut candidate.document.layers[slot];
                layer.is_guide_layer = Presence::Present(true);
                layer.guide_pos = Presence::Present(pair(centre(&origin)?)?);
                layer.guide_orientation = Presence::Present("horizontal".into());
                layer.color = Presence::Present(GUIDE_COLOR.into());
            }
            _ => {
                let target = payload
                    .target
                    .clone()
                    .ok_or(DispatchErrorCode::InvalidRequest)?;
                index_of(document, &target).map_err(|e| refusal_code(&e))?;
                let hosts = document
                    .duplicator_set()
                    .is_some_and(|set| set.duplicator(ROOT, &target).is_some());
                if !hosts {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                candidate.document.layers[slot].color = Presence::Present(EFFECTOR_COLOR.into());
                let record = EffectorLayerRecord {
                    context_id: ROOT.to_owned(),
                    layer_uid: uid.clone(),
                    effector: EffectorTarget {
                        target_layer_uid: target,
                        falloff: Presence::Present("radial".to_owned()),
                        pos: Presence::Present({
                            let point = centre(&origin)?;
                            vec![number(point[0])?, number(point[1])?]
                        }),
                    },
                };
                let Presence::Present(tables) = &mut candidate.document.component_tables else {
                    return Err(DispatchErrorCode::Unavailable);
                };
                let Presence::Present(set) = &mut tables.duplicators else {
                    return Err(DispatchErrorCode::Unavailable);
                };
                set.effector_layers.push(record);
                set_static_value(
                    &mut candidate.document,
                    ROOT,
                    &uid,
                    EFFECTOR_SEED_KEY,
                    vec![Number::from(120), Number::from(0)],
                );
            }
        }
        ObjectSnapshot::validate_drawing(&candidate.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(candidate)
    }
}
