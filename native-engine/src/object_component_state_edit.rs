//! Component-owned state edits (#2256 C-comp-owned-state, continuation): the writers of what `enterSymbol` / `exitToScene` of the
//! baseline (`src/js/app.js` at 1065e9d8) swap per component, `sym.cameraKeys` and `sym.markers`, as ONE closed edit of the stored
//! component record. The baseline editors are the camera tool (`src/js/camera.js` `setKey` / `removeKey`, the ease editor, a
//! handle drag) and the marker strip (`src/js/markers.js` `addCompMarker`, rename, remove); inside an entered component they act
//! on `state.cameraKeys` / `state.markers`, here the request names the component (`contextId`, a declared symbol id) instead.
//!
//! Camera: `setCameraKey` upserts the key at an inner frame (a new key takes the baseline defaults: rotation 0, both handles
//! `[0, 0]`, the ease-in-out curve `[0.42, 0, 0.58, 1]`; the members of an existing key that the request does not name keep their
//! bytes), `removeCameraKey` drops it, `setCameraEase` writes the curve of the segment leaving it, `clearCamera` is the delete of
//! the camera layer (`cameraKeys = []`). Markers: one per frame like After Effects, `addMarker` keeps the stable frame sort and a
//! second marker on a taken frame changes nothing, `updateMarker` renames, recolours or moves one, `removeMarker` and
//! `clearMarkers` drop them. An edit that changes nothing publishes nothing. The scene root is refused here: its camera and
//! markers have their own owners. Declared as a private child of `object_component_owner_kernels.rs`.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    require_v2, with_tables, SetupError, SetupPlan,
};
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentMarker, ComponentRecord, SCENE_CONTEXT,
};
use crate::object_document::{ObjectDocument, Presence, ProjectCameraKey};

/// `DEFAULT_EASE` of `camera.js`: the least surprising default for camera moves.
const DEFAULT_EASE: [f64; 4] = [0.42, 0.0, 0.58, 1.0];
/// `DEFAULT_COLOR` of `markers.js`.
const DEFAULT_COLOR: &str = "#e8b64c";
const MAX_KEYS: usize = 4096;
const MAX_TEXT: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StateEdit {
    SetCameraKey {
        frame: u32,
        x: Option<f64>,
        y: Option<f64>,
        w: Option<f64>,
        rot: Option<f64>,
        h_out: Option<[f64; 2]>,
        h_in: Option<[f64; 2]>,
        ease: Option<[f64; 4]>,
    },
    RemoveCameraKey {
        frame: u32,
    },
    SetCameraEase {
        frame: u32,
        ease: [f64; 4],
    },
    ClearCamera,
    AddMarker {
        frame: u32,
        name: Option<String>,
        color: Option<String>,
    },
    UpdateMarker {
        frame: u32,
        new_frame: Option<u32>,
        name: Option<String>,
        color: Option<String>,
    },
    RemoveMarker {
        frame: u32,
    },
    ClearMarkers,
}

fn invalid() -> SetupError {
    SetupError::StateEditInvalid("The component camera or marker request is not valid.")
}
fn missing(why: &'static str) -> SetupError {
    SetupError::StateTargetMissing(why)
}

fn finite(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn text_ok(text: &str) -> bool {
    text.len() <= MAX_TEXT && !text.contains('\0')
}

fn camera_edit(
    component: &mut ComponentRecord,
    edit: &StateEdit,
    total: u32,
) -> Result<(), SetupError> {
    if component.camera_keys.missing() {
        component.camera_keys = Presence::Present(Vec::new());
    }
    let keys = component.camera_keys.value_mut().ok_or_else(invalid)?;
    let at = |keys: &[ProjectCameraKey], frame: u32| keys.iter().position(|k| k.frame == f64::from(frame));
    match edit {
        StateEdit::SetCameraKey { frame, x, y, w, rot, h_out, h_in, ease } => {
            if *frame >= total
                || ![x, y, w, rot].iter().all(|v| v.is_none_or(|v| v.is_finite()))
                || !h_out.is_none_or(|v| finite(&v))
                || !h_in.is_none_or(|v| finite(&v))
                || !ease.is_none_or(|v| finite(&v) && (0.0..=1.0).contains(&v[0]) && (0.0..=1.0).contains(&v[2]))
                || w.is_some_and(|w| w <= 0.0)
            {
                return Err(invalid());
            }
            match at(keys, *frame) {
                Some(found) => {
                    let key = &mut keys[found];
                    if let Some(v) = x { key.x = *v; }
                    if let Some(v) = y { key.y = *v; }
                    if let Some(v) = w { key.w = *v; }
                    if let Some(v) = rot { key.rot = Presence::Present(*v); }
                    if let Some(v) = h_out { key.h_out = Presence::Present(*v); }
                    if let Some(v) = h_in { key.h_in = Presence::Present(*v); }
                    if let Some(v) = ease { key.ease = Presence::Present(*v); }
                }
                None => {
                    let (Some(x), Some(y), Some(w)) = (x, y, w) else {
                        return Err(invalid());
                    };
                    if keys.len() >= MAX_KEYS {
                        return Err(SetupError::TooMany);
                    }
                    let key = ProjectCameraKey {
                        frame: f64::from(*frame),
                        x: *x,
                        y: *y,
                        w: *w,
                        rot: Presence::Present(rot.unwrap_or(0.0)),
                        h_out: Presence::Present(h_out.unwrap_or([0.0, 0.0])),
                        h_in: Presence::Present(h_in.unwrap_or([0.0, 0.0])),
                        ease: Presence::Present(ease.unwrap_or(DEFAULT_EASE)),
                    };
                    let to = keys.partition_point(|k| k.frame < key.frame);
                    keys.insert(to, key);
                }
            }
            Ok(())
        }
        StateEdit::RemoveCameraKey { frame } => {
            let found = at(keys, *frame).ok_or_else(|| missing("The component camera has no key on that frame."))?;
            keys.remove(found);
            Ok(())
        }
        StateEdit::SetCameraEase { frame, ease } => {
            if !finite(ease) || !(0.0..=1.0).contains(&ease[0]) || !(0.0..=1.0).contains(&ease[2]) {
                return Err(invalid());
            }
            let found = at(keys, *frame).ok_or_else(|| missing("The component camera has no key on that frame."))?;
            keys[found].ease = Presence::Present(*ease);
            Ok(())
        }
        StateEdit::ClearCamera => {
            keys.clear();
            Ok(())
        }
        _ => Err(invalid()),
    }
}

fn marker_edit(
    component: &mut ComponentRecord,
    edit: &StateEdit,
    total: u32,
) -> Result<(), SetupError> {
    if component.markers.missing() {
        component.markers = Presence::Present(Vec::new());
    }
    let markers = component.markers.value_mut().ok_or_else(invalid)?;
    let find = |markers: &[ComponentMarker], frame: u32| markers.iter().position(|m| m.frame == frame);
    match edit {
        StateEdit::AddMarker { frame, name, color } => {
            let (name, color) = (name.clone().unwrap_or_default(), color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()));
            if *frame >= total || !text_ok(&name) || color.is_empty() || !text_ok(&color) {
                return Err(invalid());
            }
            if find(markers, *frame).is_some() {
                return Ok(()); // one per frame, like AE
            }
            if markers.len() >= MAX_KEYS {
                return Err(SetupError::TooMany);
            }
            markers.push(ComponentMarker { frame: *frame, name, color });
            markers.sort_by_key(|m| m.frame);
            Ok(())
        }
        StateEdit::UpdateMarker { frame, new_frame, name, color } => {
            let found = find(markers, *frame).ok_or_else(|| missing("The component has no marker on that frame."))?;
            if new_frame.is_some_and(|to| to >= total || (to != *frame && find(markers, to).is_some()))
                || name.as_deref().is_some_and(|n| !text_ok(n))
                || color.as_deref().is_some_and(|c| c.is_empty() || !text_ok(c))
            {
                return Err(invalid());
            }
            let marker = &mut markers[found];
            if let Some(v) = name { marker.name.clone_from(v); }
            if let Some(v) = color { marker.color.clone_from(v); }
            if let Some(v) = new_frame {
                marker.frame = *v;
                markers.sort_by_key(|m| m.frame);
            }
            Ok(())
        }
        StateEdit::RemoveMarker { frame } => {
            let found = find(markers, *frame).ok_or_else(|| missing("The component has no marker on that frame."))?;
            markers.remove(found);
            Ok(())
        }
        StateEdit::ClearMarkers => {
            markers.clear();
            Ok(())
        }
        _ => Err(invalid()),
    }
}

fn frame_of(value: &serde_json::Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
}
fn number_of(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}
fn array_of<const N: usize>(value: &serde_json::Value) -> Option<[f64; N]> {
    let items = value.as_array()?;
    if items.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = number_of(item)?;
    }
    Some(out)
}

/// The wire request: `{command, contextId, kind, ...}` with exactly the members the kind takes. Returns the component symbol id
/// and the edit; `None` is a malformed request.
pub(crate) fn parse(bytes: &[u8]) -> Option<(String, StateEdit)> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    if object.get("command")?.as_str()? != "object.component.state.edit" {
        return None;
    }
    let context = object.get("contextId")?.as_str().filter(|id| !id.is_empty() && id.len() <= 128 && !id.contains('\0'))?;
    let kind = object.get("kind")?.as_str()?;
    let (required, optional): (&[&str], &[&str]) = match kind {
        "setCameraKey" => (&["frame"], &["x", "y", "w", "rot", "hOut", "hIn", "ease"]),
        "removeCameraKey" => (&["frame"], &[]),
        "setCameraEase" => (&["frame", "ease"], &[]),
        "clearCamera" | "clearMarkers" => (&[], &[]),
        "addMarker" => (&["frame"], &["name", "color"]),
        "updateMarker" => (&["frame"], &["newFrame", "name", "color"]),
        "removeMarker" => (&["frame"], &[]),
        _ => return None,
    };
    let allowed = |key: &str| matches!(key, "command" | "contextId" | "kind") || required.contains(&key) || optional.contains(&key);
    if !object.keys().all(|key| allowed(key)) || !required.iter().all(|key| object.contains_key(*key)) {
        return None;
    }
    let get = |key: &str| object.get(key);
    // An optional member that is present must be well formed.
    macro_rules! opt {
        ($key:literal, $parse:expr) => {
            match get($key) {
                None => None,
                Some(v) => Some($parse(v)?),
            }
        };
    }
    let frame = || frame_of(get("frame")?);
    let edit = match kind {
        "setCameraKey" => StateEdit::SetCameraKey {
            frame: frame()?,
            x: opt!("x", number_of),
            y: opt!("y", number_of),
            w: opt!("w", number_of),
            rot: opt!("rot", number_of),
            h_out: opt!("hOut", array_of::<2>),
            h_in: opt!("hIn", array_of::<2>),
            ease: opt!("ease", array_of::<4>),
        },
        "removeCameraKey" => StateEdit::RemoveCameraKey { frame: frame()? },
        "setCameraEase" => StateEdit::SetCameraEase { frame: frame()?, ease: array_of::<4>(get("ease")?)? },
        "clearCamera" => StateEdit::ClearCamera,
        "addMarker" => StateEdit::AddMarker {
            frame: frame()?,
            name: opt!("name", |v: &serde_json::Value| v.as_str().map(str::to_owned)),
            color: opt!("color", |v: &serde_json::Value| v.as_str().map(str::to_owned)),
        },
        "updateMarker" => StateEdit::UpdateMarker {
            frame: frame()?,
            new_frame: opt!("newFrame", frame_of),
            name: opt!("name", |v: &serde_json::Value| v.as_str().map(str::to_owned)),
            color: opt!("color", |v: &serde_json::Value| v.as_str().map(str::to_owned)),
        },
        "removeMarker" => StateEdit::RemoveMarker { frame: frame()? },
        _ => StateEdit::ClearMarkers,
    };
    Some((context.to_string(), edit))
}

/// One `object.component.state.edit` request over the stored record of `symbol_id`.
pub(crate) fn plan_state_edit(
    document: &ObjectDocument,
    symbol_id: &str,
    edit: &StateEdit,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    if symbol_id == SCENE_CONTEXT {
        return Err(SetupError::NestedContext(symbol_id.to_string()));
    }
    let record = document
        .component_set()
        .and_then(|set| set.component(symbol_id))
        .ok_or_else(|| SetupError::UnknownComponent(symbol_id.to_string()))?;
    let total = record.total_frames();
    let mut next = record.clone();
    match edit {
        StateEdit::SetCameraKey { .. }
        | StateEdit::RemoveCameraKey { .. }
        | StateEdit::SetCameraEase { .. }
        | StateEdit::ClearCamera => camera_edit(&mut next, edit, total)?,
        _ => marker_edit(&mut next, edit, total)?,
    }
    // A list that was never authored and stays empty is still not authored (an empty list is a stored list with no key).
    if record.camera_keys.missing() && next.camera_keys.value().is_some_and(Vec::is_empty) {
        next.camera_keys = Presence::Missing;
    }
    if record.markers.missing() && next.markers.value().is_some_and(Vec::is_empty) {
        next.markers = Presence::Missing;
    }
    if next == *record {
        return Ok(SetupPlan::unchanged(document, Some(symbol_id)));
    }
    let mut candidate = document.clone();
    with_tables(&mut candidate, |tables| {
        let Presence::Present(set) = &mut tables.components else {
            return Err(SetupError::UnknownComponent(symbol_id.to_string()));
        };
        match set.components.iter_mut().find(|c| c.symbol_id == symbol_id) {
            Some(found) => {
                *found = next;
                Ok(())
            }
            None => Err(SetupError::UnknownComponent(symbol_id.to_string())),
        }
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: Some(symbol_id.to_string()),
        layer_uid: None,
        created: None,
    })
}
