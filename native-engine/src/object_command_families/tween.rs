//! Native tween command family; shared envelope/replay/revision checks run before dispatch.
//! nemo-command-feature: application
use super::CommandFamily;
use crate::request_receipts::DispatchErrorCode;
const OP_TWEEN_GENERATE: &str = "command.document.object.tween.generate";
const OP_TWEEN_REGENERATE: &str = "command.document.object.tween.regenerate";
const OP_TWEEN_SPLIT: &str = "command.document.object.tween.split";
const OP_TWEEN_REASSIGN: &str = "command.document.object.tween.reassign";
pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "tween",
    owns: |op| {
        matches!(
            op,
            OP_TWEEN_GENERATE | OP_TWEEN_REGENERATE | OP_TWEEN_SPLIT | OP_TWEEN_REASSIGN
        )
    },
    refusal: "Native tween request refused.",
    request_limit: crate::tween_kernels::limits::REQUEST_BYTES,
    dispatch: |history, request| {
        let bytes =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_tween_command(&request.operation, &bytes))
    },
};
