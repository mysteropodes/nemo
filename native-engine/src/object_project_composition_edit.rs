//! Small root-composition deltas through the existing sealed settings candidate.
//! No public command can submit a replacement array or mutate projected JS state.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, Presence, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    AddMarker {
        frame: f64,
        #[serde(default)] name: Presence<String>,
        #[serde(default)] color: Presence<String>,
    },
    UpdateMarker {
        ordinal: usize,
        #[serde(default)] frame: Presence<f64>,
        #[serde(default)] name: Presence<String>,
        #[serde(default)] color: Presence<String>,
    },
    RemoveMarker { ordinal: usize },
    ClearMarkers {},
    SetBpm {
        #[serde(default)] bpm: Presence<f64>,
        #[serde(default, rename = "bpmOffset")] offset: Presence<f64>,
        #[serde(default, rename = "bpmShow")] show: Presence<bool>,
    },
}
struct Payload { command: String, action: Action }
object_deserialize!(Payload { command: String, action: Action });

fn text_ok(value: &Presence<String>) -> bool {
    value.value().is_none_or(|s| s.len() <= 1024 && !s.contains('\0'))
}
fn valid(action: &Action) -> bool {
    match action {
        Action::AddMarker { frame, name, color } =>
            frame.is_finite() && text_ok(name) && text_ok(color),
        Action::UpdateMarker { ordinal, frame, name, color } =>
            (1..=4096).contains(ordinal)
                && frame.value().is_none_or(|v| v.is_finite())
                && text_ok(name) && text_ok(color),
        Action::RemoveMarker { ordinal } => (1..=4096).contains(ordinal),
        Action::SetBpm { bpm, offset, .. } =>
            bpm.value().is_none_or(|v| v.is_finite())
                && offset.value().is_none_or(|v| v.is_finite()),
        Action::ClearMarkers {} => true,
    }
}
fn selected<T: Clone>(value: &Presence<T>, before: Option<T>) -> Option<T> {
    value.value().cloned().or(before)
}
fn index(ordinal: usize, count: usize) -> Result<usize, Error> {
    ordinal.checked_sub(1).filter(|i| *i < count).ok_or(Error::NotFound)
}
fn change(before: &ProjectMetadata, action: Action) -> Result<ProjectMetadata, Error> {
    let (bpm, offset, show) = before.composition_bpm();
    if let Action::SetBpm { bpm: value, offset: delta, show: on } = &action {
        return before.clone().with_composition_bpm(
            selected(value, bpm), selected(delta, offset), selected(on, show),
        ).map_err(|_| Error::InvalidRequest);
    }
    let mut markers = before.composition_markers().unwrap_or_default().to_vec();
    match action {
        Action::AddMarker { frame, name, color } => {
            if !frame.is_finite() { return Err(Error::InvalidRequest); }
            if markers.iter().any(|m| m.0 == frame) { return Ok(before.clone()); }
            markers.push((frame, name.value().cloned(), color.value().cloned()));
            // This is the existing Add UI action's deliberate stable sort.
            // Storage/import/read never sort supplied marker arrays.
            markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        }
        Action::UpdateMarker { ordinal, frame, name, color } => {
            let at = index(ordinal, markers.len())?;
            if let Some(value) = frame.value() {
                if !value.is_finite() { return Err(Error::InvalidRequest); }
                markers[at].0 = *value;
            }
            if let Some(value) = name.value() { markers[at].1 = Some(value.clone()); }
            if let Some(value) = color.value() { markers[at].2 = Some(value.clone()); }
            // Drag completion sorts; renaming/colour changes leave order intact.
            if !frame.missing() { markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap()); }
        }
        Action::RemoveMarker { ordinal } => {
            let at = index(ordinal, markers.len())?;
            markers.remove(at);
        }
        Action::ClearMarkers {} => markers.clear(),
        Action::SetBpm { .. } => unreachable!(),
    }
    before.clone().with_composition_metadata(Some(markers), bpm, offset, show)
        .map_err(|_| Error::InvalidRequest)
}

impl ObjectFillOwner {
    pub(crate) fn prepare_composition_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, Error> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        { let _ = bytes; return Err(Error::Unavailable); }
        #[cfg(all(feature = "history", feature = "project_media_codec"))]
        {
            if bytes.len() > 4096 { return Err(Error::InvalidRequest); }
            let shape: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|_| Error::InvalidRequest)?;
            if !shape.get("action").is_some_and(serde_json::Value::is_object) {
                return Err(Error::InvalidRequest);
            }
            let payload: Payload = serde_json::from_slice(bytes)
                .map_err(|_| Error::InvalidRequest)?;
            if payload.command != "object.project.composition.edit" || !valid(&payload.action) {
                return Err(Error::InvalidRequest);
            }
            let origin = self.current_snapshot();
            let project = origin.project().ok_or(Error::Unavailable)?;
            let before = project.project();
            let metadata = change(before, payload.action)?;
            let changed = &metadata != before;
            if changed { ObjectSnapshot::check_candidate_saveable(&project.with_metadata(metadata.clone()))?; }
            Ok(PreparedProjectSettings { origin, metadata, changed })
        }
    }
}
