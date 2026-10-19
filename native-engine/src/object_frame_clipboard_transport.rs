//! F12 per-message budgets, never a clipboard or lifetime-retention owner.
//! Copy's existing data ceiling counts serialized records and complete order arrays.
//! Full envelopes additionally reserve 256 bytes per cell (4096 cells) plus 4096
//! header bytes. Per-cell fixed JSON is <128 bytes at two u32 maxima. The remaining
//! >=524288 bytes cover record separators: each admitted native record is >64
//! bytes, hence at most65536 separators under the4MiB data ceiling. The final
//! 4096 reserve covers bounded outer/source identities and command/result fields.
//! Complete bytes are ALWAYS checked too; these reserves are not a global lift.
use serde::Serialize;
pub(crate) const DATA_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MESSAGE_BYTES: usize = DATA_BYTES + 4096 * 256 + 4096;
pub(crate) fn owns(operation: &str) -> bool {
    matches!(
        operation,
        "command.document.object.frames.clipboard.copy"
            | "command.document.object.frames.clipboard.cut"
            | "command.document.object.frames.clipboard.paste"
    )
}
pub(crate) fn request_limit(operation: &str) -> usize {
    if owns(operation) {
        MESSAGE_BYTES
    } else {
        4096
    }
}
pub(crate) fn response_limit(operation: &str) -> usize {
    if matches!(
        operation,
        "command.document.object.frames.clipboard.copy"
            | "command.document.object.frames.clipboard.cut"
    ) {
        MESSAGE_BYTES
    } else {
        4096
    }
}
pub(crate) fn charge(total: &mut usize, value: &impl Serialize) -> bool {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return false;
    };
    let Some(next) = total
        .checked_add(bytes.len())
        .filter(|next| *next <= DATA_BYTES)
    else {
        return false;
    };
    *total = next;
    true
}
