//! One sealed layer-bar gesture: range, Motion, drawn content and Arc share the existing owner.
//! This private consumer is not a public Shift declaration or an installed acceptance result.
use crate::commands::object_fill_owner::object_frame_bulk_move::{
    prepare as prepare_content, Payload as ContentPayload, SHIFT,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, KeyLockMode, ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::{Deserialize, Serialize};

#[path = "object_layer_shift_motion.rs"]
mod motion;
#[path = "object_layer_shift_range.rs"]
mod range;
use motion::SelectedMotionKey;

const COMMAND: &str = "object.layer.range.drag";
const MAX_SAFE: i64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Part {
    In,
    Out,
    Both,
}
impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "in" => Ok(Self::In),
            "out" => Ok(Self::Out),
            "both" => Ok(Self::Both),
            _ => Err(serde::de::Error::custom("unknown layer-bar part")),
        }
    }
}
#[derive(Debug)]
struct BarSelection {
    layer_uid: String,
    part: Part,
}
object_deserialize!(BarSelection {
    layer_uid: String,
    part: Part
});
#[derive(Debug)]
struct Gesture {
    command: String,
    context_id: String,
    layer_uid: String,
    active_layer_uid: String,
    part: Part,
    bar_selection: Vec<BarSelection>,
    samples: Vec<i64>,
    plain_click: bool,
    alt: bool,
    frame: u32,
    selected_keys: Vec<SelectedMotionKey>,
}
object_deserialize!(Gesture {
    command: String, context_id: String, layer_uid: String, active_layer_uid: String,
    part: Part, bar_selection: Vec<BarSelection>, samples: Vec<i64>,
    plain_click: bool, alt: bool, frame: u32, selected_keys: Vec<SelectedMotionKey>
});
#[derive(Debug, Clone)]
struct Member {
    uid: String,
    part: Part,
    before: [f64; 2],
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn integer(value: f64) -> Result<i64, DispatchErrorCode> {
    if value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE as f64 {
        Ok(value as i64)
    } else {
        Err(DispatchErrorCode::Unavailable)
    }
}
fn delta(
    document: &ObjectDocument,
    gesture: &Gesture,
    member: &Member,
    clamped: bool,
) -> Result<[i64; 2], DispatchErrorCode> {
    let now = range::points(document, &member.uid, gesture.frame, clamped)?;
    Ok([
        integer(now[0] - member.before[0])?,
        integer(now[1] - member.before[1])?,
    ])
}
fn key_lock(document: &ObjectDocument, uid: &str) -> Option<KeyLockMode> {
    document
        .layers
        .iter()
        .find(|l| l.layer_uid == uid)
        .and_then(|l| l.key_lock.value())
        .copied()
        .flatten()
}
fn locked_delta(mode: KeyLockMode, part: Part, movement: [i64; 2]) -> i64 {
    match mode {
        KeyLockMode::Out => movement[1],
        KeyLockMode::Layer if part != Part::Both => 0,
        _ => movement[0],
    }
}
fn move_whole_locks(
    document: &mut ObjectDocument,
    gesture: &Gesture,
    members: &[Member],
    linked: &[Member],
) -> Result<i64, DispatchErrorCode> {
    let mut lock_delta = 0;
    for (items, clamped) in [(members, false), (linked, true)] {
        for member in items {
            let Some(mode) = key_lock(document, &member.uid) else {
                continue;
            };
            let dx = locked_delta(
                mode,
                gesture.part,
                delta(document, gesture, member, clamped)?,
            );
            if dx != 0 {
                motion::shift_layer(document, &gesture.context_id, &member.uid, dx)?;
                // The baseline stores the last nonzero lock displacement globally.
                lock_delta = dx;
            }
        }
    }
    for (items, clamped) in [(members, false), (linked, true)] {
        for member in items {
            for mode in [KeyLockMode::In, KeyLockMode::Out, KeyLockMode::Layer] {
                let dx = locked_delta(
                    mode,
                    gesture.part,
                    delta(document, gesture, member, clamped)?,
                );
                if dx == 0 {
                    continue;
                }
                let selected =
                    motion::locked_keys(document, &gesture.context_id, &member.uid, mode);
                if !selected.is_empty() {
                    // A key with a whole-layer and per-key lock intentionally moves twice.
                    motion::shift_selected(document, &selected, dx)?;
                    lock_delta = dx;
                }
            }
        }
    }
    Ok(lock_delta)
}
fn has_key_locks(document: &ObjectDocument, context: &str, layer: &str) -> bool {
    [KeyLockMode::In, KeyLockMode::Out, KeyLockMode::Layer]
        .into_iter()
        .any(|mode| !motion::locked_keys(document, context, layer, mode).is_empty())
}
fn drop_content(
    mut document: ObjectDocument,
    gesture: &Gesture,
    members: &[Member],
    lock_delta: i64,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let selected = !gesture.selected_keys.is_empty();
    let default_retimes = gesture.part == Part::Both;
    let retimes = if gesture.alt && !selected && gesture.part != Part::Out {
        !default_retimes
    } else {
        default_retimes
    };
    if !retimes {
        return Ok(document);
    }
    for member in members {
        let dx = delta(&document, gesture, member, false)?[0];
        if dx == 0 {
            continue;
        }
        let payload = ContentPayload {
            command: SHIFT.into(),
            layer_uid: member.uid.clone(),
            selection: Presence::Missing,
            frame: Presence::Missing,
            delta: Presence::Present(dx),
        };
        let (_, plan, next) = prepare_content(&document, &payload)?;
        document = next;
        let pairs: Vec<_> = plan
            .pairs
            .iter()
            .map(|p| (i64::from(p.from_a), i64::from(p.from_b), p.to_a, p.to_b))
            .collect();
        if !pairs.is_empty() {
            // Original shiftLayerFrames omits li; tween Arc uses the active layer UID.
            document
                .retime_motion_arcs(&gesture.active_layer_uid, &pairs)
                .map_err(|_| DispatchErrorCode::Unavailable)?;
        }
        if selected {
            continue;
        }
        if lock_delta != 0
            && (key_lock(&document, &member.uid).is_some()
                || has_key_locks(&document, &gesture.context_id, &member.uid))
        {
            continue;
        }
        motion::shift_layer(&mut document, &gesture.context_id, &member.uid, dx)?;
    }
    Ok(document)
}
fn apply(origin: &ObjectSnapshot, gesture: &Gesture) -> Result<ObjectDocument, DispatchErrorCode> {
    let document = origin.document();
    if gesture.command != COMMAND
        || gesture.context_id.is_empty()
        || gesture.layer_uid.is_empty()
        || gesture.active_layer_uid.is_empty()
        || gesture.frame >= document.total_frames
        || gesture.samples.len() > 4096
        || gesture
            .samples
            .iter()
            .any(|dx| !(-MAX_SAFE..=MAX_SAFE).contains(dx))
        || gesture.bar_selection.len() > 4096
        || gesture.selected_keys.len() > 4096
    {
        return Err(invalid());
    }
    // Nonroot content/Arc representation remains the named #2225/#2233 provider join.
    if gesture.context_id != "scene-root" {
        return Err(DispatchErrorCode::Unavailable);
    }
    if !document
        .layers
        .iter()
        .any(|l| l.layer_uid == gesture.active_layer_uid)
    {
        return Err(DispatchErrorCode::NotFound);
    }
    // Validate captured key identities even a click, zero delta or Alt leaves them in place.
    let mut next = document.clone();
    motion::shift_selected(&mut next, &gesture.selected_keys, 0)?;
    let (members, group) = range::members(document, gesture)?;
    let linked = range::linked(document, gesture, &members)?;
    for sample in &gesture.samples {
        if group && *sample == 0 {
            continue;
        }
        next = range::move_ranges(origin, next, gesture, &members, group, *sample)?;
    }
    // A plain click may have authored live ranges, but never runs the drop passes.
    if gesture.plain_click {
        return Ok(next);
    }
    next = range::reconcile(origin, next, gesture, &members)?;
    let selected = !gesture.selected_keys.is_empty();
    let lock_delta = if !selected && !gesture.alt {
        move_whole_locks(&mut next, gesture, &members, &linked)?
    } else {
        0
    };
    if selected && !gesture.alt {
        for member in &members {
            let movement = delta(&next, gesture, member, false)?;
            let dx = if gesture.part == Part::Out {
                movement[1]
            } else {
                movement[0]
            };
            if dx != 0 {
                motion::shift_selected(&mut next, &gesture.selected_keys, dx)?;
                break;
            }
        }
    }
    drop_content(next, gesture, &members, lock_delta)
}
/// Non-Clone, unforgeable origin + complete validated replacement; never a second revision owner.
#[derive(Debug)]
pub(crate) struct PreparedLayerShift {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}
impl PreparedLayerShift {
    pub(crate) fn command(&self) -> &'static str {
        COMMAND
    }
    pub(crate) fn checkpoint_required(&self) -> bool {
        true
    }
    pub(crate) fn document_changed(&self) -> bool {
        self.changed
    }
}
impl ObjectFillOwner {
    pub(crate) fn prepare_layer_shift_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerShift, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(invalid());
        }
        let gesture: Gesture = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        let origin = self.current_snapshot();
        let document = apply(&origin, &gesture)?;
        ObjectSnapshot::validate_drawing(&document).map_err(|_| DispatchErrorCode::Unavailable)?;
        let changed = document != *origin.document();
        Ok(PreparedLayerShift {
            origin,
            document,
            changed,
        })
    }
    pub(crate) fn commit_layer_shift(
        &mut self,
        candidate: PreparedLayerShift,
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
        // Original onDown checkpoints once even if the complete gesture changes no bytes.
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
#[cfg(test)]
#[path = "../tests/object_layer_shift.rs"]
mod tests;
