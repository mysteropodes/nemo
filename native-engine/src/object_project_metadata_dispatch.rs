//! Project-metadata read namespaces share the existing immutable snapshot admission.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};

impl ObjectSnapshot {
    pub(crate) fn dispatch_project_metadata_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        let failure = |code, message| Ok(self.read_failure(request, code, message));
        if request.operation == "query.document.object.project.composition.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_composition_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native composition metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.palette.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_palette_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native palette metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.guide.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_guide_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native guide metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.shadow.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_shadow_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native Shadow metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.perspective.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_perspective_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native Perspective metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.workspace.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_workspace_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native workspace metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.customBrushPreset.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_custom_brush_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native custom preset metadata is unavailable.",
            )
            .map(Some);
        }
        if request.operation == "query.document.object.project.comments.read" {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            return self.dispatch_comments_read(request).map(Some);
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return failure(
                DispatchErrorCode::Unavailable,
                "Native team comment metadata is unavailable.",
            )
            .map(Some);
        }
        Ok(None)
    }
}
