//! Retained tween ports and application error translation.
use crate::application::{DispatchErrorCode, ExportJobErrorKind, ResourceResolutionErrorKind};
#[path = "tween_ghost_query.rs"]
mod ghost;
pub use ghost::{
    TweenGhostFrame, TweenGhostKind, TweenGhostPath, TweenGhostRead, TweenGhostVisual,
};
// #2358 GHOST-UNDER: the retained ghost query as the viewport's onion underlay source.
#[path = "tween_ghost_underlay.rs"]
mod ghost_underlay;
pub use ghost_underlay::{prepare_ghost_underlay, release_ghost_caches};
#[cfg(test)]
pub(crate) use ghost_underlay::prepared_frames;

pub(super) fn resolution_code(kind: ResourceResolutionErrorKind) -> DispatchErrorCode {
    match kind {
        ResourceResolutionErrorKind::InvalidRequest => DispatchErrorCode::InvalidRequest,
        ResourceResolutionErrorKind::Unavailable => DispatchErrorCode::Unavailable,
        ResourceResolutionErrorKind::NotFound => DispatchErrorCode::NotFound,
        ResourceResolutionErrorKind::Internal => DispatchErrorCode::Internal,
    }
}

pub(super) fn export_error_code(kind: ExportJobErrorKind) -> DispatchErrorCode {
    match kind {
        ExportJobErrorKind::InvalidRequest | ExportJobErrorKind::ChangedRequestId => {
            DispatchErrorCode::InvalidRequest
        }
        ExportJobErrorKind::Busy => DispatchErrorCode::BusyConflict,
        ExportJobErrorKind::NotFound => DispatchErrorCode::NotFound,
        ExportJobErrorKind::WrongDocument => DispatchErrorCode::WrongDocument,
        ExportJobErrorKind::Released => DispatchErrorCode::Unavailable,
    }
}
