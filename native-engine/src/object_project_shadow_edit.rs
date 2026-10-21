//! Original Shadow swatch gestures use the existing sealed settings/history owner.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Add { id: String, color: String },
    Select { ordinal: u64 },
}
struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});
fn text(value: &str) -> bool {
    value.len() <= 1024 && !value.contains('\0')
}
fn change(before: &ProjectMetadata, action: Action) -> Result<ProjectMetadata, Error> {
    match &action {
        Action::Add { id, color } if !text(id) || !text(color) => {
            return Err(Error::InvalidRequest);
        }
        Action::Select { ordinal } if !(1..=9_007_199_254_740_991).contains(ordinal) => {
            return Err(Error::InvalidRequest);
        }
        _ => {}
    }
    let (source, _) = before.shadow_palette_metadata();
    let mut palette = source.unwrap_or_default().to_vec();
    if palette.is_empty() {
        // The original palette() presents these six for absent AND empty arrays.
        // Materialize them only for an explicit gesture, never during a read.
        palette = [
            ("sh1", "#ff3355"),
            ("sh2", "#ff8800"),
            ("sh3", "#ffdd00"),
            ("sh4", "#22cc55"),
            ("sh5", "#2288ff"),
            ("sh6", "#aa33ff"),
        ]
        .into_iter()
        .map(|(id, color)| (id.to_owned(), color.to_owned()))
        .collect();
    }
    let active_id = match action {
        Action::Add { id, color } => {
            // Add and select are one original UI gesture and one history entry.
            palette.push((id.clone(), color));
            id
        }
        Action::Select { ordinal } => {
            let index = usize::try_from(ordinal - 1)
                .ok()
                .filter(|index| *index < palette.len())
                .ok_or(Error::NotFound)?;
            palette[index].0.clone()
        }
    };
    before
        .clone()
        .with_shadow_palette_metadata(Some(palette), Some(active_id))
        .map_err(|_| Error::InvalidRequest)
}
impl ObjectFillOwner {
    pub(crate) fn prepare_shadow_delta_json(
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
            if payload.command != "object.project.shadow.edit" {
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
