//! Layer kind creation family (#2371 U11 / D36): Null, Effect, Guide, Folder and Effector layers as one layer-create
//! transaction (the layer-create candidate with the kind layer, and the layers it adopts, in one revision).
use super::{CommandFamily, DEFAULT_REQUEST_BYTES};
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_LAYER_KIND_CREATE: &str = "command.document.object.layer.kind.create";

pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "layer_kind",
    owns: |operation| operation == OP_LAYER_KIND_CREATE,
    refusal: "Layer kind creation request refused.",
    request_limit: DEFAULT_REQUEST_BYTES,
    dispatch: |history, request| {
        let payload =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_layer_kind_create(&payload))
    },
};

impl NativeObjectHistory {
    /// The shared envelope, replay and revision checks already ran: prepare the one candidate, commit it.
    fn dispatch_layer_kind_create(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let candidate = self
            .owner
            .prepare_layer_kind_create_json(payload)
            .map_err(|code| {
                (
                    code,
                    "Layer kind payload or target is invalid or unavailable.",
                )
            })?;
        let layer_uid = candidate.layer_uid().to_owned();
        self.commit_layer_create(candidate)
            .map(|_| CompactResult::LayerCreated { layer_uid })
            .map_err(|code| (code, "Layer kind creation could not commit."))
    }
}
