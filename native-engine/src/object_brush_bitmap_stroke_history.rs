//! #2361: one bitmap-brush Draw stroke (anchor, companion and its baked PNG) on the original history/replay owner, as one
//! undo step. Host bytes never enter a DTO; the request carries the anchor record, so its bound is 64 KiB.
use crate::commands::BitmapStroke;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
};
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};

const STROKE: &str = "command.document.object.brush.bitmap.stroke";
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}
impl NativeObjectHistory {
    fn bitmap_stroke_args(
        &self,
        request: &OpacityRequest,
    ) -> Result<BitmapStroke, DispatchErrorCode> {
        if request.api_version != 2
            || request.operation != STROKE
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
            || serde_json::to_vec(request).map_or(true, |bytes| bytes.len() > 65_536)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        BitmapStroke::parse(
            &serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?,
        )
    }
    fn bitmap_stroke_origin(
        &self,
        request: &OpacityRequest,
        args: &BitmapStroke,
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
        self.owner.bitmap_stroke_preflight(args)?;
        Ok(())
    }
    fn retain_bitmap_stroke_outcome(
        &mut self,
        request: &OpacityRequest,
        outcome: Result<(), DispatchErrorCode>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome
                .map(|()| CompactResult::Command {
                    applied: true,
                    history_entries_added: 1,
                })
                .map_err(|code| CompactFailure {
                    code,
                    message: "Native bitmap brush stroke refused.".into(),
                    details: None,
                }),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)
    }
    /// Terminal preflight failures never reserve or replace a request-id receipt. In particular,
    /// foreign owners and changed bodies must not poison or overwrite the genuine retained answer.
    fn bitmap_stroke_unretained_failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        // Never echo a malformed/unbounded request identity into a transport response.
        if !identifier(&request.request_id) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: Err(CompactFailure {
                code,
                message: "Native bitmap brush stroke refused.".into(),
                details: None,
            }),
        }
        .response(request, self.instance_id(), self.document_id())
        .filter(|response| serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() <= 4096))
        .map(Some)
        .ok_or(DispatchErrorCode::InvalidRequest)
    }
    /// None grants only source resolution. Host repeats its actual lifecycle/close checks on commit.
    /// Retained results are returned before source access, including after undo or file modification.
    pub fn bitmap_stroke_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        let args = match self.bitmap_stroke_args(request) {
            Ok(args) => args,
            Err(code) => return self.bitmap_stroke_unretained_failure(request, code),
        };
        let Some(fingerprint) = RequestFingerprint::new(request) else {
            return self
                .bitmap_stroke_unretained_failure(request, DispatchErrorCode::InvalidRequest);
        };
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(value) => {
                return value
                    .response(request, self.instance_id(), self.document_id())
                    .map(Some)
                    .ok_or(DispatchErrorCode::Internal)
            }
            ReceiptLookup::ChangedBody => {
                return self
                    .bitmap_stroke_unretained_failure(request, DispatchErrorCode::InvalidRequest)
            }
            ReceiptLookup::Missing => {}
        }
        if let Err(error) = self.bitmap_stroke_origin(request, &args) {
            return self
                .retain_bitmap_stroke_outcome(request, Err(error))
                .map(Some);
        }
        Ok(None)
    }
    pub fn dispatch_bitmap_stroke(
        &mut self,
        request: &OpacityRequest,
        bytes: &[u8],
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        self.dispatch_bitmap_stroke_cancellable(request, bytes, &|| false)
    }
    pub fn dispatch_bitmap_stroke_cancellable(
        &mut self,
        request: &OpacityRequest,
        bytes: &[u8],
        cancel: &dyn Fn() -> bool,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let Some(response) = self.bitmap_stroke_status(request)? {
            return Ok(response);
        }
        let args = self.bitmap_stroke_args(request)?;
        let outcome = (|| {
            self.bitmap_stroke_origin(request, &args)?;
            if cancel() {
                return Err(DispatchErrorCode::CancelledBeforeDispatch);
            }
            let prepared = self
                .owner
                .prepare_bitmap_stroke(&request.request_id, &args, bytes)?;
            if cancel() {
                return Err(DispatchErrorCode::CancelledBeforeDispatch);
            }
            let before_revision = self.content_revision();
            let after_revision = self.owner.commit_bitmap_stroke(prepared)?;
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
            Ok(())
        })();
        self.retain_bitmap_stroke_outcome(request, outcome)
    }
}

#[cfg(test)]
#[path = "../tests/object_brush_bitmap_stroke.rs"]
mod tests;
