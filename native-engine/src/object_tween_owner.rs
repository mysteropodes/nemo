//! Sealed tween edit using the existing native object revision owner.
use super::ObjectFillOwner;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use crate::tween_kernels::{plan_tween, TweenRequest};
#[derive(Debug)]
pub(crate) struct PreparedTween {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    command: String,
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
        let document = plan_tween(
            origin.document(),
            &request,
            origin
                .project()
                .and_then(|p| p.project().custom_brush_presets()),
        )?;
        Ok(PreparedTween {
            origin,
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
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}
