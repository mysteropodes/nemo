//! Original ruler gestures prepare metadata on the existing sealed settings owner.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
enum Axis {
    #[serde(rename = "h")]
    Horizontal,
    #[serde(rename = "v")]
    Vertical,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Add {
        axis: Axis,
        position: f64,
    },
    Move {
        axis: Axis,
        ordinal: u64,
        position: f64,
    },
    Remove {
        axis: Axis,
        ordinal: u64,
    },
}

struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});

fn at(ordinal: u64, count: usize) -> Result<usize, Error> {
    if !(1..=9_007_199_254_740_991).contains(&ordinal) {
        return Err(Error::InvalidRequest);
    }
    usize::try_from(ordinal - 1)
        .ok()
        .filter(|index| *index < count)
        .ok_or(Error::NotFound)
}

fn change(before: &ProjectMetadata, action: Action) -> Result<ProjectMetadata, Error> {
    let (axis, ordinal, position) = match action {
        Action::Add { axis, position } => (axis, None, Some(position)),
        Action::Move {
            axis,
            ordinal,
            position,
        } => (axis, Some(ordinal), Some(position)),
        Action::Remove { axis, ordinal } => (axis, Some(ordinal), None),
    };
    if position.is_some_and(|value| !value.is_finite()) {
        return Err(Error::InvalidRequest);
    }
    if ordinal.is_some_and(|value| !(1..=9_007_199_254_740_991).contains(&value)) {
        return Err(Error::InvalidRequest);
    }
    let mut guides = before
        .guide_metadata()
        .map(|(h, v)| (h.map(<[f64]>::to_vec), v.map(<[f64]>::to_vec)))
        .unwrap_or_default();
    let values = match axis {
        Axis::Horizontal => &mut guides.0,
        Axis::Vertical => &mut guides.1,
    };
    match (ordinal, position) {
        // Only an explicit add materializes this axis. The other axis stays absent.
        (None, Some(position)) => values.get_or_insert_with(Vec::new).push(position),
        (Some(ordinal), position) => {
            let values = values.as_mut().ok_or(Error::NotFound)?;
            let index = at(ordinal, values.len())?;
            if let Some(position) = position {
                values[index] = position;
            } else {
                values.remove(index);
            }
        }
        (None, None) => return Err(Error::InvalidRequest),
    }
    before
        .clone()
        .with_guide_metadata(Some(guides))
        .map_err(|_| Error::InvalidRequest)
}

impl ObjectFillOwner {
    pub(crate) fn prepare_guide_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, Error> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        {
            let _ = bytes;
            return Err(Error::Unavailable);
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
            if payload.command != "object.project.guide.edit" {
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
