//! Public expression authoring intents resolved onto the sealed `SetExpression` edit.
//! The frozen inline editor commits code and the enable checkbox independently; an empty
//! string stays present-but-empty and only an explicit remove clears the expression.
use crate::object_document::OrderExpression;
use crate::request_receipts::DispatchErrorCode;

/// Same ceiling as the evaluator input (4096 bytes); oversize is refused, never truncated.
pub(super) const MAX_CODE_BYTES: usize = 4096;

/// Replace the code; the enabled state is kept, or starts disabled for a new expression.
pub(super) fn set_code(
    current: Option<&OrderExpression>,
    code: String,
) -> Result<Option<OrderExpression>, DispatchErrorCode> {
    if code.len() > MAX_CODE_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(Some(OrderExpression {
        enabled: current.is_some_and(|expression| expression.enabled),
        code,
    }))
}

/// Flip enablement and keep the code. Disabling an absent expression is a no-op.
pub(super) fn set_enabled(
    current: Option<&OrderExpression>,
    enabled: bool,
) -> Option<OrderExpression> {
    match current {
        None if !enabled => None,
        _ => Some(OrderExpression {
            enabled,
            code: current.map(|e| e.code.clone()).unwrap_or_default(),
        }),
    }
}
