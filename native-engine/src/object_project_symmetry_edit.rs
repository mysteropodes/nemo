//! Original Draw symmetry gestures prepare one sealed settings/history transaction.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ProjectMetadata, SymmetrySettings};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    SetEnabled { enabled: bool },
    SetMode { mode: String },
    ResetGuide {},
    SetAxis { x1: f64, y1: f64, x2: f64, y2: f64 },
    SetRadialCenter { x: f64, y: f64 },
    SetSectors { sectors: f64 },
    SetExtend { extend: bool },
}
struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});
const MODES: [&str; 4] = ["y", "x", "free", "radial"];
fn valid(action: &Action) -> bool {
    match action {
        Action::SetMode { mode } => MODES.contains(&mode.as_str()),
        Action::SetAxis { x1, y1, x2, y2 } => [x1, y1, x2, y2].iter().all(|v| v.is_finite()),
        Action::SetRadialCenter { x, y } => x.is_finite() && y.is_finite(),
        Action::SetSectors { sectors } => sectors.is_finite(),
        _ => true,
    }
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
fn change(before: &ProjectMetadata, action: Action) -> Result<ProjectMetadata, Error> {
    let dimensions = before.canvas_dimensions();
    let mut next: SymmetrySettings = before.symmetry_metadata().clone();
    // setSymmetryMode and resetSymmetryGuide re-centre only the guide the mode uses.
    let reset = |next: &mut SymmetrySettings, mode: Option<&str>| {
        if mode == Some("radial") {
            next.radial_center = Some(Some(ObjectSnapshot::symmetry_default_center(dimensions)));
        } else {
            next.axis = Some(Some(ObjectSnapshot::symmetry_default_axis(dimensions, mode)));
        }
    };
    match action {
        Action::SetEnabled { enabled } => {
            next.enabled = Some(enabled);
            // The original Labs toggle ensures an axis (for the effective mode) and a centre on enable.
            if enabled {
                let mode = next.mode.clone().filter(|m| !m.is_empty());
                let mode = Some(mode.as_deref().unwrap_or("y"));
                if next.axis.flatten().is_none() {
                    next.axis = Some(Some(ObjectSnapshot::symmetry_default_axis(dimensions, mode)));
                }
                if next.radial_center.flatten().is_none() {
                    next.radial_center = Some(Some(ObjectSnapshot::symmetry_default_center(dimensions)));
                }
            }
        }
        Action::SetMode { mode } => {
            reset(&mut next, Some(&mode));
            next.mode = Some(mode);
        }
        Action::ResetGuide {} => {
            let mode = next.mode.clone();
            reset(&mut next, mode.as_deref());
        }
        Action::SetAxis { x1, y1, x2, y2 } => next.axis = Some(Some((x1, y1, x2, y2))),
        Action::SetRadialCenter { x, y } => next.radial_center = Some(Some((x, y))),
        // The original Labs slider stores Math.max(2, Math.min(24, value)).
        Action::SetSectors { sectors } => next.radial_sectors = Some(sectors.clamp(2., 24.)),
        Action::SetExtend { extend } => next.extend = Some(extend),
    }
    before
        .clone()
        .with_symmetry_metadata(next)
        .map_err(|_| Error::InvalidRequest)
}
impl ObjectFillOwner {
    pub(crate) fn prepare_symmetry_delta_json(
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
            if payload.command != "object.project.symmetry.edit" || !valid(&payload.action) {
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
