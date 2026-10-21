//! Original custom preset save/update/delete on the existing sealed settings owner.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, BrushPresetRecord};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Upsert {
        id: String,
        preset: BrushPresetRecord,
    },
    Delete {
        id: String,
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
impl ObjectFillOwner {
    pub(crate) fn prepare_custom_brush_delta_json(
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
            if payload.command != "object.project.customBrushPreset.edit" {
                return Err(Error::InvalidRequest);
            }
            let origin = self.current_snapshot();
            let project = origin.project().ok_or(Error::Unavailable)?;
            let before = project.project();
            let source = before.custom_brush_presets();
            let catalog = match payload.action {
                Action::Upsert { id, preset } => {
                    source.cloned().unwrap_or_default().with_upsert(id, preset)
                }
                Action::Delete { id } => {
                    // JS delete of an absent custom key is a semantic no-op.
                    if source.is_none_or(|catalog| !catalog.entries().iter().any(|v| v.0 == id)) {
                        let metadata = before.clone();
                        return Ok(PreparedProjectSettings {
                            origin,
                            metadata,
                            changed: false,
                        });
                    }
                    source.unwrap().clone().without_id(&id)
                }
            };
            let metadata = before
                .clone()
                .with_custom_brush_presets(Some(catalog))
                .map_err(|_| Error::InvalidRequest)?;
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
