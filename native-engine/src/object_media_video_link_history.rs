//! T12: atomic linked-video import/replace on the original history/replay owner.
//! Phase one (`video_link_status`) replays or refuses before the host touches the file;
//! phase two (`dispatch_video_link*`) commits the host-probed facts as one undo entry.
use crate::commands::{LinkedVideoFacts, VideoLink, VideoLinkResult};
use crate::history::object_history::image_import::identifier;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
};
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};

pub(crate) const OP_VIDEO_LINK: &str = "command.document.object.asset.video.link";
const REFUSED: &str = "Native video link refused.";

fn failure(code: DispatchErrorCode) -> CompactFailure {
    CompactFailure {
        code,
        message: REFUSED.into(),
        details: None,
    }
}

impl NativeObjectHistory {
    fn video_args(&self, request: &OpacityRequest) -> Result<VideoLink, DispatchErrorCode> {
        if request.api_version != 2
            || request.operation != OP_VIDEO_LINK
            || request
                .expected_revision
                .is_none_or(|value| value > 9_007_199_254_740_991)
            || [
                &request.request_id,
                &request.instance_id,
                &request.document_id,
            ]
            .iter()
            .any(|value| !identifier(value))
            || serde_json::to_vec(request).map_or(true, |bytes| bytes.len() > 4096)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        VideoLink::parse(
            &serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?,
        )
    }
    fn video_origin(
        &self,
        request: &OpacityRequest,
        args: &VideoLink,
    ) -> Result<(), DispatchErrorCode> {
        if request.cancelled_before_dispatch {
            return Err(DispatchErrorCode::CancelledBeforeDispatch);
        }
        if request.expected_revision != Some(self.content_revision()) {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let origin = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        if origin.snapshot_id() != args.document_snapshot_id {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.owner.video_link_preflight(args)
    }
    fn retain_video_outcome(
        &mut self,
        request: &OpacityRequest,
        outcome: Result<VideoLinkResult, DispatchErrorCode>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome.map(CompactResult::VideoLinked).map_err(failure),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)
    }
    /// Terminal preflight failures never reserve or replace a request-id receipt.
    fn video_unretained_failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        if !identifier(&request.request_id) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: Err(failure(code)),
        }
        .response(request, self.instance_id(), self.document_id())
        .filter(|response| serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() <= 4096))
        .map(Some)
        .ok_or(DispatchErrorCode::InvalidRequest)
    }
    /// `None` grants only the host probe; the commit repeats every check. A retained answer
    /// (including after undo or a changed file) is returned before any file access.
    pub fn video_link_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        let args = match self.video_args(request) {
            Ok(args) => args,
            Err(code) => return self.video_unretained_failure(request, code),
        };
        let Some(fingerprint) = RequestFingerprint::new(request) else {
            return self.video_unretained_failure(request, DispatchErrorCode::InvalidRequest);
        };
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(value) => {
                return value
                    .response(request, self.instance_id(), self.document_id())
                    .map(Some)
                    .ok_or(DispatchErrorCode::Internal)
            }
            ReceiptLookup::ChangedBody => {
                return self.video_unretained_failure(request, DispatchErrorCode::InvalidRequest)
            }
            ReceiptLookup::Missing => {}
        }
        if let Err(error) = self.video_origin(request, &args) {
            return self.retain_video_outcome(request, Err(error)).map(Some);
        }
        Ok(None)
    }
    pub fn dispatch_video_link(
        &mut self,
        request: &OpacityRequest,
        facts: &LinkedVideoFacts,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        self.dispatch_video_link_cancellable(request, facts, &|| false)
    }
    pub fn dispatch_video_link_cancellable(
        &mut self,
        request: &OpacityRequest,
        facts: &LinkedVideoFacts,
        cancel: &dyn Fn() -> bool,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let Some(response) = self.video_link_status(request)? {
            return Ok(response);
        }
        let args = self.video_args(request)?;
        let outcome = (|| {
            self.video_origin(request, &args)?;
            if cancel() {
                return Err(DispatchErrorCode::CancelledBeforeDispatch);
            }
            let prepared = self
                .owner
                .prepare_video_link(&request.request_id, &args, facts)?;
            if cancel() {
                return Err(DispatchErrorCode::CancelledBeforeDispatch);
            }
            let result = prepared.result().clone();
            let before_revision = self.content_revision();
            let after_revision = self.owner.commit_video_link(prepared)?;
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
            Ok(result)
        })();
        self.retain_video_outcome(request, outcome)
    }
}

#[cfg(test)]
#[path = "../tests/object_media_video_link.rs"]
mod tests;
