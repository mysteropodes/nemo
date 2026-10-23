//! The existing owner lends its actual successful preview to a bounded immutable query.
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::commands::{OpacityRequest, ResponseEnvelope};
use native_engine::object_snapshot::{
    ImageMeshReadFailure, ImageMeshReadProjection, ObjectSnapshot,
};
use serde_json::Value;

impl NativeObjectHost {
    fn image_mesh_projection(
        &self,
        snapshot: &ObjectSnapshot,
        view: &Value,
    ) -> Result<ImageMeshReadProjection, ImageMeshReadFailure> {
        use ImageMeshReadFailure::{Stale, Unpresented, Unsupported};
        let preview = &self.preview;
        if self.released.is_some()
            || preview.released
            || preview.blocked.is_some()
            || preview.pending.is_some()
            || preview.prepared.is_some()
        {
            return Err(Unpresented);
        }
        let receipt = preview.presented_mesh_view.as_ref().ok_or(Unpresented)?;
        let witness = preview.reference_cache.as_ref().ok_or(Unpresented)?;
        let identity = receipt.identity();
        let key = identity.evaluation_key();
        let work_id = format!("native-work-{}", identity.work_id().value());
        if receipt.status() != &native_engine::desktop_viewport::ViewportStatus::Presented {
            return Err(Unpresented);
        }
        if self.content_revision() != snapshot.content_revision()
            || witness.instance_id() != snapshot.instance_id()
            || witness.document_id() != snapshot.document_id()
            || witness.content_revision() != snapshot.content_revision()
            || witness.document_snapshot_id() != snapshot.snapshot_id()
            || key.document_snapshot_id() != snapshot.snapshot_id()
            || key.context_id() != witness.context_id()
            || key.frame() != witness.frame()
            || view["workId"].as_str() != Some(work_id.as_str())
            || view["viewGeneration"].as_u64() != Some(identity.view_generation().value())
            || view["frame"].as_u64() != Some(u64::from(witness.frame()))
            || view["contextId"].as_str() != Some(witness.context_id())
        {
            return Err(Stale);
        }
        let layer = view["layerUid"].as_str().ok_or(Unsupported)?;
        witness
            .image_mesh_read_projection(layer)?
            .with_presentation(work_id, identity.view_generation().value())
            .map_err(|_| Unsupported)
    }

    pub(crate) fn dispatch_image_mesh_view(
        &self,
        snapshot: &ObjectSnapshot,
        request: OpacityRequest,
    ) -> Result<ResponseEnvelope, String> {
        let wire =
            serde_json::to_value(&request).map_err(|_| "native image mesh selector is invalid")?;
        let projection = self.image_mesh_projection(snapshot, &wire["payload"]["view"]);
        snapshot
            .dispatch_with_image_mesh_view(request, projection.as_ref().map_err(|failure| *failure))
            .map_err(|_| "native image mesh read correlation is invalid".into())
    }
}

#[cfg(test)]
#[path = "native_image_mesh_read_public_tests.rs"]
mod tests;
