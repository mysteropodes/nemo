//! #2520/#2525: the property edits the Motion canvas needs beyond a plain value, as sealed pure steps over a working record:
//! the `keepPose` anchor edit (Position follows the new anchor in the same candidate, `motion.js@1065e9d8` onDrag 'anchor') and
//! `setKeyHandle` (one spatial handle of an existing key, the baseline 'handle' drag). The owner in `object_property_edit.rs`
//! resolves the holder, validates, and publishes once.
use crate::object_document::{
    ObjectDocument, Presence, PropertyHolder, PropertyHolderTarget, PropertyRecord,
};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::Number;

fn same_spelling(before: &[Number], after: &mut [Number]) {
    for (before, after) in before.iter().zip(after) {
        if before.as_f64() == after.as_f64() {
            *after = before.clone();
        }
    }
}

impl PropertyRecord {
    /// `setValue` semantics: a keyed track takes a key at `frame`, a static record its static value; an equal number keeps its spelling.
    pub(crate) fn write_value(&mut self, frame: Number, mut values: Vec<Number>) {
        if let Presence::Present(track) = &mut self.track {
            if !track.keys.is_empty() {
                track.scalar_intent_key(frame, values, None);
                return;
            }
        }
        if let Some(before) = self.static_value.value() {
            same_spelling(before, &mut values);
        }
        self.static_value = Presence::Present(values);
    }

    /// Set the `hIn` or `hOut` 2-vector of the existing key at `frame`; the key's value and curve never change.
    pub(crate) fn write_key_handle(
        &mut self,
        frame: f64,
        handle: &str,
        mut values: Vec<Number>,
    ) -> Result<(), DispatchErrorCode> {
        if values.len() != 2 || !matches!(handle, "hIn" | "hOut") {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if values.iter().any(|v| !v.as_f64().is_some_and(f64::is_finite)) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let Presence::Present(track) = &mut self.track else {
            return Err(DispatchErrorCode::NotFound);
        };
        let key = track
            .keys
            .iter_mut()
            .find(|k| k.frame.as_f64() == Some(frame))
            .ok_or(DispatchErrorCode::NotFound)?;
        let slot = if handle == "hIn" { &mut key.h_in } else { &mut key.h_out };
        if let Presence::Present(before) = slot {
            same_spelling(before, &mut values);
        }
        *slot = Presence::Present(values);
        Ok(())
    }
}

impl PropertyHolder {
    /// Write the Position that follows a `keepPose` anchor edit, with `setValue` semantics, creating the record when it is absent.
    pub(crate) fn follow_position(
        &mut self,
        document: &ObjectDocument,
        frame: Number,
        values: Vec<Number>,
    ) -> Result<(), DispatchErrorCode> {
        self.metadata_in_document(document, "position")
            .ok_or(DispatchErrorCode::Unavailable)?;
        let at = self.properties.iter().position(|r| r.property == "position");
        let mut record = at.map_or_else(
            || PropertyRecord {
                property: "position".into(),
                static_value: Presence::Missing,
                track: Presence::Missing,
                expression: Presence::Missing,
            },
            |i| self.properties[i].clone(),
        );
        record.write_value(frame, values);
        match at {
            Some(i) => self.properties[i] = record,
            None => self.properties.push(record),
        }
        Ok(())
    }
}

/// `pos + (I - R S)(anchor_old - anchor_new)` from the raw Position, Anchor, Rotation and Scale at `frame`: the Position that keeps
/// every rendered point still when the anchor moves to `anchor`.
pub(crate) fn keep_pose_position(
    origin: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    frame: f64,
    anchor: &[Number],
) -> Result<Vec<Number>, DispatchErrorCode> {
    let read = |property: &str, len: usize| {
        origin
            .sample_property_raw(target, property, frame)
            .ok()
            .filter(|v| v.len() == len && v.iter().all(|x| x.is_finite()))
            .ok_or(DispatchErrorCode::Unavailable)
    };
    let (old, rot, scale, pos) = (read("anchor", 2)?, read("rotation", 1)?, read("scale", 2)?, read("position", 2)?);
    let new = |n: &Number| n.as_f64().filter(|v| v.is_finite()).ok_or(DispatchErrorCode::InvalidRequest);
    let (s, c) = rot[0].to_radians().sin_cos();
    let (sx, sy) = (scale[0] / 100.0, scale[1] / 100.0);
    let (dx, dy) = (old[0] - new(&anchor[0])?, old[1] - new(&anchor[1])?);
    let (mx, my) = (sx * c * dx - sy * s * dy, sx * s * dx + sy * c * dy);
    [pos[0] + (dx - mx), pos[1] + (dy - my)]
        .into_iter()
        .map(|v| Number::from_f64(v).ok_or(DispatchErrorCode::Unavailable))
        .collect()
}
