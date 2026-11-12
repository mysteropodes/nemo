//! Admission ceilings bound native planning work; overflow is an atomic typed refusal.
pub(crate) const REQUEST_BYTES: usize = 262_144;
pub(crate) const STROKES_PER_FRAME: usize = 256;
pub(crate) const SPANS_PER_REQUEST: usize = 64;
pub(crate) const DABS_PER_PAIR: usize = 4096;
pub(crate) const BAKES_PER_COMMAND: usize = crate::object_document::tween::MAX_TWEEN_BAKES;

pub(super) fn require(
    count: usize,
    maximum: usize,
) -> Result<(), crate::request_receipts::DispatchErrorCode> {
    if count > maximum {
        Err(crate::request_receipts::DispatchErrorCode::Unavailable)
    } else {
        Ok(())
    }
}
