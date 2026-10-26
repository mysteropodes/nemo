//! Widget command family: joystick/slider layer create, write-from-local and settings.
use super::{CommandFamily, FamilyOutcome, DEFAULT_REQUEST_BYTES};
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_WIDGET_CREATE: &str = "command.document.object.widget.create";
const OP_WIDGET_WRITE: &str = "command.document.object.widget.write";
const OP_WIDGET_SETTINGS: &str = "command.document.object.widget.settings";

pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "widget",
    owns: |operation| {
        matches!(
            operation,
            OP_WIDGET_CREATE | OP_WIDGET_WRITE | OP_WIDGET_SETTINGS
        )
    },
    refusal: "Widget request refused.",
    request_limit: DEFAULT_REQUEST_BYTES,
    dispatch: |history, request| {
        let payload =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_widget_command(request.operation.as_str(), &payload))
    },
};

impl NativeObjectHistory {
    /// Widget creation, write-from-local and settings: the common envelope, replay and revision checks
    /// already ran, so only the typed preparation and the sealed commit remain.
    fn dispatch_widget_command(&mut self, operation: &str, payload: &[u8]) -> FamilyOutcome {
        const REFUSED: &str = "Widget payload or target is invalid or unavailable.";
        if operation == OP_WIDGET_CREATE {
            let candidate = self
                .prepare_widget_create_json(payload)
                .map_err(|code| (code, REFUSED))?;
            let layer_uid = candidate.layer_uid().to_owned();
            return self
                .commit_layer_create(candidate)
                .map(|_| CompactResult::LayerCreated { layer_uid })
                .map_err(|code| (code, "Widget layer creation could not commit."));
        }
        let candidate = if operation == OP_WIDGET_WRITE {
            self.prepare_widget_write_json(payload)
        } else {
            self.prepare_widget_settings_json(payload)
        }
        .map_err(|code| (code, REFUSED))?;
        let before = self.content_revision();
        self.commit_widget_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Widget edit could not commit."))
    }
}
