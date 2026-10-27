//! Retained tween ports and application error translation.
use crate::application::{DispatchErrorCode, ExportJobErrorKind, ResourceResolutionErrorKind};
#[path = "tween_ghost_query.rs"]
mod ghost;
pub use ghost::{
    TweenGhostFrame, TweenGhostKind, TweenGhostPath, TweenGhostRead, TweenGhostVisual,
};

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
