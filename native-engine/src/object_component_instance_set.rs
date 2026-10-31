//! Component-instance playback edits (#2256 Q5): the Component Instance panel's writers of the baseline
//! (`timeline.js` at 1065e9d8: `setSymbolPlayMode`, `setSymbolSingleFrame`, `setSymbolSpeed`, `setSymbolPlacedAt`) as ONE
//! closed partial edit of the instance record of a scene layer.
//!
//! Baseline per field (`ld` is the instance layer): `symPlayMode` is the select value (`loop`, `once`, `pingpong`, `single`);
//! `symSingleFrame` is `Math.max(0, parseInt(v) || 0)` and, when the playhead sits on an outer keyframe (`f.isKeyframe`), the
//! SAME pick is written to that key as `componentFrame` with `blankOverride` deleted; `symSpeed` is the number the panel sends
//! (the baseline floors it at 0.1 before writing); `symPlacedAt` is `parseInt(v) || 0`. Fields the baseline never writes
//! (`symTrimIn`, `symTrimOut`, `symMatrix`, the Time Remap track, every other frame key) keep their bytes.
//!
//! The edit names only the members it changes. A value equal to the stored one changes nothing, and an edit that changes
//! nothing publishes nothing (no revision, no history entry). A layer that is not a scene-root component instance is refused,
//! as is a v1 document; the lock of the layer is not consulted (the baseline writers never read it). Declared as a private
//! child of `object_component_owner_kernels.rs`.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    layer_index, require_v2, update, with_tables, SetupError, SetupPlan,
};
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentInstance, FrameKey, SCENE_CONTEXT,
};
use crate::object_document::{ObjectDocument, Presence};
use serde_json::Number;

/// The four labels the panel's select offers (`resolveSymbolFrameIdx` plays any other label as a loop).
pub(crate) const MODES: [&str; 4] = ["loop", "once", "pingpong", "single"];
/// The decoding bound of a record's frame-key array (`MAX_ENTRIES` of the instance records).
const MAX_KEYS: usize = 4096;

/// The members of one `object.component.instance.set` request (every one optional).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct InstanceEdit {
    pub(crate) mode: Option<String>,
    pub(crate) single_frame: Option<u32>,
    pub(crate) placed_at: Option<i32>,
    pub(crate) speed: Option<f64>,
    /// The outer frame the playhead is on: with `single_frame`, an outer keyframe there takes the pick as its `componentFrame`.
    pub(crate) at_frame: Option<u32>,
}

/// A JSON number: an integer when the value is one (so a stored `2` stays `2`), else the float.
fn number(value: f64) -> Number {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Number::from(value as i64)
    } else {
        Number::from_f64(value).unwrap_or_else(|| Number::from(0))
    }
}

fn set_number(slot: &mut Presence<Option<Number>>, wanted: Option<f64>) {
    let Some(value) = wanted else { return };
    let stored = slot
        .value()
        .and_then(|n| n.as_ref())
        .and_then(Number::as_f64);
    if stored != Some(value) {
        *slot = Presence::Present(Some(number(value)));
    }
}

/// `if(f&&f.isKeyframe){f.componentFrame=picked;delete f.blankOverride;}` over the held outer row of the instance layer.
fn pick_on_key(
    instance: &mut ComponentInstance,
    on_key: bool,
    frame: u32,
    picked: u32,
) -> Result<(), SetupError> {
    if !on_key {
        return Ok(());
    }
    let wanted = FrameKey {
        frame,
        component_frame: Presence::Present(Some(Number::from(picked))),
        blank_override: Presence::Missing,
    };
    update(&mut instance.frame_keys, Vec::new, |keys| {
        match keys.binary_search_by_key(&frame, |key| key.frame) {
            Ok(at) => {
                if keys[at] != wanted {
                    keys[at] = wanted;
                }
                Ok(())
            }
            Err(at) if keys.len() < MAX_KEYS => {
                keys.insert(at, wanted);
                Ok(())
            }
            Err(_) => Err(SetupError::TooMany),
        }
    })
}

/// The panel's field writers as one transaction on the layer's stored instance record.
pub(crate) fn plan_instance_set(
    document: &ObjectDocument,
    layer_uid: &str,
    edit: &InstanceEdit,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    let host = layer_index(document, layer_uid)?;
    let record = document
        .component_set()
        .and_then(|set| {
            set.instances
                .iter()
                .find(|i| i.context_id == SCENE_CONTEXT && i.layer_uid == layer_uid)
        })
        .ok_or_else(|| SetupError::NotAnInstance(layer_uid.to_string()))?;
    if edit.mode.as_deref().is_some_and(|m| !MODES.contains(&m))
        || edit.speed.is_some_and(|s| !s.is_finite() || s <= 0.0)
        || edit.at_frame.is_some() && edit.single_frame.is_none()
        || edit.at_frame.is_some_and(|f| f >= document.total_frames)
    {
        return Err(SetupError::PlaybackInvalid);
    }
    let mut next = record.clone();
    if let Some(mode) = &edit.mode {
        if next.play_mode() != Some(mode.as_str()) {
            next.sym_play_mode = Presence::Present(Some(mode.clone()));
        }
    }
    set_number(&mut next.sym_single_frame, edit.single_frame.map(f64::from));
    set_number(&mut next.sym_placed_at, edit.placed_at.map(f64::from));
    set_number(&mut next.sym_speed, edit.speed);
    if let (Some(picked), Some(frame)) = (edit.single_frame, edit.at_frame) {
        let on_key = document.layers[host]
            .timing
            .value()
            .is_some_and(|timing| timing.exposure(frame).0 == "key");
        pick_on_key(&mut next, on_key, frame, picked)?;
    }
    if next == *record {
        return Ok(SetupPlan::unchanged(document, Some(&record.symbol_id)));
    }
    let symbol_id = record.symbol_id.clone();
    let mut candidate = document.clone();
    with_tables(&mut candidate, |tables| {
        let Presence::Present(set) = &mut tables.components else {
            return Err(SetupError::NotAnInstance(layer_uid.to_string()));
        };
        match set
            .instances
            .iter_mut()
            .find(|i| i.context_id == SCENE_CONTEXT && i.layer_uid == layer_uid)
        {
            Some(found) => {
                *found = next;
                Ok(())
            }
            None => Err(SetupError::NotAnInstance(layer_uid.to_string())),
        }
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: Some(symbol_id),
        layer_uid: Some(layer_uid.to_string()),
        created: None,
    })
}

#[cfg(test)]
#[path = "../tests/object_component_convert_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_component_instance_set.rs"]
mod tests;
