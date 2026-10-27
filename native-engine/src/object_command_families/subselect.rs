//! G21 intent family; the shared envelope owns revision checks, cancellation and request replay.
use super::CommandFamily;
use crate::request_receipts::DispatchErrorCode;
const OP_SUBSELECT_EDIT: &str = "command.document.object.subselect.edit";
pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "subselect",
    owns: |op| op == OP_SUBSELECT_EDIT,
    refusal: "Native subselect request refused.",
    request_limit: 4096,
    dispatch: |history, request| {
        let bytes =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_subselect_command(&bytes))
    },
};
