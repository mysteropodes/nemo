//! Sealed tween edit using the existing native object revision owner.
use super::ObjectFillOwner;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use crate::tween_kernels::{
    plan_tween, prepare_tween_resources, PreparedTweenResources, TweenRequest,
};
#[derive(Debug)]
pub(crate) struct PreparedTween {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    command: String,
    resources: Option<PreparedTweenResources>,
}
impl PreparedTween {
    pub(crate) fn command(&self) -> &str {
        &self.command
    }
}
impl ObjectFillOwner {
    pub(crate) fn prepare_tween_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedTween, DispatchErrorCode> {
        if bytes.len() > 262144 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let request: TweenRequest =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        let planned = plan_tween(
            origin.document(),
            &request,
            origin
                .project()
                .and_then(|p| p.project().custom_brush_presets()),
        )?;
        let (document, resources) = match planned {
            None => (None, None),
            Some(plan) => {
                let bitmap = |d: &ObjectDocument| {
                    d.objects.iter().any(|r| {
                        r.vector_ext
                            .value()
                            .is_some_and(|v| !v.bitmap_brush.missing())
                    })
                };
                if bitmap(origin.document()) || bitmap(&plan.document) || !plan.bakes.is_empty() {
                    (
                        None,
                        Some(prepare_tween_resources(&origin, plan.document, plan.bakes)?),
                    )
                } else {
                    (Some(plan.document), None)
                }
            }
        };
        Ok(PreparedTween {
            origin,
            resources,
            document,
            command: request.command,
        })
    }
    pub(crate) fn commit_tween(
        &mut self,
        candidate: PreparedTween,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if let Some(resources) = candidate.resources {
            return self
                .revisions
                .publish_tween_resources(resources)
                .map_err(|_| DispatchErrorCode::Unavailable);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}
