//! Strict decoding of the `object.rig.edit` payload (I12): the envelope member checks, the closed set of
//! members each action kind admits, and every number and id bound. Pure: no document, no kernel.
//!
//! The `action` object is read as an ordered member list ([`KeyedTable`], which keeps duplicate keys), so a
//! repeated member, an unknown member and a missing one are all refusals, never silently resolved.
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::{
    rules, RigAction, RigRef, RigTarget, SegmentPart,
};
use crate::object_document::object_rig_record::{KeyedTable, RigPair, RigSegment};
use crate::object_document::{object_deserialize, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Number, Value};

/// The shared envelope already bounds the whole message; this is the payload's own limit.
const MAX_PAYLOAD_BYTES: usize = 4096;
const MAX_ID_BYTES: usize = 128;
const MAX_END_KEY_BYTES: usize = 160;
/// Leaf guard (not a baseline rule): keeps every product of the pose arithmetic finite.
const MAX_MAGNITUDE: f64 = 1.0e9;
/// A bone is a handful of anchors; the record admission bounds nothing, the envelope does the rest.
const MAX_SEGMENTS: usize = 256;

type Refusal = DispatchErrorCode;

#[derive(Debug)]
struct EditPayload {
    command: String,
    layer_uid: String,
    action: KeyedTable<Value>,
}
object_deserialize!(EditPayload {
    command: String,
    layer_uid: String,
    action: KeyedTable<Value>
});

#[derive(Debug)]
struct SegmentPayload {
    point: [Number; 2],
    handle_in: [Number; 2],
    handle_out: [Number; 2],
}
object_deserialize!(SegmentPayload {
    point: [Number; 2],
    handle_in: [Number; 2],
    handle_out: [Number; 2]
});

#[derive(Debug)]
struct TargetPayload {
    stroke_id: Presence<String>,
    mesh_id: Presence<String>,
}
object_deserialize!(TargetPayload {
    #[serde(default)] stroke_id: Presence<String>,
    #[serde(default)] mesh_id: Presence<String>
});

#[derive(Debug)]
struct RefPayload {
    bone_id: String,
    vi: Number,
}
object_deserialize!(RefPayload {
    bone_id: String,
    vi: Number
});

fn bad<T>() -> Result<T, Refusal> {
    Err(DispatchErrorCode::InvalidRequest)
}

fn bounded(number: &Number) -> Result<f64, Refusal> {
    match number.as_f64() {
        Some(value) if value.is_finite() && value.abs() <= MAX_MAGNITUDE => Ok(value),
        _ => bad(),
    }
}

/// An integer in `0..=high` (a JSON `4.0` is the same JavaScript number as `4`).
fn whole(number: &Number, high: f64) -> Result<u32, Refusal> {
    match number.as_f64() {
        Some(value) if value.fract() == 0.0 && (0.0..=high).contains(&value) => Ok(value as u32),
        _ => bad(),
    }
}

fn identifier(text: &str, limit: usize) -> Result<(), Refusal> {
    if text.is_empty() || text.len() > limit {
        return bad();
    }
    Ok(())
}

/// The members of one `action` object: closed set, no duplicates.
struct Members<'a> {
    entries: &'a [(String, Value)],
}

impl<'a> Members<'a> {
    fn new(table: &'a KeyedTable<Value>, allowed: &[&str]) -> Result<Self, Refusal> {
        let mut seen: Vec<&str> = Vec::new();
        for (key, _) in table.entries() {
            if !allowed.contains(&key.as_str()) || seen.contains(&key.as_str()) {
                return bad();
            }
            seen.push(key);
        }
        Ok(Self {
            entries: table.entries(),
        })
    }

    fn get(&self, name: &str) -> Option<&'a Value> {
        self.entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    fn opt<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, Refusal> {
        match self.get(name) {
            None => Ok(None),
            Some(value) => serde_json::from_value::<T>(value.clone())
                .map(Some)
                .map_err(|_| DispatchErrorCode::InvalidRequest),
        }
    }

    fn req<T: DeserializeOwned>(&self, name: &str) -> Result<T, Refusal> {
        self.opt(name)?.ok_or(DispatchErrorCode::InvalidRequest)
    }

    fn id(&self, name: &str) -> Result<String, Refusal> {
        let text: String = self.req(name)?;
        identifier(&text, MAX_ID_BYTES)?;
        Ok(text)
    }

    fn opt_id(&self, name: &str) -> Result<Option<String>, Refusal> {
        match self.opt::<String>(name)? {
            None => Ok(None),
            Some(text) => {
                identifier(&text, MAX_ID_BYTES)?;
                Ok(Some(text))
            }
        }
    }

    fn index(&self, name: &str) -> Result<usize, Refusal> {
        Ok(whole(&self.req::<Number>(name)?, f64::from(u32::MAX))? as usize)
    }

    fn frame(&self) -> Result<u32, Refusal> {
        whole(&self.req::<Number>("frame")?, f64::from(u32::MAX))
    }

    fn number(&self, name: &str) -> Result<f64, Refusal> {
        bounded(&self.req::<Number>(name)?)
    }

    fn opt_number(&self, name: &str) -> Result<Option<f64>, Refusal> {
        match self.opt::<Number>(name)? {
            None => Ok(None),
            Some(number) => bounded(&number).map(Some),
        }
    }

    /// A non-negative bounded number (a radius).
    fn opt_radius(&self, name: &str) -> Result<Option<f64>, Refusal> {
        match self.opt_number(name)? {
            Some(value) if value < 0.0 => bad(),
            other => Ok(other),
        }
    }

    fn flag(&self, name: &str, absent: bool) -> Result<bool, Refusal> {
        Ok(self.opt::<bool>(name)?.unwrap_or(absent))
    }

    fn target(&self) -> Result<RigTarget, Refusal> {
        let payload: TargetPayload = self.req("target")?;
        match (payload.stroke_id.value(), payload.mesh_id.value()) {
            (Some(id), None) => {
                identifier(id, MAX_ID_BYTES)?;
                Ok(RigTarget::Stroke(id.clone()))
            }
            (None, Some(id)) => {
                identifier(id, MAX_ID_BYTES)?;
                Ok(RigTarget::Mesh(id.clone()))
            }
            _ => bad(),
        }
    }

    fn reference(&self, name: &str) -> Result<RigRef, Refusal> {
        let payload: RefPayload = self.req(name)?;
        identifier(&payload.bone_id, MAX_ID_BYTES)?;
        Ok(RigRef {
            bone_id: payload.bone_id,
            vi: whole(&payload.vi, f64::from(u32::MAX))?,
        })
    }
}

fn pair(value: &[Number; 2]) -> Result<[f64; 2], Refusal> {
    Ok([bounded(&value[0])?, bounded(&value[1])?])
}

/// A pair spelled like `JSON.stringify` would (`1.0` is `1`, `-0` is `0`), so a drawn bone stores exactly
/// the bytes the baseline export writes for the same numbers.
fn spelled_pair(value: &[Number; 2]) -> Result<RigPair, Refusal> {
    let [x, y] = pair(value)?;
    match (rules::js_number(x), rules::js_number(y)) {
        (Some(x), Some(y)) => Ok([x, y]),
        _ => bad(),
    }
}

fn segments(members: &Members<'_>) -> Result<Vec<RigSegment>, Refusal> {
    let raw: Vec<SegmentPayload> = members.req("segments")?;
    if raw.len() < 2 || raw.len() > MAX_SEGMENTS {
        return bad();
    }
    let mut out = Vec::with_capacity(raw.len());
    for item in raw {
        out.push(RigSegment {
            point: spelled_pair(&item.point)?,
            handle_in: Presence::Present(Some(spelled_pair(&item.handle_in)?)),
            handle_out: Presence::Present(Some(spelled_pair(&item.handle_out)?)),
        });
    }
    Ok(out)
}

fn part_of(text: &str) -> Result<SegmentPart, Refusal> {
    match text {
        "point" => Ok(SegmentPart::Point),
        "handleIn" => Ok(SegmentPart::HandleIn),
        "handleOut" => Ok(SegmentPart::HandleOut),
        _ => bad(),
    }
}

fn allowed_members(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "drawBone" => &["kind", "boneId", "segments", "closed"],
        "moveSegment" => &["kind", "boneId", "vi", "part", "to", "breakTangent"],
        "setBoneRadius" => &["kind", "boneId", "radius"],
        "bindStroke" => &[
            "kind", "strokeId", "frame", "boneIds", "radius", "rotate", "softness",
        ],
        "bindMesh" => &["kind", "meshId"],
        "autoAssign" => &[
            "kind",
            "frame",
            "defaultRadius",
            "rotate",
            "softness",
            "paths",
            "meshes",
        ],
        "setWeight" => &["kind", "target", "vertex", "entry", "percent"],
        "addWeight" => &["kind", "target", "vertex", "boneId"],
        "removeWeight" => &["kind", "target", "vertex", "entry"],
        "resetWeights" => &["kind", "target", "vertex", "radius", "softness"],
        "setIk" => &["kind", "root", "joint", "end", "flip"],
        "removeIk" => &["kind", "endKey"],
        "removeBone" => &["kind", "boneId"],
        "removeBind" => &["kind", "target"],
        _ => return None,
    })
}

fn action_of(table: &KeyedTable<Value>) -> Result<RigAction, Refusal> {
    let kind = match table.entries().iter().find(|(key, _)| key == "kind") {
        Some((_, Value::String(kind))) => kind.as_str(),
        _ => return bad(),
    };
    let allowed = allowed_members(kind).ok_or(DispatchErrorCode::InvalidRequest)?;
    let m = Members::new(table, allowed)?;
    match kind {
        "drawBone" => Ok(RigAction::DrawBone {
            bone_id: m.opt_id("boneId")?,
            segments: segments(&m)?,
            closed: m.req::<bool>("closed")?,
        }),
        "moveSegment" => Ok(RigAction::MoveSegment {
            bone_id: m.id("boneId")?,
            vi: m.index("vi")?,
            part: part_of(&m.req::<String>("part")?)?,
            to: pair(&m.req::<[Number; 2]>("to")?)?,
            break_tangent: m.flag("breakTangent", false)?,
        }),
        "setBoneRadius" => Ok(RigAction::SetBoneRadius {
            bone_id: m.id("boneId")?,
            radius: m.number("radius")?,
        }),
        "bindStroke" => {
            let bone_ids: Option<Vec<String>> = m.opt("boneIds")?;
            if let Some(ids) = &bone_ids {
                for id in ids {
                    identifier(id, MAX_ID_BYTES)?;
                }
            }
            Ok(RigAction::BindStroke {
                stroke_id: m.id("strokeId")?,
                frame: m.frame()?,
                bone_ids,
                radius: m.opt_radius("radius")?,
                rotate: m.flag("rotate", false)?,
                softness: m.opt_number("softness")?,
            })
        }
        "bindMesh" => {
            m.id("meshId")?;
            Ok(RigAction::BindMesh)
        }
        "autoAssign" => Ok(RigAction::AutoAssign {
            frame: m.frame()?,
            default_radius: m.opt_radius("defaultRadius")?,
            rotate: m.flag("rotate", false)?,
            softness: m.opt_number("softness")?,
            paths: m.flag("paths", true)?,
            meshes: m.flag("meshes", true)?,
        }),
        "setWeight" => Ok(RigAction::SetWeight {
            target: m.target()?,
            vertex: m.index("vertex")?,
            entry: m.index("entry")?,
            percent: m.number("percent")?,
        }),
        "addWeight" => Ok(RigAction::AddWeight {
            target: m.target()?,
            vertex: m.index("vertex")?,
            bone_id: m.id("boneId")?,
        }),
        "removeWeight" => Ok(RigAction::RemoveWeight {
            target: m.target()?,
            vertex: m.index("vertex")?,
            entry: m.index("entry")?,
        }),
        "resetWeights" => {
            // The panel slider is 0..100 %, so the softness the popover passes is 0..=1.
            let softness = m.opt_number("softness")?.unwrap_or(0.0);
            if !(0.0..=1.0).contains(&softness) {
                return bad();
            }
            Ok(RigAction::ResetWeights {
                target: m.target()?,
                vertex: m.index("vertex")?,
                radius: m.opt_radius("radius")?,
                softness,
            })
        }
        "setIk" => Ok(RigAction::SetIk {
            root: m.reference("root")?,
            joint: m.reference("joint")?,
            end: m.reference("end")?,
            flip: m.flag("flip", false)?,
        }),
        "removeIk" => {
            let end_key = m.req::<String>("endKey")?;
            identifier(&end_key, MAX_END_KEY_BYTES)?;
            Ok(RigAction::RemoveIk { end_key })
        }
        "removeBone" => Ok(RigAction::RemoveBone {
            bone_id: m.id("boneId")?,
        }),
        "removeBind" => Ok(RigAction::RemoveBind {
            target: m.target()?,
        }),
        _ => bad(),
    }
}

/// `(layerUid, action)` of a strict `object.rig.edit` payload.
pub(super) fn parse_edit(bytes: &[u8]) -> Result<(String, RigAction), Refusal> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return bad();
    }
    let payload: EditPayload = match serde_json::from_slice(bytes) {
        Ok(payload) => payload,
        Err(_) => return bad(),
    };
    if payload.command != "object.rig.edit" {
        return bad();
    }
    identifier(&payload.layer_uid, MAX_ID_BYTES)?;
    let action = action_of(&payload.action)?;
    Ok((payload.layer_uid, action))
}
