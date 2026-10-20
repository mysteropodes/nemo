//! Original Perspective gestures prepare one sealed settings/history transaction.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    SetEnabled { enabled: bool },
    SetMode { mode: String },
    Reset {},
    LockAll { locked: bool },
    Move { ordinal: u64, x: f64, y: f64 },
    Translate { dx: f64, dy: f64 },
}
struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});
fn valid(action: &Action) -> bool {
    match action {
        Action::SetMode { mode } => matches!(mode.as_str(), "1pt" | "2pt" | "3pt" | "fisheye"),
        Action::Move { ordinal, x, y } => {
            (1..=9_007_199_254_740_991).contains(ordinal) && x.is_finite() && y.is_finite()
        }
        Action::Translate { dx, dy } => dx.is_finite() && dy.is_finite(),
        _ => true,
    }
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
fn change(before: &ProjectMetadata, action: Action) -> Result<ProjectMetadata, Error> {
    let (mut enabled, source_mode, density, source) = before.perspective_metadata();
    let mut mode = source_mode.map(str::to_owned);
    let mut vps = source.map(|points| points.map(|points| points.to_vec()));
    if !matches!(&action, Action::SetEnabled { enabled: false }) {
        let defaults_mode = match &action {
            Action::SetMode { mode } => Some(mode.as_str()),
            _ => source_mode,
        };
        let empty = source.flatten().is_none_or(|points| points.is_empty());
        if empty || matches!(&action, Action::Reset {} | Action::SetMode { .. }) {
            vps = Some(Some(ObjectSnapshot::perspective_default_vps(
                before,
                defaults_mode,
            )?));
        }
    }
    match action {
        Action::SetEnabled { enabled: value } => enabled = Some(value),
        Action::SetMode { mode: value } => mode = Some(value),
        Action::Reset {} => {}
        Action::LockAll { locked } => {
            for point in vps
                .as_mut()
                .and_then(Option::as_mut)
                .ok_or(Error::Internal)?
            {
                point.2 = Some(locked);
            }
        }
        Action::Move { ordinal, x, y } => {
            let points = vps
                .as_mut()
                .and_then(Option::as_mut)
                .ok_or(Error::Internal)?;
            let index = usize::try_from(ordinal - 1)
                .ok()
                .filter(|index| *index < points.len())
                .ok_or(Error::NotFound)?;
            let point = &mut points[index];
            if point.2 != Some(true) {
                point.0 = x;
                point.1 = y;
            }
        }
        Action::Translate { dx, dy } => {
            let points = vps
                .as_mut()
                .and_then(Option::as_mut)
                .ok_or(Error::Internal)?;
            if points.len() < 2 {
                return Err(Error::NotFound);
            }
            for point in points {
                if point.2 == Some(true) {
                    continue;
                }
                let (x, y) = (point.0 + dx, point.1 + dy);
                if !x.is_finite() || !y.is_finite() {
                    return Err(Error::InvalidRequest);
                }
                point.0 = x;
                point.1 = y;
            }
        }
    }
    before
        .clone()
        .with_perspective_metadata(enabled, mode, density, vps)
        .map_err(|_| Error::InvalidRequest)
}
impl ObjectFillOwner {
    pub(crate) fn prepare_perspective_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, Error> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        {
            let _ = bytes;
            Err(Error::Unavailable)
        }
        #[cfg(all(feature = "history", feature = "project_media_codec"))]
        {
            if bytes.len() > 4096 {
                return Err(Error::InvalidRequest);
            }
            let shape: serde_json::Value =
                serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
            if !shape
                .get("action")
                .is_some_and(serde_json::Value::is_object)
            {
                return Err(Error::InvalidRequest);
            }
            let payload: Payload =
                serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
            if payload.command != "object.project.perspective.edit" || !valid(&payload.action) {
                return Err(Error::InvalidRequest);
            }
            let origin = self.current_snapshot();
            let project = origin.project().ok_or(Error::Unavailable)?;
            let before = project.project();
            let metadata = change(before, payload.action)?;
            let changed = &metadata != before;
            if changed {
                ObjectSnapshot::check_candidate_saveable(&project.with_metadata(metadata.clone()))?;
            }
            Ok(PreparedProjectSettings {
                origin,
                metadata,
                changed,
            })
        }
    }
}
