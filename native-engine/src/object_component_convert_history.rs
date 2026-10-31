//! Lane-4 setup / convert operations are single transactions in the existing
//! object history (L04-M): one successful convert / expose / place / duplicator
//! setup is one undo entry and one monotonic revision; a plan that changes nothing
//! leaves presence, bytes, undo and redo untouched. A merge convert removes the
//! source layers, adds the instance and the component record in the SAME entry,
//! never a second one. The operation names are staged: the public capability
//! catalog stays closed.
use crate::commands::{PreparedComponentSetup, SetupKind, SetupRefusal};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_COMPONENT_CONVERT: &str = "command.document.object.component.convert";
const OP_COMPONENT_EXPOSE: &str = "command.document.object.component.expose";
const OP_COMPONENT_PLACE: &str = "command.document.object.component.place";
const OP_DUPLICATOR_SET: &str = "command.document.object.duplicator.set";
const OP_DUPLICATOR_OBJECTS: &str = "command.document.object.duplicator.objects";
const OP_DUPLICATOR_PATH: &str = "command.document.object.duplicator.path";
const OP_DUPLICATOR_CONFIG: &str = "command.document.object.duplicator.config";

fn kind_of(operation: &str) -> Option<SetupKind> {
    match operation {
        OP_COMPONENT_CONVERT => Some(SetupKind::Convert),
        OP_COMPONENT_EXPOSE => Some(SetupKind::Expose),
        OP_COMPONENT_PLACE => Some(SetupKind::Place),
        OP_DUPLICATOR_SET => Some(SetupKind::DuplicatorSet),
        OP_DUPLICATOR_OBJECTS => Some(SetupKind::DuplicatorObjects),
        OP_DUPLICATOR_PATH => Some(SetupKind::DuplicatorPath),
        OP_DUPLICATOR_CONFIG => Some(SetupKind::DuplicatorConfig),
        _ => None,
    }
}

impl NativeObjectHistory {
    pub(crate) fn prepare_component_setup_json(
        &self,
        kind: SetupKind,
        bytes: &[u8],
    ) -> Result<PreparedComponentSetup, SetupRefusal> {
        self.owner.prepare_component_setup_json(kind, bytes)
    }
    pub(crate) fn commit_component_setup(
        &mut self,
        candidate: PreparedComponentSetup,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_component_setup(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Operations the staged dispatcher routes here.
    pub(crate) fn component_setup_operation(operation: &str) -> bool {
        kind_of(operation).is_some()
    }
    /// Prepare and commit one setup for the dispatcher, which has already checked
    /// the envelope, owner pins, cancellation, revision and request replay. A
    /// plan that created a layer reports its identity.
    pub(crate) fn dispatch_component_setup(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let kind = kind_of(operation).ok_or((
            DispatchErrorCode::InvalidRequest,
            "Component setup operation is unknown.",
        ))?;
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Component setup payload is invalid.",
            )
        })?;
        let candidate = self
            .prepare_component_setup_json(kind, &bytes)
            .map_err(|refusal| (refusal.code, refusal.message))?;
        let created = candidate.created().map(str::to_owned);
        let before = self.content_revision();
        let after = self
            .commit_component_setup(candidate)
            .map_err(|code| (code, "Component setup could not commit."))?;
        Ok(match created {
            Some(layer_uid) if after != before => CompactResult::LayerCreated { layer_uid },
            _ => CompactResult::Command {
                applied: after != before,
                history_entries_added: u64::from(after != before),
            },
        })
    }
}

#[cfg(test)]
#[path = "../tests/object_component_convert_dispatch.rs"]
mod dispatch_tests;
#[cfg(test)]
#[path = "../tests/object_duplicator_config_history.rs"]
mod config_history_tests;
#[cfg(test)]
#[path = "../tests/object_component_convert_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_component_convert_history.rs"]
mod history_tests;
