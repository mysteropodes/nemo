//! Strict decoding of the `object.rig.pose` payload (I13): the envelope members, the closed set of
//! members each action kind admits and the id and frame bounds. Pure: no document, no kernel.
//!
//! The payload is `{command, layerUid, action}` with `action` either `{kind:"reset"}` or
//! `{kind:"commit", frame}`. The strict object decoder refuses a repeated member, an unknown member and a
//! missing one, so nothing is silently resolved; `reset` admits no `frame` and `commit` requires one.
use crate::commands::object_fill_owner::widget_edit::object_rig_pose_commit::{
    PoseAction, PoseRefusal,
};
use crate::object_document::{object_deserialize, Presence};
use serde::Deserialize;
use serde_json::Number;

/// The shared envelope already bounds the whole message; this is the payload's own limit.
const MAX_PAYLOAD_BYTES: usize = 1024;
const MAX_ID_BYTES: usize = 128;
/// The `command` member of the payload.
pub(super) const COMMAND: &str = "object.rig.pose";

#[derive(Debug)]
struct PosePayload {
    command: String,
    layer_uid: String,
    action: ActionPayload,
}
object_deserialize!(PosePayload {
    command: String,
    layer_uid: String,
    action: ActionPayload
});

#[derive(Debug)]
struct ActionPayload {
    kind: String,
    frame: Presence<Number>,
}
object_deserialize!(ActionPayload {
    kind: String,
    #[serde(default)] frame: Presence<Number>
});

/// An integer in `0..=u32::MAX` (a JSON `4.0` is the same JavaScript number as `4`).
fn whole(number: &Number) -> Result<u32, PoseRefusal> {
    match number.as_f64() {
        Some(value) if value.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&value) => {
            Ok(value as u32)
        }
        _ => Err(PoseRefusal::Malformed),
    }
}

/// The target layer and the validated action, or `Malformed`.
pub(super) fn parse_pose(bytes: &[u8]) -> Result<(String, PoseAction), PoseRefusal> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(PoseRefusal::Malformed);
    }
    let payload: PosePayload = serde_json::from_slice(bytes).map_err(|_| PoseRefusal::Malformed)?;
    if payload.command != COMMAND
        || payload.layer_uid.is_empty()
        || payload.layer_uid.len() > MAX_ID_BYTES
    {
        return Err(PoseRefusal::Malformed);
    }
    let action = match (payload.action.kind.as_str(), payload.action.frame.value()) {
        ("reset", None) => PoseAction::Reset,
        ("commit", Some(number)) => PoseAction::Commit {
            frame: whole(number)?,
        },
        _ => return Err(PoseRefusal::Malformed),
    };
    Ok((payload.layer_uid, action))
}
